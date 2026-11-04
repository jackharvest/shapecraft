# Changelog

How versions are numbered: [`docs/versioning.md`](docs/versioning.md). Newest first.

## 0.5.6-alpha

Fixed:
- Offset Face on a round face (a shaft, a hole) now offers Offset, Radius and Diameter, starting on Diameter.
  The value is exact (a 10 mm cylinder reads "⌀10 mm", not 9.98), the dashed line runs to the axis for Radius and
  across to the far side for Diameter, and the value comes before the type. Plain Offset shows only the arrow
  until you pull it. The arrow itself is a shorter, lighter double arrow.
- Selecting a round face shows its diameter at the bottom ("⌀ 10 mm"); 0.5.3 showed the radius.
- Cancel and Escape after pulling a tool (Extrude, Offset Face and the rest) now throw the result away. Before,
  they kept it as if you had pressed Done.
- Pushing a sketch into a part shows the material that will be cut away in red while you pull, and the Result
  badge says Subtract the whole time instead of flipping to New Body once the cut appears.
- History dims its filter and steps while a tool's result waits for Done, and while you sketch it dims every step
  but the sketch you're in. A new sketch with nothing drawn in it isn't listed yet.
- The view cube's edges look cut rather than rounded, its axes are shorter, and clicking near a face's edge or
  corner picks that edge or corner view without having to hit the thin bevel exactly.
- Tool hints now say what to pick (faces, edges, a profile) instead of how to drag.
- Fill (Beta) has its own icon, and it and the sketch Fillet carry a small blue "new" dot, as do the Tools and
  More buttons that hold them. The Tools list now shows Fillet and Chamfer separately and includes Fill. Scale and
  Project show their S and P keys.
- Projects keep each body exactly as it was saved: a surface body, or one body made of separate pieces, could
  come back wrong or missing. A project whose bodies file is missing says so and rebuilds its bodies from the
  History instead of failing with "STEP import failed".

## 0.5.5-alpha

Fixed:
- Long names in the Items and History lists ran out of the panel into the model. They now end in "…", and
  project names on the home screen stop at two lines.
- A project whose saved bodies file was damaged wouldn't open at all ("STEP import failed"). It now opens, its
  bodies are rebuilt from the History, and a message tells you; the damaged file is kept beside it.

## 0.5.4-alpha

Fixed:
- After an update, ShapeCraft could sit with no window for as long as macOS was waiting for you to allow access
  to the Documents folder (where projects live), so it looked stuck. The window now opens right away, the home
  screen says it's opening your projects folder, and it tells you to choose Allow if macOS asks. If access was
  turned down, it says where to turn it back on instead of showing an empty home screen.

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
