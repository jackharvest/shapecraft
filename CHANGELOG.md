# Changelog

How versions are numbered: [`docs/versioning.md`](docs/versioning.md). Newest first.

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
