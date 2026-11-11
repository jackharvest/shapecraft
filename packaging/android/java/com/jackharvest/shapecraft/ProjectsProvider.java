package com.jackharvest.shapecraft;

import android.database.Cursor;
import android.database.MatrixCursor;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsContract.Document;
import android.provider.DocumentsContract.Root;
import android.provider.DocumentsProvider;
import android.webkit.MimeTypeMap;

import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;

/**
 * ShapeCraft's projects folder in the Files app and every app's file picker (#7): "ShapeCraft" shows up beside
 * Downloads and Drive, so projects and exports can be copied out, backed up or shared, and files copied in.
 *
 * A document's id is "p:" and its path below the projects folder (files/ShapeCraft); nothing outside it can be
 * named. Names starting with a dot (the app's own bookkeeping) are left out.
 */
public class ProjectsProvider extends DocumentsProvider {
    private static final String ROOT_DOC = "p:";
    private static final String[] ROOT_COLUMNS = {
        Root.COLUMN_ROOT_ID, Root.COLUMN_FLAGS, Root.COLUMN_TITLE, Root.COLUMN_SUMMARY, Root.COLUMN_DOCUMENT_ID, Root.COLUMN_ICON,
    };
    private static final String[] DOC_COLUMNS = {
        Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE, Document.COLUMN_FLAGS,
        Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED,
    };

    @Override
    public boolean onCreate() {
        return true;
    }

    private File base() throws FileNotFoundException {
        if (getContext() == null) {
            throw new FileNotFoundException("no context");
        }
        File base = new File(getContext().getFilesDir(), "ShapeCraft");
        base.mkdirs();
        return base;
    }

    /** The file an id names, refused if it would lead outside the projects folder. */
    private File file(String id) throws FileNotFoundException {
        if (id == null || !id.startsWith(ROOT_DOC)) {
            throw new FileNotFoundException(String.valueOf(id));
        }
        File root = base();
        String rel = id.substring(ROOT_DOC.length());
        File f = rel.isEmpty() ? root : new File(root, rel);
        try {
            String c = f.getCanonicalPath();
            String r = root.getCanonicalPath();
            if (!c.equals(r) && !c.startsWith(r + File.separator)) {
                throw new FileNotFoundException(id);
            }
        } catch (IOException e) {
            throw new FileNotFoundException(id);
        }
        return f;
    }

    private String id(File f) throws FileNotFoundException {
        try {
            String r = base().getCanonicalPath();
            String c = f.getCanonicalPath();
            return c.equals(r) ? ROOT_DOC : ROOT_DOC + c.substring(r.length() + 1);
        } catch (IOException e) {
            throw new FileNotFoundException(f.getName());
        }
    }

    @Override
    public Cursor queryRoots(String[] projection) throws FileNotFoundException {
        MatrixCursor c = new MatrixCursor(projection != null ? projection : ROOT_COLUMNS);
        c.newRow()
            .add(Root.COLUMN_ROOT_ID, "projects")
            .add(Root.COLUMN_FLAGS, Root.FLAG_SUPPORTS_CREATE | Root.FLAG_SUPPORTS_IS_CHILD | Root.FLAG_LOCAL_ONLY)
            .add(Root.COLUMN_TITLE, "ShapeCraft")
            .add(Root.COLUMN_SUMMARY, "Projects and exports")
            .add(Root.COLUMN_DOCUMENT_ID, ROOT_DOC)
            .add(Root.COLUMN_ICON, getContext() != null ? getContext().getApplicationInfo().icon : 0);
        return c;
    }

    @Override
    public Cursor queryDocument(String documentId, String[] projection) throws FileNotFoundException {
        MatrixCursor c = new MatrixCursor(projection != null ? projection : DOC_COLUMNS);
        row(c, file(documentId));
        return c;
    }

    @Override
    public Cursor queryChildDocuments(String parentId, String[] projection, String sortOrder) throws FileNotFoundException {
        MatrixCursor c = new MatrixCursor(projection != null ? projection : DOC_COLUMNS);
        File[] kids = file(parentId).listFiles();
        if (kids != null) {
            for (File k : kids) {
                if (!k.getName().startsWith(".")) {
                    row(c, k);
                }
            }
        }
        return c;
    }

