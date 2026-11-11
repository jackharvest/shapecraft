package com.jackharvest.shapecraft;

import android.app.NativeActivity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.OpenableColumns;
import android.webkit.MimeTypeMap;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayDeque;
import java.util.Arrays;

/**
 * ShapeCraft's activity: the NativeActivity that runs libshapecraft.so, plus the system's file picker (#7), so
 * Open, Import and Export reach Files, Drive, Dropbox, USB storage and anything else Android offers.
 *
 * The app asks through a mailbox of small files instead of calls into Java: it writes a request into
 * files/.mailbox/to-android ("open", its id, a label; or "save", a file it wrote, the name to offer), this activity
 * shows the picker, copies the picked document into the cache (or the written file to where the user chose) and
 * writes the outcome into files/.mailbox/to-app ("picked", id, path; "saved", name; "cancelled"; "failed", why).
 */
public class ShapeCraftActivity extends NativeActivity {
    private static final int OPEN = 1;
    private static final int SAVE = 2;
    private static final long POLL_MS = 200;

    private final Handler handler = new Handler(Looper.getMainLooper());
    private final ArrayDeque<String[]> queue = new ArrayDeque<>();
    /** The request the picker is showing for, or null. */
    private String[] showing;
    private File toAndroid;
    private File toApp;

    private final Runnable poll = new Runnable() {
        @Override
        public void run() {
            readRequests();
            showNext();
            handler.postDelayed(this, POLL_MS);
        }
    };

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        File box = new File(getFilesDir(), ".mailbox");
        toAndroid = new File(box, "to-android");
        toApp = new File(box, "to-app");
        toAndroid.mkdirs();
        toApp.mkdirs();
        // Copies picked in an earlier run were opened then; they're done with.
        deleteTree(new File(getCacheDir(), "picked"));
    }

    @Override
    protected void onResume() {
        super.onResume();
        handler.removeCallbacks(poll);
        handler.post(poll);
    }

    @Override
    protected void onPause() {
        handler.removeCallbacks(poll);
        super.onPause();
    }

    /** Takes the app's new requests, oldest first. */
    private void readRequests() {
        File[] files = toAndroid.listFiles((dir, name) -> name.endsWith(".req"));
        if (files == null || files.length == 0) {
            return;
        }
        Arrays.sort(files);
        for (File f : files) {
            try {
                queue.add(new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8).split("\n", -1));
            } catch (IOException e) {
                // Unreadable: dropped below.
            }
            f.delete();
        }
    }

    /** Shows the picker for the next request once the last one is answered. */
    private void showNext() {
        if (showing != null || queue.isEmpty()) {
            return;
        }
        String[] req = queue.poll();
        Intent intent;
        int code;
        if (req.length >= 2 && req[0].equals("open")) {
            intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            // CAD formats mostly have no MIME type of their own: everything stays pickable.
            intent.setType("*/*");
            code = OPEN;
        } else if (req.length >= 3 && req[0].equals("save")) {
            intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            intent.setType(mimeOf(req[2]));
            intent.putExtra(Intent.EXTRA_TITLE, req[2]);
            code = SAVE;
        } else {
            return;
        }
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        try {
            showing = req;
            startActivityForResult(intent, code);
        } catch (RuntimeException e) {
            showing = null;
            answer("failed", "No file picker on this device");
        }
    }

    @Override
    protected void onActivityResult(int code, int result, Intent data) {
        super.onActivityResult(code, result, data);
        final String[] req = showing;
        showing = null;
        if (req == null) {
            return;
        }
        final Uri uri = result == RESULT_OK && data != null ? data.getData() : null;
        if (uri == null) {
            if (code == SAVE) {
                new File(req[1]).delete();
            }
            answer("cancelled", code == OPEN ? req[1] : req[2]);
            return;
        }
        // Copies can be big (a STEP file of a whole machine): off the main thread.
        new Thread(() -> {
            if (code == OPEN) {
                copyIn(uri, req[1]);
            } else {
                copyOut(new File(req[1]), uri, req[2]);
            }
        }).start();
    }

    /** The picked document into the cache, under its own name, for the app to open. */
    private void copyIn(Uri uri, String id) {
        String name = safeName(displayName(uri));
        File dir = new File(getCacheDir(), "picked/" + System.nanoTime());
        dir.mkdirs();
        File to = new File(dir, name);
        try (InputStream in = getContentResolver().openInputStream(uri); OutputStream out = new FileOutputStream(to)) {
            if (in == null) {
                throw new IOException("the document couldn't be read");
            }
            copy(in, out);
            answer("picked", id, to.getAbsolutePath());
        } catch (IOException | RuntimeException e) {
            answer("failed", "Couldn't open " + name + ": " + e.getMessage());
        }
    }

    /** A file the app wrote, to where the user chose. */
    private void copyOut(File from, Uri uri, String name) {
        try (InputStream in = new FileInputStream(from); OutputStream out = getContentResolver().openOutputStream(uri, "wt")) {
            if (out == null) {
                throw new IOException("the place chosen couldn't be written");
            }
            copy(in, out);
            answer("saved", name);
        } catch (IOException | RuntimeException e) {
            answer("failed", "Couldn't save " + name + ": " + e.getMessage());
        }
        from.delete();
    }

    private String displayName(Uri uri) {
        try (Cursor c = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst() && !c.isNull(0)) {
                return c.getString(0);
            }
        } catch (RuntimeException e) {
            // Some providers can't say: fall back to the URI's last part.
        }
        String last = uri.getLastPathSegment();
        return last != null ? last : "picked";
    }

    /** A name that stays one file name: no folders, nothing hidden, not empty. */
    private static String safeName(String name) {
        String n = name.replace('/', '_').replace('\\', '_').replace('\0', '_').trim();
        while (n.startsWith(".")) {
            n = n.substring(1);
        }
        return n.isEmpty() ? "picked" : n;
    }

    private static String mimeOf(String name) {
        int dot = name.lastIndexOf('.');
        String ext = dot >= 0 ? name.substring(dot + 1).toLowerCase() : "";
        String mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext);
        return mime != null ? mime : "application/octet-stream";
    }

    private static void copy(InputStream in, OutputStream out) throws IOException {
        byte[] buf = new byte[1 << 16];
        int n;
        while ((n = in.read(buf)) > 0) {
            out.write(buf, 0, n);
        }
    }

    /** One answer for the app, written whole before it shows (a temporary name, then renamed). */
    private synchronized void answer(String... lines) {
        String stamp = String.format("%020d", System.nanoTime());
        File tmp = new File(toApp, stamp + ".tmp");
        try {
            Files.write(tmp.toPath(), String.join("\n", lines).getBytes(StandardCharsets.UTF_8));
            tmp.renameTo(new File(toApp, stamp + ".res"));
        } catch (IOException e) {
            tmp.delete();
        }
    }

    private static void deleteTree(File f) {
        File[] kids = f.listFiles();
        if (kids != null) {
            for (File k : kids) {
                deleteTree(k);
            }
        }
        f.delete();
    }
}
