# Changelog

How versions are numbered: [`docs/versioning.md`](docs/versioning.md). Newest first.

## 0.6.8-alpha
**Starting a sketch no longer starts a line by itself.**
### Fixed
- **Sketch, then a click on a plane, a face or the grid** turns the view to face it and has Line ready, and that click
  only picks where to sketch: it no longer becomes the first end of a line you then had to cancel. Picking a tool
  first (L, R, C…) and then clicking still places that tool's first point.

## 0.6.7-alpha
**Coils, springs and threads from Revolve, a Screenshot mode with sizes up to 8K, more ways to measure, DWG from a
sketch, and more picture formats.**
### New
- **Revolve with a height makes a helix.** An arrow on the axis raises the profile as it turns; type an angle, press
  Tab and type the height (1080, Tab, 12 is three turns rising 12 mm). Turns that would run into each other are
  refused. Height is on the History card too, so a spring's pitch can change later.
- **Screenshot mode.** Screenshot takes the left column: Grid, Transparency and Body Edges switches, Resolution
  (Actual, Double, Full HD, 4K or 8K) and Capture. Your choices are remembered.
- **Measure: Central Distance and Parallel Distance.** From a hole's axis, a circle's or arc's center, or a body's
  center to the other pick, and between parallel faces, edges and axes. A picked arc shows its Central Angle.
- **DWG from Sketch** (File ▸ Export to): a sketch's curves as an AutoCAD drawing, like DXF from Sketch.
- **Show Versions** on a project's right-click menu opens its older versions in your backups, ready to restore.
### Changed
- **Pictures:** Insert Image, decals, custom materials and drawing-sheet images also take TIFF, BMP, GIF and ICO.
- **More of ShapeCraft is Rust.** The helix behind Revolve's height and Offset Face's choice between ways of moving
  faces are worked out in Rust, with only single geometry-kernel calls left in C++. Packaging for macOS, Windows and
  Linux, the benchmark, the icons and the website's recordings are built by `cargo xtask` instead of Python, shell
  and PowerShell scripts.

## 0.6.5-alpha

**Smoother first steps: the setup's Light and Dark pictures are the right way round, a typed extrude shows the part
it will make, and sketches show their closed shapes while you draw.**

### Changed
- **Closed shapes are tinted while you sketch.** A rectangle or circle you just closed shows its faint blue fill in
  the open sketch too, so you can see it's a profile ready to pull.

### Fixed
- **First-run setup:** the Light and Dark cards showed each other's picture when the app was in the light theme.
- **Extrude with a typed depth** drew a blue stand-in box over the new part until Done, and showed a Result badge
  for the body it was making; it now shows the new part in plain grey, the same as when you drag.
- **Command search** no longer lists History's Edit Sketch (or other steps that need a place in History) among the
  recent commands, where picking it only showed an error.
- **Tablet connections no longer drop at random** when the Mac is the host.
- **A run given its own projects folder** (`--projects`, as a tablet server uses) no longer backs up into the
  normal Work Folder's backups, where a project with the same name would have replaced the other's latest copy.

## 0.6.3-alpha

**No more crash when stretching a part a second time, value pills you can click, and edges, sketch points and sketch
profiles you can scale, move and turn right where they are.**

### New
- **Click a value pill to type into it.** The number on a handle ("19 mm", "R 2mm", "1.50×") opens the value
  field with that value selected when you click it, the same as typing would; the first key replaces it.
- **Scale on a rim.** With one round edge picked (the top of a cylinder, say), Scale works on that edge alone: the
  flat face inside it scales with it and the side follows, so pulling it out makes a taper and a one-axis pull makes
  an oval top.
- **Move/Rotate on a rim.** The same edge can be moved: the gizmo comes up at its middle, and moving it up makes the
  part taller, sideways slants it.
- **Scale and turn a straight edge.** With one straight edge picked, Scale stretches that edge about its middle and
  Move/Rotate's turning handles turn it; the faces around it follow (a face that can't stay flat bends smoothly).
- **Move a sketch point.** Pick a point (a corner, an end, a center) and press M: the gizmo sits on it, the point
  moves in its sketch, and whatever meets there comes along, keeping the sketch's constraints.
- **Two flat faces.** The bar at the bottom shows the angle between them, and the left column offers **Axis - 2
  Planes** and **Plane - Midplane**; between two faces that meet, the midplane halves the angle inside the part.
