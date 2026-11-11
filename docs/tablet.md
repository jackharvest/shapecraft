# Tablet access (iPad, iPhone, any browser)

> **In testing.** Tablet access is an early look, not a finished feature. It has been tried on a real iPad only
> briefly, so expect rough edges in how the Pencil, touch and the on-screen keyboard behave. Please
> [open an issue](https://github.com/jackharvest/shapecraft/issues/new/choose) for anything that feels wrong.

ShapeCraft can serve itself to a browser on your network, so you can model on an iPad with an Apple Pencil while
your projects stay on your computer. Safari on iPad comes first; Chrome and other Chromium browsers work too. Nothing
is installed on the tablet, and nothing leaves your network.

## Turning it on

1. On the computer: **Preferences ▸ Tablet ▸ Serve ShapeCraft to browsers**.
2. On the iPad: scan the QR code with the Camera, or type the address shown under it into Safari.
3. For full screen, like an app: **Share ▸ Add to Home Screen**, then open ShapeCraft from the Home Screen.
   In Safari itself, the **Full Screen** button at the bottom does the same on iPad.

**Reachable From** chooses between the whole network and this computer only. The address carries a pairing key
(`?k=…`). A browser that has opened it once remembers the key. **New Pairing Key** turns away every device paired
before.

The tablet gets its own copy of ShapeCraft, running on the computer, with the same projects folder and preferences.
Close the tab or let the iPad sleep and the copy keeps going: open it again and you're where you left off.

## Pencil, fingers, keyboard

| Input | What it does |
| --- | --- |
| Apple Pencil | The pointer: tap, drag, draw. Edges, sketch curves and snap points reach about twice as far as for a mouse, and a small wobble during a tap still counts as a tap. On iPads with Pencil hover, holding the tip above the glass lights up what a tap would pick, and what's lit holds on until the tip moves well away, so the tap picks it. |
| Pencil, drag on nothing | Turns the view. |
| Pencil, hold, then drag | Area Selection: drag right for what's fully inside the box, left for what it crosses. |
| Pencil in a sketch | **Sketch** arms Line at once: draw straight onto the floor grid or a face. A stroke that bends round becomes a circle around where it started. |
| Pencil on a value pill | Opens a number pad beside it (digits, minus, backspace, ✓). |
| Fingers in the 3D view | Move the view only: fingers never pick, draw or drag handles (the Pencil does). |
| One finger, tap | Press a button. In the 3D view: nothing. |
| One finger, double tap | In the 3D view: zoom to the face under it. |
| One finger, drag | In the 3D view: turn the view, even with a sketch tool on. Over lists: scroll. |
| One finger, hold | Over a panel: right-click (context menu). In the 3D view: nothing. |
| Two fingers | In the 3D view: pan and pinch to zoom. Over lists: scroll. |
| Three-finger swipe | Left: undo. Right: redo. The on-screen Undo and Redo buttons sit at the bottom of the tool column. |
| Palm on the glass while drawing | Ignored. |
| Magic Keyboard / any keyboard | Every shortcut, ⌘ included. Typing a number while a tool runs works as on the computer. |
| On-screen keyboard | When ShapeCraft wants text, a keyboard button appears at the bottom right. iOS opens its keyboard only from a tap, so the button is there for that tap. |
| Trackpad | Two-finger scroll turns the view, pinch zooms, as on a Mac trackpad. |

## Files

- **Import**, **Insert ▸ Image**, decals, materials and environments open a sheet: **Choose…** picks from Files,
  Photos or iCloud Drive, and the file is sent to the computer.
- **Export**, **Screenshot** and drawing exports come back as a **Download** button, to save in Files.
- **Export ▸ Slicer** gives you the 3MF to download and open in your slicer's app.

## Without a window (server, VM, container)

```sh
shapecraft --serve 8780                 # every network interface, port 8780
shapecraft --serve 127.0.0.1:8780       # this machine only (behind your own reverse proxy)
SHAPECRAFT_KEY=yourkey shapecraft --serve --projects /srv/shapecraft
```

It prints the address with the key. No display is needed. Without a GPU, ShapeCraft draws on the processor (Mesa's
llvmpipe or lavapipe), which works but is slower.

## How it works

The tablet's copy runs offscreen on the computer with its own GPU device, drawn at the tablet's own pixel density.
Only the parts of the screen that change are sent, as JPEG while things move and then again as lossless PNG once the
picture is still, so text and edges end up exact. Input goes back over a WebSocket. The page is one small HTML file
built into the app, with no web framework and no app download.

## Good to know

- The connection is plain HTTP on your network, protected by the pairing key. Don't open the port to the internet.
  If you need to reach it from outside, use a VPN, or a reverse proxy with HTTPS.
- Each device gets its own copy of ShapeCraft (up to four at once), so an iPad and a phone don't get in each
  other's way. Opening it in a second tab on the same device moves it there; the first tab says so and waits with a
  **Use Here** button instead of taking it back.
- The computer and the tablet can both open projects. Don't edit the *same* project on both at once: each saves its
  own copy over the other.
- The first time, the tablet asks how you'll work (Apple Pencil, fingers, or keyboard and trackpad) and shows the
  controls for it. Projects open with History hidden and the Items column at its narrowest, for room to model.
- The project title is a pill at the top with a ⋯ menu (Import, Export to, Help, Settings). The Items list has
  finger-sized rows, and Rename, Delete and More… along its bottom act on what's selected. One tap opens a project.
- The layout is the desktop's. An iPad shows it at full size. Smaller screens (an iPad in portrait, a phone) see it
  scaled down to fit.