    private void row(MatrixCursor c, File f) throws FileNotFoundException {
        boolean dir = f.isDirectory();
        String id = id(f);
        int flags = dir ? Document.FLAG_DIR_SUPPORTS_CREATE : Document.FLAG_SUPPORTS_WRITE;
        if (!id.equals(ROOT_DOC)) {
            flags |= Document.FLAG_SUPPORTS_DELETE | Document.FLAG_SUPPORTS_RENAME;
        }
        c.newRow()
            .add(Document.COLUMN_DOCUMENT_ID, id)
            .add(Document.COLUMN_DISPLAY_NAME, id.equals(ROOT_DOC) ? "ShapeCraft" : f.getName())
            .add(Document.COLUMN_MIME_TYPE, dir ? Document.MIME_TYPE_DIR : mimeOf(f.getName()))
            .add(Document.COLUMN_FLAGS, flags)
            .add(Document.COLUMN_SIZE, dir ? null : f.length())
            .add(Document.COLUMN_LAST_MODIFIED, f.lastModified());
    }

    @Override
    public ParcelFileDescriptor openDocument(String documentId, String mode, CancellationSignal signal) throws FileNotFoundException {
        File f = file(documentId);
        if (f.isDirectory()) {
            throw new FileNotFoundException("a folder");
        }
        return ParcelFileDescriptor.open(f, ParcelFileDescriptor.parseMode(mode));
    }

    @Override
    public String createDocument(String parentId, String mimeType, String displayName) throws FileNotFoundException {
        File parent = file(parentId);
        String name = safeName(displayName);
        File f = new File(parent, name);
        // Never over an existing file: "name (2).ext", as Files does.
        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String ext = dot > 0 ? name.substring(dot) : "";
        for (int n = 2; f.exists(); n++) {
            f = new File(parent, stem + " (" + n + ")" + ext);
        }
        try {
            boolean made = Document.MIME_TYPE_DIR.equals(mimeType) ? f.mkdir() : f.createNewFile();
            if (!made) {
                throw new FileNotFoundException("couldn't make " + name);
            }
        } catch (IOException e) {
            throw new FileNotFoundException("couldn't make " + name);
        }
        return id(f);
    }

    @Override
    public void deleteDocument(String documentId) throws FileNotFoundException {
        File f = file(documentId);
        if (id(f).equals(ROOT_DOC) || !deleteTree(f)) {
            throw new FileNotFoundException("couldn't delete " + f.getName());
        }
    }

    @Override
    public String renameDocument(String documentId, String displayName) throws FileNotFoundException {
        File f = file(documentId);
        File to = new File(f.getParentFile(), safeName(displayName));
        if (id(f).equals(ROOT_DOC) || to.exists() || !f.renameTo(to)) {
            throw new FileNotFoundException("couldn't rename " + f.getName());
        }
        return id(to);
    }

    @Override
    public boolean isChildDocument(String parentId, String documentId) {
        return parentId != null && documentId != null && parentId.startsWith(ROOT_DOC)
            && (parentId.equals(ROOT_DOC) ? documentId.startsWith(ROOT_DOC) : documentId.startsWith(parentId + "/"));
    }

    private static String safeName(String name) {
        String n = name == null ? "" : name.replace('/', '_').replace('\\', '_').replace('\0', '_').trim();
        while (n.startsWith(".")) {
            n = n.substring(1);
        }
        return n.isEmpty() ? "Untitled" : n;
    }

    private static String mimeOf(String name) {
        int dot = name.lastIndexOf('.');
        String mime = dot >= 0 ? MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substring(dot + 1).toLowerCase()) : null;
        return mime != null ? mime : "application/octet-stream";
    }

    private static boolean deleteTree(File f) {
        File[] kids = f.listFiles();
        if (kids != null) {
            for (File k : kids) {
                deleteTree(k);
            }
        }
        return f.delete();
    }
}
