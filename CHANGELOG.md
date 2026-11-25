# Changelog

How versions are numbered: [`docs/versioning.md`](docs/versioning.md). Newest first.

## 0.12.12-alpha
**Big models turn smoothly on slow computers, pens and touch screens work on Linux, moved bodies in `.shapr` files are where they
belong, and other languages fit.** Much of this comes from BAR × SUK's careful test of the Android app in Russian. Thank you!
### New
- **Big models draw coarser while the view moves**, and whole again the moment it stops. It only goes as far as your computer
  needs to keep up, so on a fast one nothing changes: a 1.9-million-triangle model turned at 30 frames a second on a 2016
  laptop and now turns at 58. Preferences ▸ General ▸ Coarser While Moving: Automatic, Always or Never (#25).
- **Pens on Linux** (Wayland): a pen hovers, picks and draws, its button is a right-click, and it never turns the view.
  Thanks [@johanl19](https://github.com/johanl19) (#97).
- **Touch screens on Linux** (Wayland): one finger turns the view, two pan and pinch; a tap picks and holding opens the
  right-click menu until a pen is used, which then does the picking. With no keyboard in use, sizes are tapped in on the
  numpad. Thanks [@johanl19](https://github.com/johanl19) (#97).
- **KDE's global menu** shows ShapeCraft's menus (Plasma on Wayland), and the window leaves its own row out while it does.
  Thanks [@johanl19](https://github.com/johanl19) (#97).
- **A fillet or chamfer can stop at the biggest size the part can take** while you pull it, instead of going sharp again:
  Preferences ▸ General ▸ Fillet and Chamfer. A cylinder's end pulled all the way is then a dome.
- **Colors change on the model as you drag** in Visualization, for bodies, faces and the backdrop. The whole drag is one undo.
- **Preferences on the projects screen of a tablet**, at the bottom of the sidebar.
- **A pen without a button can open the right-click menu**: Preferences ▸ Tablet ▸ Hold still, then lift.
### Changed
- **No window buttons on a maximized window** where KDE takes the borders off maximized windows. Thanks
  [@johanl19](https://github.com/johanl19) (#97).
- **Plugins, Advanced, reports and the update notes follow your language.** Their translations were there but weren't used.
  The last missing texts are in all eleven languages.
- Russian: Visualize and Decal have their proper names.
### Fixed
- **Insert ▸ File and Insert ▸ Project open their windows.** File said `'path' must be a string`, and Project "is coming soon".
  Thanks [@Staratel](https://github.com/Staratel) (#98).
- **Everything command search lists can be run from it**: Import, Export, Save, Insert Image, Create Drawing and others
  complained about a missing value. What only works from its own panel is no longer listed (#98).
- **Moved bodies in a `.shapr` file are where they belong.** Bodies a Move/Rotate had placed could come in unmoved, at
  another body's place, or not at all; copies made by one were missing. Moves, rotations, copies and deletions are now read
  for exactly the bodies they name, and saving keeps them. Thanks [@pablohc](https://github.com/pablohc) (#45).
- A faded or picked body's nearest surface could show speckles on some graphics cards.
- **Long words stay in their place**: Preferences' tabs, labels, pop-up buttons and Sync's rows; the buttons under the Items
  list; Visualization's pop-ups and a decal's file name; the Area Selection card.
- **What's New swipes sideways on a tablet**, and the Tutorials page scrolls under the title bar, not over it.
- Normal to Sketch no longer covers the project's name or a tutorial's card.
- A circle's diameter is easy to read: its dashed line stops for the number.
- The Metal graphics choice was translated as the material.

## 0.12.11-alpha
**Patterns you can make with the mouse alone, and long edges that can always be picked.**
### New
- **Circular Pattern has handles.** Three turning arrows stand around a dot on what you picked: pull one and the copies
  follow. Drag the dot first to go around somewhere else (it snaps to face and hole centers, corners and edge middles, or
  anywhere on a sketch), or click an edge, an axis or a sketch circle. The angle, the quantity, Total or Spacing, and Rotated
  or Uniform sit next to the handle. Thanks [@micide0606](https://github.com/micide0606) (#96).
- **Click another body while a pattern waits** to take it in, or to leave it out.
### Changed
- **Return takes a pattern's typed value and keeps going**; Return again (or Done) finishes. A click anywhere else takes it
  too, and the copies stay on screen while you type.
- Tab opens a pattern's quantity straight away. The words on the pills follow your language.
### Fixed
- **A pattern's quantity stays what you typed.** Pulling an arrow put it back to 3×, and Done used it as the spacing. Done now
  also takes a typed angle in Circular Pattern and Revolve. Thanks
  [@micide0606](https://github.com/micide0606) (#96).
- **A pattern's pills stay in reach**: they could end up under the History list or past the window's edge, where they couldn't
  be clicked.
- A quick pull on a Linear Pattern arrow turned the camera instead.
- **Long edges couldn't be picked near their middle in a big window**: the face next to them was. One step of the phone
  stand tutorial couldn't be finished on a large screen because of it.
- Pressing Tab while a tool waited for a value could leave the keyboard dead.

## 0.12.10-alpha
**Nine new tutorials you can click through, the whole app in twelve languages, and a bar that shows your project saving.**
### New
- **Tutorials, rebuilt.** Three beginner courses (a coaster, a pencil cup, a phone stand), then one for each tool worth
  learning: Revolve (a drawer knob), Loft (a flower pot), Sweep and Union (a coat hook), Pattern and Subtract (a birthday
  cake), Mirror (a serving tray) and Thread (a bottle neck). An animated pointer shows every move on the very thing to press,
  the step plays in the corner, sizes can be set with a button, and Do It for Me helps when you're stuck. Keep the result
  as a project of your own when you're done.
- **Saving shows how far along it is**, in a bar at the bottom right, whenever it takes a moment.
- **The whole interface in German, French, Italian, Portuguese, Russian, Japanese, Korean and Chinese**, the new texts in
  Spanish and Slovak too. Messages with a name or a number in them follow your language as well, and so do plugins, reports
  and the failure notes. Corrections are very welcome:
  each language is now a file of its own that opens in the usual translation tools, with a
  [guide](https://github.com/jackharvest/shapecraft/blob/main/docs/translating.md) for translators.
### Changed
- **Save, Close and Quit no longer freeze the window on a big project.** Saving runs in the background; closing waits for it
  with the bar showing. A save that fails keeps the project open and says why.
- The Tutorials page scrolls and wraps to the window.
### Fixed
- **Revolve, Loft, Sweep, Shell and Plane - Offset can be finished with a click.** They showed only Cancel: without a
  keyboard's Return there was no way to apply them.
- A tutorial could leave you stuck: a click on a sketch's outline opened it with nothing to do there, the sample part next to
  yours could be picked instead, and some steps needed the Space bar or Return.
- Short messages at the bottom were measured in your language but shown in English.

## 0.12.8-alpha
**A hovering stylus lights what it's over on Android, and Spanish is complete.**
### New
- **Plane squares on touch screens** (Preferences ▸ Tablet ▸ Show the plane squares when starting a sketch): tap one with
  the pen to sketch on that plane, upright ones too. Off, the pen's first stroke picks the floor or a face, as before.
  Thanks [@Capdan83](https://github.com/Capdan83) (#95).
- **The interface is in Spanish throughout**: settings, tips and suggestions included. Corrections are very welcome.
### Changed
- **A sketch hides once all its curves have been used.** Extrude a ring and the circle inside it goes along; extrude every
  letter of a text and their holes do. Separate profiles still wait their turn, and Preferences ▸ General ▸ Sketches ▸
  Stay shown until every region is used keeps it until the very last one.
- Hints on a touch screen follow your language. Reworded for the pen, they used to fall back to English.
### Fixed
- **A stylus hovering over the screen on Android lights corners, middles, centers and faces**, before it touches. The view
  wasn't redrawn until something else moved. A mouse's hover, buttons and wheel answer at once there too.
  Thanks [@Capdan83](https://github.com/Capdan83) (#95).
- A ring counted as used once the circle inside it was (they share a center), so the sketch could hide too early.
- The Project button said "to project" in languages that tell the two apart.

## 0.12.7-alpha
**3MF files open, threads come out right, and there are two new sketch tools: Slot and Extend.**
### New
- **3MF import.** Every object on the plate comes in as a mesh body where it was placed, in a folder named after the
  file, with its color. Files from Bambu Studio and OrcaSlicer bring their part names and filament colors along, and
  their modifier and negative parts stay out. The README said we could already do this. Now it's true. Thanks
  [@movh](https://github.com/movh) (#93).
- **Slot** (Sketch ▸ Slot): click the two end centers, or the middle and one end, then pull out the radius. Type the length
  and the radius and they stay as dimensions. It's held together, so the sides stay tangent and the ends round.
- **Extend** (Sketch ▸ Extend): click a line or an arc near an end and it grows to the next curve in its way; click again
  to go on to the one after. A ring shows where it will end. Return keeps it, Escape takes it all back.
- **Trim cuts splines and ellipses.** What's left is a control-point spline on the same curve.
- **Export ▸ Resolution ▸ Custom**: type how far the triangles may be off the real surface (STL, 3MF, OBJ, GLB and the
  other mesh formats).
- **The whole interface can be translated**: menus, the projects screen, Preferences, hints, dialogs, export and drawings
  follow Preferences ▸ Language (Slovak is complete; the other languages have the new text in English until someone
  translates it). Thanks [@Hardo12](https://github.com/Hardo12) (#23).
### Changed
- **Return fixes a typed size and goes on to the next.** Type a slot's length or a rectangle's width and Return: it's
  held while you aim and size the rest. Return once more finishes.
- **One undo takes a drawn shape together with the sizes typed for it**, and the drawing tool stays in your hand.
- **Failures say what's wrong in plain words** ("The walls don't fit at this thickness. Try 9.9 mm.") instead of the
  kernel's own.
- Trim and Extend show the curves alone, without fills and points in the way.
- Construction curves are drawn dash and dot, round circles and arcs too. They could look solid before.
- A typed arc radius reads "R 2 mm" at the arc, not a diameter across it.
### Fixed
- **Threads come out right.** Some everyday sizes failed (an M6 × 10 bolt), lost part of the body or had small gaps
  in their mesh. The thread is now built directly and checked against its exact volume, in about half the time.
  Thanks [@mhdthoriq14](https://github.com/mhdthoriq14) (#94).
- **Union, Subtract and Intersect no longer fail when only merging the result's faces did.** You get the result, with
  its faces as they were cut.
- A rectangle's width fixed with Tab stays as a dimension when you click the corner.
- **Low resolution exports are low resolution.** They used to write the same triangles as High.

## 0.12.6-alpha
**Everything moves now: popups grow in and out, the side columns slide.**
### New
- **F2 renames the thing you picked.** Not the project. The thing you picked. Revolutionary, we know. It works on
  anything in Items, on a face or edge (its body) and on the projects screen; Tab goes on to the next name, ⇧Tab back.
- **A step that's too big says what fits.** When Shell, Fillet, Chamfer or Offset Face can't make the size you asked
  for, the message offers one that works ("Try 9.9 mm"), tried on your part first.
### Changed
- **Popups open and close with motion.** Every menu, popover, card, toast and sheet grows out of the spot that
  opened it and shrinks back into it when it closes, quick and decelerating in, accelerating away. Each one bounces:
  it grows a few points past its size and settles back, and swells a little before it's pulled away. A sheet over a
  dimmed window (Preferences, Keyboard Shortcuts) grows out of its middle while the dim fades in under it.
- **The side columns slide.** Hiding Items or History throws the column off the side after a small lean the other
  way; showing it springs it back in with a few points of overshoot, and the tools beside it ride along.
- Opening a project dissolves the projects screen into it (and closing one dissolves back). A newly picked
  Preferences tab fades in while the highlight glides over to it; History, Visualize and a drawing's properties
  fade into each other.
- **Buttons press in.** Tool buttons and the title bar's and Items' buttons squash a little under the pointer and
  spring back when let go; their hover highlights fade in and out instead of flicking.
- **The camera settles.** View changes, the view cube, Zoom to Fit and turning to a sketch plane run a hair past
  and settle, and the grid fades in going into a sketch and away leaving it.
- **Values pop onto the view.** Value pills and dimension boxes pop in from a little small, and a menu's rows come
  in one after another as it opens. The window menus (File, Edit, …) move about two and a half times faster than
  other popups, since they're opened often and in quick succession.
- **Lists glide.** History and Items rows slide to their new places when steps or items come and go, and a new
  message pushes the older ones up instead of jumping them.
- **Undo and Redo show what changed.** The bodies they change light up pale blue for a moment and fade back.
- A dialog opened over another (Navigation Presets over Preferences, export and import sheets) grows in on its own.
- **Preferences ▸ General ▸ Animations ▸ Fancy animations** turns all of this off, for slower computers. It's also
  off with the system's Reduce Motion (macOS; Animation effects on Windows).
### Fixed
- **A fault inside the geometry kernel no longer ends the app.** Offset Face on a swept tube (and Shell on one swept
  round a corner) could run OpenCASCADE into a missing edge and crash ShapeCraft. Such a fault now fails that one step,
  with a message, and your part stays as it was. Thanks [@movh](https://github.com/movh) (#91).
- **Trim takes only the piece you click.** Cutting part of a rectangle's side pulled the next side along, so it looked
  deleted. The other curves now stay put; what still holds on the rest is kept, and dimensions the cut made wrong go.
  Thanks [@micide0606](https://github.com/micide0606) (#90).
- **Union says when bodies don't touch.** Nothing was joined, nothing was said, and both bodies came out under one
  name. Now a message says so and each body keeps its own name; the step stays, and joins them if you later change
  them to overlap. Thanks [@jan-tdy](https://github.com/jan-tdy) (#92).
- The Escape that closes Preferences, Search or another popup no longer also reaches the workspace behind it (it
  used to leave the sketch too).
- **Renaming something inside a closed folder** opens the folder. It used to leave every key dead, with no field to
  type in. Reveal in Items Sidebar opens nested folders too.

## 0.12.0-alpha
**Open the parts other CAD systems save: SolidWorks, NX, Solid Edge and JT. Export FBX and VRML.**
### New
- **Open SolidWorks parts.** File ▸ Import File takes `.sldprt` files and brings in the part's solid bodies exactly,
  as if it were a STEP file: same faces, same sizes, ready to model on. A part with several configurations comes in
  as the first one. Parts saved from SolidWorks 2015 on read best; some older ones don't open yet and say why.
  No SolidWorks needed.
- **Open NX, Solid Edge and JT files.** File ▸ Import File also takes NX parts (`.prt`), Solid Edge parts and sheet
  metal (`.par`, `.psm`, `.pwd`) and JT files saved with exact geometry, and brings their solids in exactly. A JT
  assembly brings in every part, each where its own file puts it for now. A JT saved for viewing only (meshes, no
  exact geometry) says so instead of coming in empty.
- **Edges rounded against an edge come in.** Parts with a round that rolls along a face and an edge, which some
  Parasolid, SolidWorks, NX and Solid Edge files have, used to stop the import; they read now.
- **A second geometry kernel, in pure Rust (in testing).** Preferences ▸ Advanced ▸ Geometry Kernel picks between
  Open CASCADE (the default) and a new kernel written entirely in Rust, built on the open-source Arris kernel. It
  models exactly (sketches, extrude with draft, revolve, loft, sweep, threads; booleans, fillets, chamfers, shells,
  offsets, Offset Edge, deleting holes and pockets; drawings; STEP with part names and colors, IGES solids both
  ways, and the other files Open CASCADE opens) and passes 179 of the 190 modeling checks Open CASCADE does. Wrap &
  Emboss works on cylinders and cones as Open CASCADE's does (profiles reaching most of the way round included), and
  variable-radius fillets ease from one radius to the other the same way, on straight edges between flat faces.
  Fill, Project as Edge, moving or scaling an edge on its own and two-distance chamfers work in the common cases
  (flat openings, outlines that land inside a face, round rims and straight edges, chamfers on outside edges) and
  explain the rest. The Advanced tab says what each kernel is the better pick for and why ShapeCraft keeps two (no single point of failure), and lists what Arris does the same, what works with limits, and what still needs Open
  CASCADE; while it runs, setback corners are greyed out and say why. IGES files of loose trimmed surfaces still need Open CASCADE to open, and say so. Most modeling runs
  as fast or faster than on Open CASCADE (fillets, chamfers, shells and lofts several times faster); cuts into faces
  full of holes and threads are slower. Projects made with it live in a folder of their own, so your work stays on
  Open CASCADE while you try it. Restart to switch. For developers: `--kernel rust` or `SHAPECRAFT_KERNEL=rust`, and
  the command-line tools build without Open CASCADE at all (`cargo build -p shapecraft-cli --no-default-features`),
  checked by CI on every push.
- **Export to FBX and VRML.** File ▸ Export to ▸ FBX… writes a binary FBX that Blender, Unity and Unreal open with
  each body as its own object, in its color, the right way up and at its real size. VRML… writes a `.wrl` for older
  viewers and simulators.
### Fixed
- **Wrap & Emboss reaching most of the way round a cylinder** raised nothing (the profile came out on the wrong side
  of the face); it raises and engraves the whole profile now.

## 0.11.2-alpha
**Sketches you can shape by dragging, and a fillet's edges you can change afterwards.**
### New
- **Drag a sketch's curves straight in the view.** With no tool running, press on a curve and drag: a rectangle's
  side slides and the rectangle stays square, its corner moves with the opposite corner held, a circle's rim sets
  its size, a polygon or an ellipse grows or shrinks about its middle, and a loose line slides sideways. The sizes
  show as you drag, it snaps to the grid, and one undo takes it back. Before, the drag started a selection box.
  Thanks [@barnabasjohnthomas](https://github.com/barnabasjohnthomas) (#87).
- **A rectangle has a middle point.** It shows as a dot, and the pointer snaps to it as Center, so a circle or a
  line can start right in the middle without construction lines. Thanks [@micide0606](https://github.com/micide0606) (#86).
- **Change a fillet's or chamfer's edges afterwards.** In the step's card, Edge ▸ Edit... rolls the model back to
  just before the step with its edges picked: click edges to add or remove them, then Done. Escape leaves it as it
  was. Thanks [@barnabasjohnthomas](https://github.com/barnabasjohnthomas) (#89).
### Fixed
- **A value typed into a sketch dimension is kept when you click away.** It's applied as if you'd pressed Return;
  before, it was thrown away. Thanks [@barnabasjohnthomas](https://github.com/barnabasjohnthomas) (#88).

## 0.11.1-alpha
**Plugins and Report a Problem work on Android.**
### Fixed
- **Android: plugins can be added and go online.** The app now has Android's network permission and its own
  plugins folder, and it checks secure connections with the certificates it carries. Before, adding Generative
  Render, and Report a Problem, failed on Android.
- **Android and iPhone: links open.** Sign In with OpenRouter, the Free Chat sites and the plugin links now open
  in the browser.

## 0.11.0-alpha
**Plugins: add only what you want, and the first one puts your model in any scene, with your own AI and no limits
of ours.**
### New
- **Plugins.** ShapeCraft stays lean, and bigger optional features come as plugins you add from Preferences ▸
  Plugins ▸ Browse. Each one shows exactly what it may do before you add it (which sites it talks to, whether it
  sees the view), runs sealed off from your files and computer, and can be switched off or removed any time. Adding
  one shows its progress and what to do next.
- **Generative Render (a plugin).** In Visualization, describe a scene and get a picture of your model in it.
  - **Your own AI:** sign in with OpenRouter (one account, dozens of image models), paste an OpenAI or Google Gemini
    key, or run it free and offline on your own computer with ComfyUI (found by itself; FLUX.2 klein works on a
    laptop). No account at all? Switch the sheet to **Free Chat** to take your view to ChatGPT, Gemini, Copilot,
    Grok or Meta AI.
  - **Keep It Exact** keeps your model's own pixels, so the AI can't change its shape; **Let It Move** lets it
    float, turn or scale your model to suit the scene. On your own computer, ComfyUI repaints only
    the scene around your model, shadows included, so it stays exactly where it is with no second copy.
  - **History** keeps every picture on your computer, with what made it, when, and your words: Save, Copy, Use
    Prompt again, Delete. Pills on each picture say which service and model made it.
  - Every option is a tile with a small picture of what it does. 1, 2 or 4 pictures at a time, a prompt box that
    grows with your words, Edit and Try Again after a result, and a sheet you can move. On a phone held sideways the
    picture sits on the left and the options scroll beside it. ShapeCraft sets no quota.
  - See [docs/generative-render.md](docs/generative-render.md). Why a plugin: not everyone wants AI in their CAD
    app, and this way it's there only if you add it.
- **Assistants ask before they spend.** AI assistants connected to ShapeCraft (MCP) can now use plugins too, but
  anything that costs money on your account (a picture from OpenAI, Gemini or OpenRouter) waits for you to press
  **Allow** in ShapeCraft, with what it makes and what it sends. Free ones, like ComfyUI on your computer, run at once.
  See [docs/mcp.md](docs/mcp.md).
- **Write your own plugin** in Rust against a small, documented API; share it with a pull request and CI builds it
  for every release. See [docs/plugins.md](docs/plugins.md).

## 0.10.5-alpha
**Report a problem or suggest an idea without leaving ShapeCraft.**
### New
- **Report a Problem and Suggest an Idea, inside the app.** Help ▸ Report a Problem… opens a short form that becomes
  an issue on ShapeCraft's GitHub under your own name: sign in once with a code you approve on github.com, from any
  phone or computer. You choose what goes along (what failed, version and system, the recent log, a screenshot) and
  Preview shows the exact text first. Help ▸ Your Reports… shows what you've sent and whether it's been answered.
  See [docs/reporting.md](docs/reporting.md).
- **Report… on a failed step.** When a tool can't do something, its message offers Report…, with what the tool was
  asked to do already filled in. After ShapeCraft closes unexpectedly, the next start offers to report it.
### Fixed
- **Copy reaches other apps on Mac, Windows and Linux.** Show Log's Copy All and other copies stayed inside
  ShapeCraft.

## 0.10.4-alpha
**The Android download that 0.10.3 was missing, with that version's Android fixes.**
### Fixed
- **The Android .apk is back.** 0.10.3 went out without it. This one has 0.10.3's Android fixes: ShapeCraft starts
  on Android 8 to 12 and HarmonyOS again (thanks [@bootNoob](https://github.com/bootNoob), #78), and the numpad
  comes up for every value you tap on a touch screen (thanks [@Capdan83](https://github.com/Capdan83), #84).

## 0.10.3-alpha
**Android fixes from your reports: the app starts on Android 8 to 12 again, and the numpad comes up for every value
you tap.**
### Fixed
- **ShapeCraft starts on Android 8 to 12 (and HarmonyOS).** It closed straight away on anything older than Android 13,
  because it asked the system for something only Android 13 has. Building the app now checks that everything it
  needs is there on the oldest Android it supports. Thanks [@bootNoob](https://github.com/bootNoob) (#78).
- **The numpad comes up for every value field on a touch screen.** Tapping a sketch's dimension (a circle's diameter,
  a line's length, an angle) opened the field with no way to type into it unless a keyboard was attached. Now the
  numpad opens beside it, clear of the field, and a tap on the field's 123 key brings it back. Thanks
  [@Capdan83](https://github.com/Capdan83) (#84).

## 0.10.2-alpha
**.shapr projects from every year open and save, the .deb and .rpm update themselves, and a round of fixes from your
reports: the mouse wheel zooms on a Mac again, pinned measurements come off, and the booleans stay in reach.**
### New
- **The .deb and .rpm update themselves.** A Linux copy installed from the package now downloads the next one and
  installs it when you close ShapeCraft, asking for your password the way your desktop does. Thanks
  [@jan-tdy](https://github.com/jan-tdy) (#72).
- **Choose the bodies a cut works on.** In a cut's History card, Subtract From lists the bodies it cuts. Edit… lets
  you pick them in the view, × starts over, and Escape ends the picking and then closes the card. Thanks
  [@chickey](https://github.com/chickey) (#69).
- **The import screen shows what it's doing**: reading, repairing, building the bodies, with a running count and how
  long it has taken. Thanks [@chickey](https://github.com/chickey) (#70).
### Changed
- **Union, Subtract and Intersect sit higher with several bodies picked**, under Chamfer, so a short window keeps
  them in the column instead of behind More. Thanks [@jan-tdy](https://github.com/jan-tdy) (#75).
- **The Items list, reworked.** Picked rows join into one bright blue block, folders light up with what's picked
  inside them, and a hidden item shows a closed eye. A folder now has its own eye: hiding it hides what's in it without
  changing each item's own setting, so showing it again brings everything back as it was. Picking from the list is
  also much faster on big projects. Thanks [@pablohc](https://github.com/pablohc) (#77).
- **Booleans and History edits work in the background.** Union, Subtract and Intersect, and editing, suppressing or
  deleting a step, moving the breakpoint or changing a variable, show a card with Cancel when they take a while, and
  the window keeps drawing, so a slow computer no longer says ShapeCraft isn't responding. Leaving a sketch opened
  from the History without changing it no longer rebuilds the steps after it. Thanks
  [@jan-tdy](https://github.com/jan-tdy) (#59) and [@chickey](https://github.com/chickey) (#74, #76).
### Fixed
- **The mouse wheel zooms on a Mac** with mice that scroll smoothly (Logitech and others), instead of turning the
  view. Thanks [@movh](https://github.com/movh) (#81).
- **Pinned measurements come off** with their bin again. Thanks [@movh](https://github.com/movh) (#82).
- **A picked tool's icon reads clearly** on its white button in the dark theme. Thanks
  [@movh](https://github.com/movh) (#83).
- **.shapr files saved this year keep their names, folders and visibility** on import. Thanks
  [@pablohc](https://github.com/pablohc) (#79, #80).
- **One more kind of rounded offset body comes in** from big .shapr projects. Thanks
  [@chickey](https://github.com/chickey) (#71).
- **Older .shapr projects open.** Projects from before 2020, 2018 sketches, shape groups, 2024 projects saved
  without their cached history, and projects with deleted pieces left inside all import now, with their bodies,
  sketches and folders. Saving an older project writes a new project in today's layout instead of failing.
- **Saving keeps every body as one body.** Sheets, wires and bodies made of several solids are written as one body
  each, and a mesh body keeps triangles that sit outside its shell.
- **Round walls on freeform surfaces come in whole.** A wall that runs all the way round a pocket or hole on a
  freeform surface was cut into slivers on import and went missing. It now comes in as one face, and a save and
  reopen gives the same part.
- **Empty sketches survive a save.** A named sketch plane with nothing drawn on it yet stays in the project when you
  save it as .shapr.
- **Projects saved in 2024 come in as they were left.** Their history is read too, so deleted bodies and planes stay
  deleted, construction planes are planes again rather than bodies, and moved, mirrored and split bodies keep
  their names, folders and shown/hidden state. One such project failed to open at all.
- **More rounded edges read.** A fillet whose stored path runs on past the faces it rounds (common where a fillet
  meets a curved face) failed to come in, so the body was left out. It now comes in, matching the original's volume
  to within a few thousandths of a percent.
- **Domes, rounded rims and seam-crossing walls measure right.** Some curved faces from .shapr files came in empty or
  inside out: a dome closing to a point, a face bounded by a closed spline stored without end knots, and a short
  wall on a round face that crosses the place where the round surface starts over. One wallet project had bodies
  off by up to the whole body; they now match the original to within a hundredth of a percent or so.
- **Hollow lofted ducts come in.** A shelled body whose wall is an offset of a lofted surface was left out of the
  project ("offset of a C0 surface"). It now comes in, within a twentieth of a percent of the original's volume.
- **The oldest projects name their bodies as they were.** A body from a 2018 project is called "Body".
- **A copied body keeps its own name.** Move/Rotate with Copy makes a new body; it no longer takes the name and
  visibility of the body it was copied from.

## 0.10.1-alpha
**The iPhone app is back in the release, and on an ordinary iPhone it's a viewer: the whole modeler stays on the
iPhone Duo.**
### Changed
- **Other iPhones view projects.** On an iPhone that doesn't fold, a project opens sideways to look at: one finger
  turns, two pan and pinch, a double tap looks straight at a face, with the standard views, Display Modes and
  Screenshot on the right and Export under the project's name. Files from the Files app open to view too. Tools,
  new projects, tutorials, Rename and Undo stay on a computer or an iPhone Duo, which keeps the full modeler. A
  card on the first project says so.
### Fixed
- **The iPhone .ipa is on the release again.** 0.10.0 went out without it.

## 0.10.0-alpha
**Threads: pick a hole or a cylinder and it becomes a nut or a bolt, ready to print.**
### New
- **Thread.** Pick a cylindrical face and press Thread (after Visualize in the face's tools, last under Tools, or
  Tools ▸ Thread): a hole gets an internal thread, a cylinder an external one, at once. Choose ISO metric (M0.35 to
  M600, coarse and fine pitches), ANSI inch (#0 to 4", UNC/UNF/UNEF) or ACME, right or left hand, the whole face or
  a custom length from either end. The size starts at the one nearest the face; change it from the pill on the model,
  by typing a diameter, or in the card behind the button's ≡. Threads are modelled at the standard's medium fit
  (6g bolt, 6H nut), and a Thread step in History changes later like any other.
- **Print Fit for threads.** FDM prints swell, so a bolt and nut modelled exactly can jam. Print Fit (Snug 0.1 mm,
  Normal 0.2 mm, Loose 0.3 mm, or your own Clearance) makes a bolt that much smaller and a nut that much larger all
  round. ShapeCraft remembers the setting for your next thread, so both parts get it.
- **Bolts and nuts line up.** Every thread of the same pitch on the same axis line follows one helix, so a bolt and
  its nut threaded separately, at any length or height, screw together without turning one of them to match.

## 0.9.4-alpha
**Big boards come in with their colours, sketching on them is fast, and editing an early import no longer means
waiting for it all over again.**
### New
- **Imports keep colours set on single faces.** STEP files and .shapr projects that colour faces one by one (a PCB's
  solder mask, pads and sockets) come in with those colours, any number of them per body, and STEP exports write
  them back out. Thanks [@chickey](https://github.com/chickey) (#65).
- **Offset Edge on a face's outline.** Sketching on a face, pick edges of the body lying on the sketch and Offset
  Edge them (Chain grows them to the whole outline); clicking the lit button of the running sketch tool puts it down
  so you can pick. Double-clicking the body there now leaves the sketch with the body selected. Thanks
  [@chickey](https://github.com/chickey) (#67).
- **Windows on Arm download.** The native Arm64 MSI and portable zip that missed 0.9.3 are in this release. Thanks
  [@Hobbesdcc](https://github.com/Hobbesdcc) (#61).
### Changed
- **Changing a History step only reruns that step and the ones after it.** On a big import, changing a later
  extrude went from about a minute (the whole file was read again) to a fraction of a second. Thanks
  [@chickey](https://github.com/chickey) (#68).
- **Sketching on a face of a big model is smooth**, and the model stays solid while you sketch on it, its colours
  softened, so a dark board doesn't vanish. Thanks [@chickey](https://github.com/chickey) (#67).
- **Shell, Fillet and Chamfer work in the background with Cancel** when they take a while, as Offset Face does, so
  the window keeps drawing on a slow computer. Thanks [@jan-tdy](https://github.com/jan-tdy) (#59).
### Fixed
- **.shapr import: closed walls stored as rim rings come in whole.** A body whose wall lost a strip and turned
  invalid now imports with the right volume and size. Thanks [@pablohc](https://github.com/pablohc) (#63, #64).
- **.shapr import: bodies sharing a name land in their own folders**, and a hidden folder hides the bodies it holds,
  not others. Thanks [@pablohc](https://github.com/pablohc) (#66).

## 0.9.3-alpha
**Offset Face no longer freezes the window on big parts or slow computers, and Windows on Arm gets its own download.**
### New
- **Windows on Arm.** Each release now has its own Arm64 download (MSI and portable zip) for Snapdragon and other Arm
  laptops, so ShapeCraft runs natively instead of through emulation, and updates pick the Arm64 file. Thanks
  [@Hobbesdcc](https://github.com/Hobbesdcc) (#61).
### Changed
- **Offset Face works in the background when it takes a while.** A quick offset lands at once as before; a long one
  shows a small card with how long it has been working and **Cancel**, while the window keeps drawing (so the
  desktop no longer says ShapeCraft is not responding). Its live preview is worked out in the background too and
  gives up after 20 seconds instead of holding the window. Thanks [@jan-tdy](https://github.com/jan-tdy) (#59, #52).

## 0.9.2-alpha
**More from your reports: a built-in log, a real-size view, and whole texts picked in one go.**
### New
- **Help ▸ Show Log…** shows what ShapeCraft said while it ran (each step and how long it took, warnings, errors),
  with Copy All and Show in Folder. It's also kept as `shapecraft.log` next to your settings, so you no longer need a
  terminal to see it. Thanks [@jan-tdy](https://github.com/jan-tdy) (#54).
- **View ▸ Actual Size** zooms so 1 mm of your model is 1 mm on the screen, to hold a part or a ruler against. The
  first time, a bank card drawn on screen is matched to a real one (View ▸ Calibrate Actual Size… to redo it).
  Thanks [@jan-tdy](https://github.com/jan-tdy) (#62).
- **Right-click a letter ▸ Select Whole Text** picks every letter of a text at once, leaving out the insides of
  letters like O and B, ready to extrude. Thanks [@jan-tdy](https://github.com/jan-tdy) (#49).
### Changed
- **Tutorials: a new instruction swells and glows for a moment** at the top of the window, so it isn't missed next to
  the picture at the bottom left. Thanks [@jan-tdy](https://github.com/jan-tdy) (#48).

## 0.9.1-alpha
**Fixes from your reports: sketches stay out while they still have profiles to use, text is easier to set up, and an
emptied body can't block extruding any more.**
### Changed
- **A sketch stays shown until every profile in it is used.** Extrude one of four holes and the other three stay
  there for the next pull; the sketch hides once the last one is used. Thanks [@jan-tdy](https://github.com/jan-tdy) (#53).
### Fixed
- **"bounds failed: shape has no extent" no longer blocks extruding.** A cut that would take away all of a body is
  refused with a message instead of leaving an empty body behind, and an empty body already in a project is passed
  over, so new sketches extrude again. Thanks [@jan-tdy](https://github.com/jan-tdy) (#50).
- **Scrolling the Add Text font list scrolls the list.** The view behind the Add Text dialog no longer zooms while you
  scroll over the dialog or its font list. Thanks [@jan-tdy](https://github.com/jan-tdy) (#57).
- **Capital Height takes any value you type**, like 0.5 mm or 1.5 mm. Before, a value starting with 0 or holding a
  decimal point couldn't be typed. Thanks [@jan-tdy](https://github.com/jan-tdy) (#58).
- **.shapr import: a closed wall stored as two faces joins into one**, so a solid that came in invalid, with the wrong
  volume and size, imports whole. Thanks [@pablohc](https://github.com/pablohc) (#55, #56).

## 0.9.0-alpha
**ShapeCraft on iPhone: the whole modeler as a native app, laid out for a phone, iPhone Duo included.**
### New
- **A native iPhone app.** The same modeler, kernel and files as on a computer, in a layout made for a phone: icon
  columns down the sides, menus as iOS sheets, the system font, and buttons kept clear of the status corner and the
  home bar. Download the `-ios-arm64.ipa` from the release and sideload it with AltStore, Sideloadly or a similar
  tool, signed with your own Apple account, or build it yourself ([docs/ios.md](docs/ios.md)).
- **Fingers move the view, a stylus works the model.** One finger turns the view, two pan and pinch, a double tap
  zooms to a face and a three-finger swipe undoes or redoes. A stylus taps, draws and drags handles, and a drag on
  empty space draws a selection box. The stylus never moves the camera.
- **Made for the iPhone Duo.** Folded, the front screen holds your projects upright. Open it up to model on the inside
  screen. A project started on the front screen waits with an "Open up to model" card and picks up when you unfold.
  The projects screen puts its buttons in iOS's vertical bar, and its pages scroll and glide like iOS.
- **On other iPhones** projects show upright, and a project turns the phone sideways.
- **Import and Export use the Files app** on iPhone, and projects live where the Files app can see them.
### Changed
- **On every touch screen a stylus works the model and never moves the view** (Android and the iPad page too): a
  stylus drag on empty space draws a selection box instead of turning the view. Fingers move the view.
### Fixed
- **The "STLs stay light" card on the home screen shows its picture.**

## 0.8.7-alpha
**One Mac download for every Mac, sturdier .shapr and STEP imports, and a round of everyday fixes: Measure snaps and
moves its points, + and - zoom, and sketches can be copied.**
### New
- **Measure's points snap and move.** In Point-to-Point Distance and 3-Point Angle a click near a corner, an edge's
  middle or a circle's center takes that exact point, and a placed point can be dragged to another one. Thanks
  [@jan-tdy](https://github.com/jan-tdy) (#33, #36).
- **+ and - zoom the view** in and out, about the pointer. Thanks [@jan-tdy](https://github.com/jan-tdy) (#34).
- **A picked round face shows its height beside its diameter** in the bottom pill ("1 face | ⌀ 14 mm | ↕ 18 mm").
  Thanks [@jan-tdy](https://github.com/jan-tdy) (#35).
- **Sketches can be copied and pasted.** Pick a sketch in Items, Copy (⌘C / Ctrl+C) and Paste: a whole copy lands in
  place ("Sketch 01 (1)"), ready to move. Thanks [@jan-tdy](https://github.com/jan-tdy) (#37).
### Changed
- **The Mac download is now a single universal app** that runs natively on Apple silicon and Intel Macs, so there's
  no longer a choice to make on the download page. Copies already installed update into it on their own, as usual.
- **Large models stay smooth when the pointer moves or much is picked:** only the bodies whose edges change are
  redrawn, and a big selection is looked up once per frame (about 93 ms to 35 ms per pick change on 1,763 bodies).
  Thanks [@chickey](https://github.com/chickey) (#29).
- **A .shapr import keeps the file's own folder tree,** every folder in its order with each body where the file lists
  it, and a STEP assembly comes in as nested folders instead of one flat folder per path. Thanks
  [@pablohc](https://github.com/pablohc) (#43).
### Fixed
- **On Linux the window keeps answering while a file dialog is open,** so the desktop no longer offers to close the
  app ten seconds into browsing. Thanks [@pablohc](https://github.com/pablohc) (#41).
- **A STEP file with bodies that have no triangles no longer closes the app when it's shown.** Thanks
  [@pablohc](https://github.com/pablohc) (#39, #40).
- **.shapr bodies on the offset of a sweep carried on by a straight piece import** instead of being left out as
  "offset of a C0 surface". Thanks [@pablohc](https://github.com/pablohc) (#42, #44).
- **A size typed right after placing a circle's center (or a rectangle's corner) shows its field** before the pointer
  moves; Return makes the shape at that size. Thanks [@jan-tdy](https://github.com/jan-tdy) (#32).
- **Undo and Redo from the window menu (Ctrl+Z on Linux and Windows) put the running tool down first,** as ⌘Z and the
  on-screen buttons already did. Thanks [@jan-tdy](https://github.com/jan-tdy) (#38).

## 0.8.5-alpha
**Large projects import, save and select much faster, and .shapr imports bring in more of the model.**
### Changed
- **Big projects are much quicker to work with.** A .shapr import rebuilds its bodies on several cores (157 s to 38 s
  on a project of about 1,700 bodies), saving no longer slows down with the square of the body count (over ten
  minutes to about 14 s), picking something with History open no longer replays the whole history on every click,
  and autosave writes in the background so the window keeps answering. Thanks
  [@chickey](https://github.com/chickey) (#28).
### Fixed
- **.shapr imports bring in imported bodies that nothing changed afterwards,** which could be missing before, give
  bodies that a later step changed the names they have in the file, and accept a face on the offset of a sweep whose
  path changes speed (it was refused as a crease). Thanks [@chickey](https://github.com/chickey) (#27).

## 0.8.4-alpha
**.shapr imports with slow free-form bodies finish in seconds, not minutes.**
### Fixed
- **Importing a .shapr file no longer spends minutes measuring free-form bodies.** Each body gets a few seconds to be
  measured exactly; one that would take longer is measured from its triangles instead, so the import finishes. Exports
  back into the file still recognise it. Measure does the same, and says when a volume is approximate. Thanks
  [@chickey](https://github.com/chickey) (#26).
- **Quitting while such a measurement was still running in the background could crash on the way out;** it now
  closes cleanly.

## 0.8.3-alpha
**Box selection in a sketch picks its curves and points, picked profiles are easier to read, and imports get a proper
screen.**
### Changed
- **While a file imports, the window shows it:** a calm full-window screen with the file's name and a turning badge,
  and Cancel underneath (or Escape), instead of a small card over the project.
- **A box dragged inside a sketch filters by All Items, Edges (E), Points (P) and Constraints (C).** It picks the
  sketch's curves or their points, not its regions; to pick regions with a box, exit the sketch and use Faces (F).
  This replaces the 0.8.1 change for #19. Thanks [@leo-smi](https://github.com/leo-smi) (#19).
- **Picked sketch profiles are a darker orange,** brighter while the pointer is over them and fainter while a box is
  still choosing them, so the lines and the Extrude arrow stay easy to see.
### Fixed
- **Normal to Sketch no longer sits on top of the Area Selection banner;** it moves below it while you drag a box.
- **Pressing Return in Import Preferences, Export or Import Project no longer also opens Search Commands.**

## 0.8.1-alpha
**Big imports no longer freeze the window, more .shapr projects come in whole, and Slovak.**
### New
- **Imports run in the background.** A big STEP, IGES, Parasolid or .shapr file no longer freezes ShapeCraft while it
  comes in: a card shows the file and the time so far, and **Cancel** stops it and leaves the project as it was.
  Thanks [@chickey](https://github.com/chickey) (#22).
- **Slovenčina** in Preferences ▸ Language. Thanks [@Hardo12](https://github.com/Hardo12) (#17).
### Changed
- **Faster orbiting on big models.** Bodies outside the view aren't drawn, and closed solids skip the back of their
  faces (sheets, open meshes and Section View still show both sides). Thanks [@chickey](https://github.com/chickey)
  (#24).
### Fixed
- **Box selection in a sketch takes its regions with Faces (F).** Dragging a box over a sketch you're drawing and
  pressing F used to pick nothing. Thanks [@leo-smi](https://github.com/leo-smi) (#19).
- **Importing a .shapr file with very dense free-form surfaces no longer hangs** working out their volume: such a body
  is measured from its triangles in seconds. Thanks [@chickey](https://github.com/chickey) (#20).
- **STEP import with Quality healing can't hang on one bad shape any more.** A repair that runs past 20 seconds is
  tried again without its slowest step, and after that the shape comes in unrepaired, with a note saying how many.
  Thanks [@chickey](https://github.com/chickey) (#21).
- **.shapr projects come in whole more often:** lemon-shaped tori (an enclosure lost 6 of its 29 parts), offset
  surfaces on smooth B-spline faces (a chair lost 3 parts), a torus face that came in inside out, and construction
  planes made from a face offset all read correctly now.
- **Exporting back into a .shapr project says when an edit can't be kept in that file** (a rename or color change
  on a body it can't place) instead of quietly dropping it.

## 0.8.0-alpha
**STL and OBJ files stay light mesh bodies, .shapr projects with meshes open in seconds, and new ways to install.**
### New
- **Mesh bodies.** STL and OBJ files come in as mesh bodies, as they are, in a folder named after the file ("Mesh body
  01"): a 20,000-triangle part takes a second instead of minutes. Click one to pick it whole; Move/Rotate, Scale,
  Patterns, Translate, Rotate Around Axis, Mirror, Split Body and Visualize work on it, and Union, Subtract and
  Intersect combine it with other bodies (mesh or not) while it stays a mesh. Open (unclosed) meshes take no
  booleans. Tools that need exact faces (Fillet, Shell, Offset Face…) don't show for a mesh.
- **Sketch next to a mesh:** Sketch on Face (right-click) sketches on the triangle under the pointer, and an extrude
  from there makes its own body, ready to Subtract from the mesh or Union onto it. Measure, Section View and Wireframe
  (which shows a mesh's outline) work on mesh bodies too, and exports include them.
- **Windows: an MSI download.** Double-click it to install ShapeCraft for yourself, no administrator needed (or for
  everyone with `msiexec /i … ALLUSERS=1`). It shows up in Apps & features and updates itself like before.
- **Linux: .deb and .rpm packages** for Debian, Ubuntu, Fedora and openSUSE. They put ShapeCraft in your app menu and
  `shapecraft` on your PATH; install the next version's package to update.
### Changed
- **Projects keep mesh bodies in a small file of their own** (a `meshes` folder) instead of the STEP file, so saving is
  quick. Projects with mesh bodies need this version or later to open.
- **Windows copies from the setup program move to the MSI at their next update,** in the same folder, with your
  shortcuts and settings kept. The setup program download stays for a few more versions and then goes away.
### Fixed
- **Opening a .shapr project with mesh bodies (imported STLs) no longer freezes ShapeCraft** (Windows called it "Not
  Responding"): every triangle used to become an exact face, and saving those took minutes. They stay mesh bodies now.

## 0.7.10-alpha
**Move arrows take a click, and tablets get easier sketching and tutorials you can finish.**
### Fixed
- **Clicking a Move/Rotate arrow picks that arrow** and opens its value box, ready for a distance or angle, instead of
  selecting whatever was behind it (or clearing the selection). The same goes for Scale and Pattern handles,
  Rotate Around Axis and the single arrow of Extrude, Offset Face, Chamfer/Fillet and the others. Typing into Move
  now shows the full value box at the arrow, with Copy moved past it. Thanks
  [@ryansinclair14](https://github.com/ryansinclair14) (#18).
- **Tablets: a finger turning the view no longer drags an unfinished line or rectangle along.** It stays where the
  pen left it. Thanks [@joravarsinghing](https://github.com/joravarsinghing) (#14).
- **Tablets: Line only turns into a circle for a quick round stroke.** Steering a line to fix its angle or length
  keeps it a line, and Preferences ▸ Tablet ▸ Pen turns the circle gesture off. Thanks
  [@joravarsinghing](https://github.com/joravarsinghing) (#14).
- **Tutorials on tablets can be finished.** The step's instruction shows under the project name instead of hidden
  behind it, the words talk about taps, the pen and the value keypad instead of keys, and a step that needs a
  sketch on a face offers Sketch. Thanks [@joravarsinghing](https://github.com/joravarsinghing) (#15).
### New
- **Tablets: Cancel for an unfinished shape.** While a line, rectangle, circle or other shape is half drawn, Cancel
  at the bottom drops it and keeps the tool; lines you finished stay. Thanks
  [@joravarsinghing](https://github.com/joravarsinghing) (#14).
- **Tablets: Undo and Redo in tutorials,** under Retry, so a slip takes one step back instead of starting the step
  over. Undo stops where the step began. Thanks [@joravarsinghing](https://github.com/joravarsinghing) (#16).
- The on-screen Undo and Redo on tablets end the running tool first, as ⌘Z does.

## 0.7.9-alpha
**SVG export now writes your sketches as clean, true-size outlines for laser cutters, CNC and vector apps.**
### Changed
- **File ▸ Export to ▸ SVG… exports sketches, not the drawing sheet.** Each visible sketch (hidden ones too with
  Include Hidden Items) is drawn face on, whatever plane it's on, as a thin black outline. Several sketches give
  one SVG each, named after them, in a .zip. The menu item saves straight away with your last SVG settings; the
  Export dialog's SVG row shows them and says what will be drawn.
- **True size in any app.** The file gives its width and height in millimetres, so Inkscape, Illustrator, Shaper
  Origin and laser software open it at real size, whatever DPI they assume. DPI (96) still sets the pixel grid.
- **Curves stay curves.** With Keep Curves (on), arcs, circles and ellipses are written as arcs and splines as exact
  Bézier curves, instead of hundreds of short lines. Off, they're split into points at the Resolution you choose
  (0–100).
- **Join Segments** (on): connected lines and curves become one shape, closed when they meet, so an outline cuts in
  one pass. Off: each line, arc and circle is its own shape.
- **Export what you pick.** Select sketches, sketch curves, regions or flat faces of a body before exporting and
  only those are drawn (a face as its outline). With nothing picked, every visible sketch is.
- **Text is exported as its letters' outlines**, ready to cut or engrave.

  A drawing sheet still exports as SVG from the Drawings workspace's Export.

## 0.7.7-alpha
**ShapeCraft for Android comes with each release, signed so later versions install over it, and a simpler first run on touch screens.**
### New
- **An Android APK on the Releases page** (`shapecraft-<version>-android-arm64.apk`, arm64, Android 8.0+). Every
  version is signed with the same release key, so a new one updates the installed app and keeps your projects. A copy
  from an earlier test build (debug-signed) has to be uninstalled once first. Follow-up to
  [@joravarsinghing](https://github.com/joravarsinghing)'s #3 (#8).
- **Ready for Google Play later.** When ShapeCraft comes to the Play Store, it will use the same app ID and the same
  signing key as these APKs, so the copy you install from here should be able to take its updates from Play
  without uninstalling, if you'd rather update that way.
### Changed
- **The first run on a tablet offers two choices, not three:** Apple Pencil or Mouse or Trackpad on an iPad, Stylus
  or Mouse and Keyboard on Android. Fingers always move the view, so their gestures are in the pencil's list instead
  of a choice of their own. A USB-C or Bluetooth mouse on an iPad works too: click and right-click as on a computer.
- **"Apple Pencil (or Stylus) may not be connected" explains itself.** Tap it for what's needed: a connected stylus
  that the device sees as a pen (Apple Pencil, S Pen, a USI pen), since rubber-tip styluses count as fingers and
  fingers only move the view. **Use a Mouse** switches to the mouse instead. It comes back for a moment when a
  finger taps the model before any pen has, and never shows for mouse or trackpad users.

## 0.7.6-alpha
**Patterns take a typed quantity, value boxes insert and create variables, and Android opens and saves files anywhere.**
### New
- **Type a pattern's quantity.** In Linear and Circular Pattern, Tab moves the value box from the spacing (or angle)
  to Quantity and back, and a click on the Quantity pill opens it. Whole counts from 2 to 500; variables work too.
  Thanks [@leo-smi](https://github.com/leo-smi) (#6).
- **Insert a variable from the value box.** The x+ button at the end of the box lists the project's variables with
  their values; pick one to put its name in. It shows once the project has variables. Follow-up to
  [@kiroandyor-create](https://github.com/kiroandyor-create)'s #5 (#11).
- **Create a missing variable from the box.** Typing a name that isn't a variable yet (`20 + w`) and pressing Return
  offers **Create w…** under the reason: give it a value and a type, and the box applies at once (#12).
- **Android: open, import and export through the system's file picker.** Files, Drive, Dropbox, USB storage and any
  other place Android offers, and ShapeCraft's projects folder shows up in Files and every file picker as
  "ShapeCraft". Follow-up to [@joravarsinghing](https://github.com/joravarsinghing)'s #3 (#7).
- **Android: the first run asks Stylus, Fingers or Mouse and Keyboard**, then shows that one's controls; a mouse
  goes on to how the view moves (#10).
### Changed
- **Android phones keep the view clear.** On screens under 600 dp across, Items starts folded away (its button opens
  it) and the tool columns show icons only; Preferences ▸ Menu Labels brings the words back (#9).
- **The Variables window reads a bare length in the project's unit**, as value boxes do. Saved projects keep their
  meaning: a value saved as a plain number shows with "mm" after it in an inch project (#13).
- **Hints for the pattern tools** say how to set the quantity.
### Fixed
- **A click on the pattern's Quantity pill** put its count into the spacing (#6).
- **The drawing holds still while the pointer is over the value box**, so its buttons can be reached while drawing.

## 0.7.5-alpha
**Value boxes take variables, expressions and units. Phones are landscape only.**
### New
- **Type variables and expressions into any value.** Once a tool's value box is open, letters go into it instead of
  running shortcuts, so `box_width * 2` or `wall + 3 mm` works wherever a number does: tool values, sketch
  dimensions, the step card and History. Use `+ - * / ^` and brackets, and functions such as `sqrt`, `round`,
  `floor`, `ceil`, `abs`, `min`, `max`, `avg`, `mod`, `sin`, `cos`, `tan`, `asin`, `atan2`, `radians` and `pi()`.
  Mix units freely: `5 mm + 2 cm`, `1' 6"`, `1 1/2"`, `2 ft + 3 in`, `30 deg` (`mm`, `cm`, `m`, `in`/`"`,
  `ft`/`'`, `deg` and their long names). A number without a unit is in the project's unit, and a value that uses a
  variable follows it when the variable changes. Thanks [@kiroandyor-create](https://github.com/kiroandyor-create) (#5).
### Changed
- **Phones run ShapeCraft sideways.** The Android app stays in landscape (either way round) on screens under 600 dp
  across, and the tablet page in a phone's browser asks you to turn the phone when it's upright. Tablets and opened
  foldables still turn freely.
- **A value that doesn't work says why.** A yellow warning sign shows beside the value box while what you've typed
  doesn't make a value; Return then keeps the box open with the reason under it, instead of using the dragged value.
### Fixed
- **Typed lengths in inch, foot, centimeter and meter projects.** A plain number typed while drawing or pulling was
  read as millimeters; it now uses the project's unit, as the step card and History already did.

## 0.7.3-alpha
**ShapeCraft runs on Android: a first test build of the app, and the command-line tools with the full modelling kernel.
Touch screens get the Pencil's own gestures.**
### New
- **The Pencil on touch screens.** Hold it still, then drag, for an Area Selection (right for what's fully inside,
  left for what it crosses); a plain drag on nothing turns the view. **Sketch** has Line ready at once with no
  plane to pick: the first stroke draws straight onto the floor grid or a face, and a stroke that bends round
  becomes a circle around where it started. A tap on a tool's value opens a number pad beside it (digits, minus,
  backspace, ✓) when no keyboard is attached. A note under the title says when no pen has been used yet.
- **Items on touch screens.** Finger-sized rows with the eye always shown, the filter as the panel's title, and
  **Rename**, **Delete** and **More…** along the bottom acting on what's selected (More… holds Invert Body
  Visibility, Show Hidden Bodies, New Drawing…, Reveal Selection, Zoom to, Isolate and Show/Hide).
- **The ⋯ menu** on touch screens has **Import ›** (File…, Project…, Image…) and **Export to ›** (Export…, Export to
  Slicer…, Quick Export, Open Screenshot Tool…, Open Visualization…).
- **Touch screens (Android, and the tablet page) work like a CAD app on iPad.** The Pencil picks, draws and drags
  handles; fingers only move the view: one finger turns it, two pan and pinch, a double tap zooms to a face, and a
  three-finger swipe left or right undoes or redoes. Undo and Redo also sit at the bottom of the tool column. There
  is no title bar: the project name floats over the view (a bar across the top when the screen is upright), a tap on
  it renames the project, and its ⋯ menu holds Import, Export, Help and Settings. Settings closes from its corner,
  History starts closed, the Items column starts at its iPad width, labels are a little bigger, and keyboard
  shortcuts stay out of sight until a keyboard is attached. On Android everything is drawn at the size it has on an
  iPad.
- **ShapeCraft for Android (test build).** The whole app, the same interface as on the desktop, as an APK for
  arm64 phones and tablets (and x86_64 for emulators) on Android 8.0 or later. A stylus draws and edits like a
  precise mouse, hovering included; fingers move the view, as on the tablet page. A mouse works with all its buttons
  and the scroll wheel. The interface scales to fit small
  screens (sideways works best on a phone), the on-screen keyboard opens for text fields, and a hardware keyboard
  types numbers into tools. The launcher icon is an adaptive icon: the blue tile fills the device's icon shape.
  Projects live in the app's own storage; exports go to its folder under Android/data, which a computer can read
  over USB. Opening files from elsewhere on the device isn't there yet. Build it with `cargo xtask android-apk`; the Android CI job
  builds it too. See `packaging/android/README.md`.
- **Android build of `shapecraft-cli`** (arm64, plus x86_64 for emulators). It is one self-contained executable,
  with OpenCASCADE built by the Android NDK and linked in. It is aligned for the 16 KB memory pages Android 15+
  devices use, and it runs on Android 8.0 and later. Build it with `cargo xtask android-occt`, then
  `cargo xtask android-cli`. `cargo xtask android-smoke` runs a modelling test on a device or emulator: extrude,
  fillet, boolean, STEP out and back in. The volumes must match the geometry, and they match the desktop exactly.
  A weekly CI job builds it and runs the test on an Android 15 emulator. Thanks
  [@joravarsinghing](https://github.com/joravarsinghing) (#3).
### Changed
- **Hints on a touch screen** say tap, not click, and leave out the keys; Cancel and Done lose their keycaps and grow
  to fingertip size. On the projects screen one tap opens a project, with three cards across an iPad.
### Fixed
- **Renaming with Return** (the project name, or an item in the Items panel) no longer opens Search Commands as well.
- **Preferences on a short screen** scrolls, so every setting and Done can be reached.

## 0.6.9-alpha
**Sketch on the empty background when you look straight at a plane.**
### Fixed
- **Sketching in a side view.** In Front, Back, Left, Right, Top or Bottom (or any view square to an origin plane),
  Sketch or a sketch tool (L, R, C…) now opens the sketch on the plane you're facing straight away, with the grid
  across the view and the tool ready, so clicks on the empty background draw. Before, nothing happened there unless
  you clicked the model. A click on the empty background while a sketch waits for its plane also uses the plane
  facing you when the floor can't be reached.
- **Left and Right views were swapped.** View ▸ Left (⌘7) now looks from the left side and View ▸ Right (⌘6) from
  the right, matching the view cube's labels.

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