- **Copy for sketch curves.** Move/Rotate's Copy works on sketch curves: copies made in the plane join the sketch,
  copies lifted off it go to a new sketch and the originals stay.

### Changed
- **Split Body** isn't offered for a single straight edge any more (a round edge still offers it).
- **Offset Face on a whole body** picks all of its faces and starts at once, with Done showing.
- **Scale on a sketch profile** picked outside its sketch scales it right there, instead of opening the sketch.
- **Pulling a profile into a new body** shows the body a little see-through, with its sketch inside, until Done.
- **The value field** you type into is see-through over the view, and Scale shows its factor ("1.00×", up to four
  decimals) along the active handle from the start.
- **Several faces with a curved one** no longer offer Shell.

### Fixed
- **Crash when stretching a part again.** A one-axis (non-uniform) scale of a part that had been stretched before,
  or of a cylinder with a filleted or chamfered rim, could close the app. Those stretches now work.
- **Scale with one straight edge picked** scaled the whole part.
- **A handle left on screen** after running a command from the left column (Offset Face's arrow stayed up after
  Plane - Midplane, for one).

## 0.6.0-alpha

**Your projects, kept safe and on every computer: a Work Folder that autosave writes to, backups to Google Drive,
OneDrive, Dropbox, iCloud Drive, a NAS or any folder (with older versions to go back to), and a first-run setup that
asks how ShapeCraft should look and where your projects live.**

### New
- **Preferences ▸ Sync.** Three parts: your Work Folder, your backups, and the cloud storage apps found on this
  computer (Google Drive, OneDrive, Dropbox and iCloud Drive), each with its state: ready with its folder,
  installed but not signed in, a **Get** link when it isn't installed, or **Not Supported** where the app has no
  version for your system (Google Drive and iCloud Drive on Linux; add an rclone or Insync folder there instead).
  ShapeCraft finds each app's folder from the app's own settings, so a folder you moved somewhere else, or put on
  another drive, is found too.
- **Work Folder.** Where projects save as you work: Documents/ShapeCraft on this computer by default, or any folder
  you choose (a network folder works too, more slowly). **Change…** asks whether to bring your projects along.
- **The same projects on every computer.** **Work Here** puts your Work Folder in a cloud app's folder, so every
  computer signed in to it sees the same projects and Recents. Made safe for that:
  - Opening a project another computer has open asks first (**Open Anyway** or **Cancel**).
  - A project whose new shapes haven't finished syncing waits ("still syncing") instead of opening with old ones.
  - When two computers saved a project at once, its card says **2 Versions**, and **Split Off Other Version**
    (right-click) makes the other computer's version a project of its own.
- **Backups.** An ordered list you control (**Up** and **Down**): any cloud app found, or any folder or NAS share.
  The first is brought up to date first. Each keeps `ShapeCraft Backups/<computer name>/` with the same folders and
  project names as your Work Folder, so it reads well in Finder or Explorer without ShapeCraft. Turn off **A Folder
  for Each Computer** when computers share a Work Folder. A cloud backup follows its app's folder if you move it.
  - **When:** on **⌘S / Ctrl+S**, closing a project and quitting (the default, which leaves a gap to go back to), or
    a moment after every change. **Back Up Now** any time.
  - **Older versions:** each backup keeps the last 10 copies of every project it replaced (5 to 50, or off), dated,
    under **Older Versions**.
  - **Never deletes:** deleting or renaming a project on this computer leaves its backup copy alone.
  - Each backup shows how it's doing: up to date and when, backing up (3 of 12), or why it can't (a drive that
    isn't connected is tried again every 10 minutes).
- **Restore from a backup.** Backups are listed in the projects sidebar. Browse any computer's copies and their older
  versions, then **Restore** or **Restore and Open**. A restored project comes back beside your projects, never over
  one.
- **First-run setup.** After the device and navigation questions: Light, Dark or Match System; where your projects
  live (this computer or a cloud app found); and which backups to keep. New installs see it once, and so does
  everyone updating to this version.

### Changed
- **⌘S / Ctrl+S** saves as before (projects save themselves) and now also backs up.
- **Preferences** has no Account tab: there are no accounts.
- **Saves** write their temporary files under hidden names that sync apps leave alone, so a half-written file is
  never uploaded.

## 0.5.19-alpha

**Sketch on curved faces, Move/Rotate on sketches, and what you push, pull or move stays highlighted where it went.
Also an early look at ShapeCraft on your iPad (in testing): tablet access serves the app to Safari with Apple Pencil
and touch, and your projects stay on your computer. Plus sketch text on Linux systems with fonts outside the usual
folders, and a Nix development shell, both thanks to [@eamondo2](https://github.com/eamondo2).**

### New
- **Tablet access (iPad first, any browser), in testing.** An early look, not finished: expect rough edges.
  Preferences ▸ Tablet ▸ Serve ShapeCraft to browsers, then scan the QR code with the iPad's camera. The tablet gets
  the full app at its own Retina sharpness. Add it to the Home Screen and it opens full screen, like an app.
  See [docs/tablet.md](docs/tablet.md).
  - **Apple Pencil** is the pointer: tap, drag and draw, with hover highlighting on iPads that support it, and the
    palm ignored while you draw.
  - **Fingers** turn the view (one finger, even mid-sketch), pan and pinch-zoom (two). A two-finger tap undoes, a
    three-finger tap redoes, and a long press opens the context menu.
  - **Keyboards**: every shortcut on a Magic Keyboard, and a keyboard button for the on-screen one when ShapeCraft
    wants text or a number.
  - **Files**: Import and Insert pick from Files or Photos, and exports, screenshots and slicer 3MFs come back as
    downloads.
  - **Easy to hit with the Pencil**: edges and curves reach farther than with a mouse, what the hovering Pencil
    lights up stays lit until you move well away (so the tap picks it), and a tap that wobbles stays a tap.
  - **Several devices at once**, each with its own copy of the app. A second tab on the same device takes over
    and the first one waits with a **Use Here** button, instead of the two reconnecting over each other.
  - **Made for the tablet**: a first-run question (Apple Pencil, fingers, or keyboard and trackpad) with the
    controls for each, bigger project cards, and projects that open with History hidden and the Items column
    narrow.
- **`shapecraft --serve`** runs only the tablet host, with no window, for a server, VM or container.
- **Nix development shell.** `nix develop` (or `direnv allow` with nix-direnv) sets up the Rust toolchain,
  OpenCASCADE and the window libraries on Linux. Thanks [@eamondo2](https://github.com/eamondo2) (#1).
- **Sketch on a curved face.** Space or Sketch on Face over a cylinder's wall (or any curved face) starts a sketch on
  the origin plane facing you and turns to it, instead of saying it can't. Clicking a curved face while a sketch tool
  waits for a plane does nothing, and the tool keeps waiting.
- **Move/Rotate on sketches.** Pick a sketch profile and press M: the gizmo comes up on it. Clicking a sketch's curve
  opens that sketch with the curve picked, and a sketch's row in Items opens it with all its curves picked, so
  Move/Rotate and the other sketch tools act on them straight away.
- **Lift sketch curves off their plane.** The gizmo on sketch curves now has all three arrows and turns. Moving or
  turning every curve of a sketch carries its plane along; moving only some puts them in a new sketch on the moved
  plane.

### Changed
- **Moving, turning, scaling, mirroring or patterning curves inside a sketch** is part of that sketch in History,
  not a step of its own, and Done or Return after a move leaves nothing picked.
- **The History sidebar stays open** when a sketch starts with Space or Sketch on Face.

### Fixed
- **Sketch text on NixOS, Guix and similar systems.** When none of the usual font files exist, the Text tool uses
  the system's sans-serif font instead of drawing nothing, and shows that font's real name. Thanks
  [@eamondo2](https://github.com/eamondo2) (#2).
- **A face pushed or pulled with Offset Face, or moved with Move/Rotate, stays picked at its new place.** Before, the
  whole body lit up after you let go (a pocket floor pulled up, for one). The arrow or gizmo and the highlight now
  sit on the face together, drag after drag; Replace Face does the same.
- **Fillets, chamfers, Shell, Fill, Project as Edge and Wrap no longer pick the whole body afterwards.** A live fillet
  or chamfer keeps the edges it rounds drawn as picked until Done.
- **Done and Cancel leave nothing picked**, and Cancel takes a pulled result back without leaving picks that no
  longer fit the part.
- **A live Offset Face or Move** shows the short left column with the running tool lit, as Extrude and Fillet do.

## 0.5.13-alpha

**The arrows come up for whatever you pick, however you pick it: edges and faces move and turn on their own, hidden
picks show through the part, and sketch curves scale and turn.**

### New
- **Move/Rotate on faces.** Pick faces of a body and press M: the gizmo comes up at them and moves or turns just
  those faces, the rest of the body following. A flat face slides along itself, and a whole pocket or hole moves
  to a new spot.
- **Move an edge.** Pick a straight edge and press M: the gizmo comes up at its middle, and dragging it moves the edge
  while the faces beside it tilt to follow.
- **Turn faces around an edge.** Pick faces (or bodies), then Shift-click an edge: Rotate Around Axis comes up with
  that edge as the axis. A face turned about one of its own edges tilts, the rest of the body following.
- **Scale and Rotate Around Axis on sketch curves.** Pick curves in a sketch and choose Scale: the handle comes up in
  their middle. Rotate Around Axis takes more curves as you click them, then turns them about a sketch point.
- **View ▸ Visualized (⌥V) is a display mode.** The part gets the Visualize look (no edges or grid, the backdrop and
  a floor shadow) while the modeling tools, the left column and History stay where they are; what you pick is
  outlined. Pick another display mode, or press ⌥V again, to go back.

### Changed
- **Hidden picks show through.** A picked face behind or inside the part (a pocket's floor, a hole's wall), a
  picked edge, a picked body inside another and a body you point at in the Items list are drawn faintly in blue
  over whatever hides them, with their outlines. Tools started on them (Extrude, Offset Face, Fillet) show where
  they are.
- **Revolve waits for an axis.** With a sketch region picked, nothing spins and nothing previews until you click a
  line, an edge or a round face to turn it around, as it already did for a face.
- **Shift-click on something already picked takes it back out of the selection.**
- **Several faces picked:** the arrow sits on the face picked last, with a plain distance (no Total choice).
- **Fillet chosen with F or the column** shows its G1 badge from the start, and its handle sits on the newest edge (a
  face's inner loop, such as a hole's rim).
- **A picked body** draws as one solid tinted body, without its own hidden edges showing through.

### Fixed
- **Picking a body in the Items list** (or with Select All) after a face or an edge left the old Offset Face or
  Fillet arrow on screen instead of bringing up the Move gizmo. Several bodies now share one gizmo in their middle.
- **A face and an edge picked together** kept showing the face's Offset Face arrow and hint.
- **Scale in a sketch** let go of the picked curves and asked for bodies.
- **A failed step** no longer says "failed" twice ("offset failed: offset failed for this distance").

## 0.5.10-alpha

**STEP, IGES and Parasolid imports ask how to clean the geometry up, pushing a face out looks the way it will turn
out, the menus show each tool's single key, and History dims as soon as a tool starts.**

### New
- **Import Preferences.** Before a STEP, IGES or Parasolid file comes in, choose Quality (repair, rebuild edges, sew
  and simplify), Speed (take the file as it is) or Custom (pick the clean-ups one by one). "Don't show this again"
  keeps your choice.
- **Single keys in the menus.** Sketch, Transform and Tools show each tool's key (L, A, C, P, T, M, S, N, F, E, H, V,
  W; I, R and G for the Recent Spline, Rectangle and Polygon). Typing in a field and a tool's own keys, such as E
  flipping Sides while you extrude, work as before.

### Changed
- **Pushing a body's face out** shows the part as it was with the face still picked and the added volume
  see-through over it. The arrow is blue from the start, and there's no draft handle on a body face.
- **The face's tools stay in the left column** with Extrude lit after you press E, until you pull or type a depth.
- **History dims** the moment a modeling tool starts, not only once a result waits for Done.
- **Imported parts** are called "Imported 01", "Imported 02"… in each file, and nothing is selected after an import.
  Importing into a project that has work in it keeps the project's name.
- **View ▸ Show Sketches** is off by default and applies to the Visualized look. While modeling, a sketch shows or
  hides by its own eye in Items.

## 0.5.9-alpha

**Fillet, Offset Face and Extrude now wait for Return before a typed value takes effect, Return finishes a tool,
and a result waiting for Done looks the same in every tool. The home screen gets its own menus and folder commands,
the View menu shows check marks, and multi-colour 3MF files land on separate filaments in Bambu Studio and
OrcaSlicer.** Everything since 0.5.3 is below.

### New
- **Open in your slicer with one click.** Export ▸ Slicer lists an "Open in …" row for each slicer found on your
  computer. One click exports the model and opens it there.
- **Multi-colour 3MF.** Each body colour arrives in Bambu Studio and OrcaSlicer (Snapmaker Orca tested) on its own
  filament. Before, every body landed on filament 1. The slicer still picks the filament colours for those slots.
- **Home screen menus.** The home screen has its own menu bar: File (New Project, New Folder ⇧⌘N, New Folder with
  Selection ⌥⌘N, Import as new project ⇧⌘I, Duplicate ⌘D), Edit (Rename ⌘R), Go (Back, Forward, Enclosing Folder,
  Recents ⇧⌘1, Projects ⇧⌘2), View (Sort By, Search ⌘F), Window and Help.
- **New Folder with Selection** (⌥⌘N) puts the selected project or folder into a new folder in its place, with the
  name ready to type.
- **Sketch menu submenus.** Spline, Rectangle and Polygon start as the type you pick there (Rectangle ▸ 3 Points,
  Polygon ▸ Hexagon …) or as last used.
- **View menu.** Show Sketches; Lock Grid Size shows the size it would lock, with a tip that zooming changes it;
  Material, Environment and Camera Properties (in Visualization) and Drawing Properties (in Drawings). The sidebars,
  view options, display mode, units and formats show a check mark when they're on.
- **Return with nothing to finish** opens Search Commands where the pointer is.

### Changed
- **Typed values take effect on Return**, in every tool. The part, the arrow and the cut stay where they are while
  you type, and the typing box opens where the value was. Escape closes just the box; the tool stays open. Fillet
  uses the same full-size box as the other tools.
- **Offset Face** shows its value on the dashed line after the distance type. A flat face offers Offset and Total
  (its thickness). A round face, such as a shaft or a hole, offers Offset, Radius and Diameter, starts on Diameter and
  reads exactly ("⌀10 mm", not 9.98). Cones, spheres and free-form faces just offset, with the value always shown.
  The arrow is a shorter, lighter double arrow.
- **Fillet on the canvas** reads "0 mm" before you pull, then "R2 mm" with one badge, Continuity (G1 or G2). Type and
  Corner are in the Fillet card.
- **Pushing a sketch into a part** shows the picked profile in orange and the volume to be cut in faint red, with a
  thin dark rim where it ends. The part stays whole until Done, and the Result badge says Subtract the whole time.
- **While a result waits for Done**, the left column shows the usual tools with the running one lit below them
  (Fillet now too) and Isolate at the bottom. History dims its filter and steps and lists the new step only once you
  press Done. While you sketch, History dims every step but the sketch you're in, and an empty new sketch isn't listed
  yet.
- **Selection.** A round face shows its diameter at the bottom ("⌀ 10 mm"). A sphere is picked whole with one click
  and shows its diameter too.
- **Tool hints** say what to pick (faces, edges, a profile) instead of how to drag.
- **View cube.** Its edges look cut rather than rounded, its axes are shorter, and clicking near a face's edge or
  corner picks that view without having to hit the thin bevel.
- **New tools are marked.** Fill (Beta) has its own icon. It and the sketch Fillet carry a small blue dot, as do the
  Tools and More buttons that hold them. The Tools list shows Fillet and Chamfer separately and includes Fill. Scale
  and Project show their S and P keys.
- **Home screen.** New folders are called "Untitled Folder", and a folder says how many items it holds. A search that
  finds nothing says "No results for …". Project names stop at two lines.
- **Menus.** "Wrap & Emboss" and "Foot & Inch Format" show their "&". The Window menu has the system's items (Zoom,
  Fill, Center, Move & Resize, tiling) and lists the window by its project's name. A project's menus no longer carry
  the home screen's items. The .shapr format is named after its file type everywhere in the app.

### Fixed
- **Return finishes a tool.** With a result already applied and nothing typed, Return now acts as Done, as the
  button's ↵ says. Before, it applied the same result again and the tool stayed open.
- **Cancel and Escape throw a pulled result away** (Extrude, Offset Face and the rest). They used to keep it as if
  you had pressed Done.
- **Offset Face on a cone** showed a wrong diameter ("⌀15.6576 mm"), and on a sphere it offered Total. Both now
  just offset.
- **The Extrude card** said New Body for a cut waiting for Done. It now says "Subtract - Auto" and lists what it
  cuts, and agrees with the Result badge while you type a depth.
- **Projects keep each body exactly as saved.** A surface body, or one body made of separate pieces, could come back
  wrong or missing. A project whose bodies file is missing or damaged now opens, rebuilds its bodies from the History
  and tells you, instead of failing with "STEP import failed". A damaged file is kept beside it.
- **Long names** in the Items and History lists ran out of the panel. They now end in "…".
- **Slow to open after an update (macOS).** ShapeCraft could sit with no window while macOS waited for you to allow
  access to the Documents folder. The window now opens right away, says it's opening your projects folder, and tells
  you to choose Allow. If access was turned down, it says where to turn it back on.
- **Scripts and agents.** A command sent over the control channel or MCP while a modeling tool was open left that
  tool's handles and hint on screen. The tool now closes first, keeping what it already applied.

## 0.5.3-alpha

Fixed:
- A sketch drawn on the ground over a part's top face, then pulled into the part, made a separate body. A shape
  that lies wholly on a face now cuts when pushed in and joins when pulled out, just like one sketched on that
  face. Only partly on the face still makes a new body. The Result badge and the step's card show the same choice.
- Escape inside a sketch now steps back one layer at a time: it puts down the tool, then clears the selection,
  then leaves the sketch. With nothing drawn yet, Escape leaves right away.
- New projects opened a bit too close and slightly turned, so the same clicks made smaller parts than people
  expect. The starting view now looks straight down the diagonal from a little farther back.
- Clicking the middle of a cylinder's wall could select the far side of its bottom edge, hidden behind the wall.
- Selecting a round face (a cylinder's wall, a hole) shows its radius at the bottom, next to "1 face".

## 0.5.2-alpha

Fixed:
- While the first-run questions (device, navigation style) were open, keyboard shortcuts still reached the model
  behind them, so ⌘V or a tool key changed a project you couldn't see. They now wait until you answer or skip.
- The Help menu follows the familiar order: Help, then shortcuts and search, then Get Started and Tutorials (which
  now open those pages of the home screen), then reporting a problem, release notes and updates.

## 0.5.1-alpha

New:
- Edit ▸ Copy and Paste (⌘C / ⌘V): copy bodies or construction planes and paste them in place as separate,
  selected copies, ready to move. Text fields still copy and paste text as before.
- Export ▸ Slicer lists the slicers you actually have installed (Bambu Studio, OrcaSlicer, PrusaSlicer, Snapmaker
  Orca, UltiMaker Cura, Creality Print, ElegooSlicer), plus your default app for 3MF files, and remembers your pick.
- The Help menu has the three tutorials, the website, release notes and a link for reporting a problem or idea.
- Tool types you pick (Rectangle ▸ 3 Points, Fillet ▸ G2 and the rest) stay picked in your next project and after
  a restart.

Fixed:
- Pulling a round profile showed a fence of thin lines up its side while the preview caught up; it now shows a
  smooth tinted wall, with lines only up real corners.

For scripts and agents:
- `ui.edit {action: copy|paste}` runs Edit ▸ Copy / Paste.

## 0.5.0-alpha

New:
- The first launch asks a second question after the device: which app's navigation feels like home. Your existing
  install asks it once too. All twelve navigation presets now fit in the Navigation Presets window (the last three
  were cut off).

Fixed:
- A cut made from a plane that isn't on a body goes into the bodies it really runs into. It used to cut the oldest
  body whose bounding box it touched, so a logo pushed into an inset panel cut the shell around it instead.

For scripts and agents:
- `sketch` returns its plane (origin, u and v axes, normal) and takes `x_dir` to aim the u axis.
- `inspect` gives each region its hole count and the region it sits inside (letters' counters, nested rings), and
  narrows to one `body` or `sketch`, with `faces: false` to leave out face lists.
- `"look": false` keeps the open app's camera still when a script starts a sketch. `visual_settings` says that
  environments show in Visualize.
- docs/mcp.md: a one-line setup for agent tools, the sketch axis table, and conventions that used to need testing.

## 0.4.3-alpha

Fixed (found by updating real installs on all three systems):
- Two copies of ShapeCraft on one computer (an installed one and a portable one) no longer share their update folder,
  so one copy's update can't clear away the other's download.
- When an update can't be installed, ShapeCraft says why the next time it starts and tries again; with Update and
  Reopen it still reopens, on the version you had.

## 0.4.2-alpha

Fixed:
- Help ▸ What's New lists the newest features, as the home screen's cards do (it showed made-up entries).
- Windows: updates find the folder ShapeCraft runs from in its plain form, so the portable zip's files are replaced
  cleanly.
- Release builds: macOS binaries leave room for the bundle's library paths (the Intel build couldn't be packaged), and
  the Windows build uses Microsoft's linker.

## 0.4.1-alpha

The first public release.

New (from 0.4.0, which was tagged but never published):
- ShapeCraft keeps itself up to date: it checks for a new version when it starts and once a day, downloads it in the
  background, and installs it when you close ShapeCraft (it asks first, or always, your choice).
- Help ▸ Check for Updates…, and an Updates section in Preferences ▸ ShapeCraft.
- Downloads for macOS (Apple silicon and Intel), Windows (installer and portable zip) and Linux (AppImage and
  tar.gz), each with OpenCASCADE inside, and `SHA256SUMS.txt` to check them.

Fixed:
- The release builds on Linux (a C++ conversion GCC refused) and Windows (the linker).

## 0.4.0-alpha

New:
- ShapeCraft keeps itself up to date. It checks for a new version when it starts and once a day, downloads it in the
  background, and installs it when you close ShapeCraft (it asks first, or always, your choice). A quiet note in the
  bottom-right corner shows the download and offers Restart; its card has the release notes and Skip This Version.
- Help ▸ Check for Updates…, and an Updates section in Preferences ▸ ShapeCraft: check now, automatic checks and
  downloads on or off, and what closing does with a downloaded update.

Changed:
- Download names follow one pattern on every system (`shapecraft-0.4.0-windows-x64-setup.exe`, …), with
  `SHA256SUMS.txt` beside them. The Linux download is a `.tar.gz`.
- Linux: `install.sh` leaves a copy of itself with the app, so `~/.local/lib/shapecraft/install.sh --uninstall`
  works after the download folder is gone.

## 0.3.1-alpha

Fixed (found by building the same parts by hand in both apps, side by side):
- Offset Face shows its distance type again: "Total ⌄" with the part's thickness through the point you picked.
- Offset Face on a round face reads "Radius ⌄" and "R…", with its dashed line running in to the axis; typing a
  radius sets it, for shafts and holes alike.
- The value pill keeps clear of the type pill however long the number is.
- Extrude picked inside a sketch leaves the sketch, as the reference does: the idle column with Extrude lit below
  it, and Cancel at the bottom of the view.
- An edge or corner of the view cube turns to an orthographic view, as its faces do; orbiting goes back to
  perspective.
- "Normal to Sketch" and "Zoom to Selection" are readable in the light theme (they were white on white).

## 0.3.0-alpha

New:
- In-app tutorials: Model a Phone Stand, Model a Pencil Cup and Model a Key Tag, about 7 minutes each. Each step
  starts from its own scene, shows only the tools it needs, moves its instruction on as you work, and lights Continue
  once its goal is met; a picture of where the step is headed waits in the corner.
- Get Started and Tutorials pages on the home screen.
- What's new on the home screen: a card for each new feature, naming the version that added it.
- Drag the view cube to orbit the view.
- Drag a material from Visualization's list onto a body to give it that material.

Changed and fixed:
- The home screen follows the reference's colours and spacing: white title strip in a focused window, greyed when it
  isn't; the selected row turns blue while the sidebar has focus; a Show Projects Folder button where the reference
  has its account button.
- Default View (⌘1) and opening a file frame the model, small parts included.
- Hovered faces lighten as much as the reference's; rounded faces catch a faint highlight.
- Redrawn icons: Display Modes, Screenshot, History, Sketch, Insert, Tools, Visualize.
- The short line under History shows only with a selection or while sketching.
- The ground grid stays up while choosing a sketch plane in a project that already has sketches.
- Sketch-only projects get readable thumbnails on light cards.
- View animations take 0.33 s, as measured.

## 0.2.0-alpha

New:
- Parameter Cards: Fillet (with Curvature), Split Body (with Keep Originals); Show on Tool Start / History Edit;
  cards can be dragged.
- Fillet curvature profile slider; constant-chord fillets where the face angle varies.
- Link badge on Move and Rotate Around Axis copies; unlinked copies as their own History step.
- Eleven interface languages (toolbar, sidebars, cards).
- F cycles a running Fillet between Radius and Chordal; O cycles Offset Edge between Single and Chain.

## 0.1.0-alpha

The first alpha, as a Linux bundle: modeling, sketching with constraints, History, Visualization, Drawings,
STEP/STL/3MF/OBJ/glTF/DWG/DXF export, .shapr import and export, the control channel and the MCP server.
