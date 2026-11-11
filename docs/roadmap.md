# Roadmap to parity with the reference

> **Written:** 2026-10-28 · **Rescored:** 2026-10-30 · **Target:** the reference, release 26.200 (macOS) · Companion to `docs/ui-parity.md` (look & feel).

`ui-parity.md` scores how closely the parts we have *look and feel* like the reference. This file scores **feature
coverage**: of what the reference offers (from its public changelog 2019–2026 and manual, paraphrased), how much do we
do, and how well. Cloud, team, account, iPad-pen and Enterprise-only features are out of scope and not counted
(sharing, Published Versions, Spaces, sync, Generative Render, AR, Vision Pro, JT/CATIA/NX import).

Scores are self-assessed: 100% = every in-scope feature of the category exists and behaves like the reference.

| # | Category | Score | Biggest gaps |
|---|---|---:|---|
| 1 | Sketch drawing tools | 90% | font picker for Text, spline break/join badge, change sketch plane |
| 2 | Sketch constraints, dimensions & snapping | 85% *(was 73%)* | done: fully defined elements turn green, Lock on several elements, Unlock, angle dimensions; since: sketch POINT picking (a click on a key point picks it, ⇧ adds), point-to-point (aligned) and point-to-line (perpendicular) distances shown live and kept as driving dimensions (click the value, type), arcs count as defined once their ends and radius are, snapping to where curves cross, mirrored copies stay linked (Symmetric about the mirror line, Equal radii), offset copies stay linked (parallel at the offset distance, concentric). Left: linked patterns, distant-edge snapping |
| 3 | Solid modeling tools | 85% *(was 76%)* | done: the Extrude card (Sides, Extent, Start, Result); since: Extrude To Object and Start From Plane (pick the face or plane from the card), Result New / Union / Subtract / Intersect for Revolve, Loft and Sweep, Fill (Beta) (a smooth face through a loop of edges, sewn on; an open shell closes into a solid), 2-Distance Chamfer (command) and Ruled Loft (the Loft tool's ≡). Left: Wrap & Emboss, chordal fillet + fillet settings, Sweep twist/scale/orientation, Offset Face Radius type, Split by edges/images, cards for the new Loft/Chamfer options |
| 4 | Transform, pattern & align | 85% *(was 80%)* | done since: Align with straight edges (an edge turns parallel to the target edge and their middles meet; pick an edge first in the Align tool) and points (command), Circular Pattern Uniform orientation (≡ menu: copies keep facing the same way), pattern counts typed as expressions of variables, Move/Translate several copies at once ("copies", command). Left: a copy count in the Move gizmo itself |
| 5 | Construction geometry | 86% *(was 75%)* | done since: Plane Perpendicular to Edge at Point and Plane Parallel to Face at Point (pick, then click the point), Axis Through 2 Planes (faces or planes, where they meet), plane display size (plane_size). Left: dragging a plane's corner to resize it, construction points |
| 6 | Selection & inspection | 85% *(was 76%)* | done: Section View edit mode (arrow, flip, Delete Section, Section Only, 2D Section View); since: Section View's two rotate handles (drag to tilt the cut), Measure's Maximum Distance and the Angle between two flat faces or straight edges, every Items row (sketch, plane, axis, image too) lights its item up when hovered, X-Ray opacity (Display Modes ▸ X-Ray ▸ Opacity). Left: selection filters (Bodies/Faces/Edges Only, Tab cycles), interference check |
| 7 | History & parametrics | 85% *(was 76%)* | done: the step ⋯ menu (Insert Breakpoint, Suppress/Unsuppress, Rename, Delete), breakpoint row, failing steps marked ⚠, the 26.200 step card; since: Duplicate and Zoom To in the ⋯ menu, the card's Profile Edit... (pick the sketch's regions again in the view, Done rebuilds), variable types (Length / Angle / Number) in the Variables window, more expression functions (min, max, pow, mod, atan2, asin/acos/atan, exp, ln, log, sign, trunc, cbrt, e) and units (µm as um, yd, rad), unit-checked maths (a length plus an angle is refused; a plain number next to a length takes the project's unit), feet-and-inch forms (1' 6", 1 1/2"), avg/pi()/radians and long unit and function names, expressions typed straight into every value box. Left: Parameter Cards, an insert-variable button beside the value box |
| 8 | Items & organization | 86% *(was 80%)* | done since: the Items row menu has Zoom to, Isolate and Invert Visibility for bodies and Opacity for reference pictures; Reveal in Items Sidebar opens the folder holding the item; imported STEP assemblies arrive as folders (and folders export as assemblies). Left: nested folders, drag to reorder rows |
| 9 | Viewport, navigation & display | 85% *(was 78%)* | done since: the View menu as the reference has it (Nearest Ortho View, Rotate View by 15° / 45°, Grid Position XY / YZ / ZX, Show Grid / World Axes / Edges / Hidden Edges / Decals, Go to Modeling, Units, Foot & Inch Format, Degree Format, Lock Grid Size, Collapse All), Architectural feet-and-inches (1' - 4 1/2") and fractional degrees (27°15') that really change how values show, X-Ray opacity. Left: light theme + accent colour, SpaceMouse, trackpad tuning |
| 10 | Visualization | 85% *(was 72%)* | done: decals (library dragged onto a body; scale, rotate, opacity, planar or cylindrical, extend), environment settings with a cast shadow, nine studio environments, depth of field, Visualization's right-click menu; since: custom materials from a picture (Custom ⊕, tiled over the body by box projection, Scale slider, Rename/Delete), Transparent capture (a PNG with a see-through backdrop and half see-through shadows), studio reflections that follow the backdrop and turn with Light Rotation, material search by category and folding categories, Used Materials picking their bodies, the reference's color picker (SV square, hue bar, Hexadecimal). Left: per-material roughness/IOR editing, panorama environments |
| 11 | 2D Drawings | 85% *(was 73%)* | done: Views flyout, title blocks, date formats, dimension card with Prefix / Tolerances / Suffix, Min-Max Distance; since: pictures on the sheet (Image tool: a PNG/JPEG placed, picked, dragged; kept in SVG and PDF exports with their transparency), Custom Base View… (the view the Modeling camera has), dimensions and notes in DXF exports. Left: the base view's Bodies… pill, DWG export, an explicit Update for drawings (ours follow the model live) |
| 12 | Import & export | 86% *(was 75%)* | done: Export dialog, STEP AP242/214/203, IGES, 3MF, GLB, OBJ+MTL, STL, DXF from Sketch; import of IGES, STL/OBJ, DXF/SVG; Import Project; since: STEP keeps part names and colors both ways and its assemblies (Items folders export as assemblies, imported assemblies become folders), USDZ export (passes usdchecker), Slicer hand-off (a 3MF opened in PrusaSlicer, Bambu Studio or OrcaSlicer), Quick Export to Favorite Formats (the last export's formats into its folder), File ▸ Import Material… / Import Image as Decal…. 2026-10-30: the reference projects round-trip with their colours, hidden bodies and moves (written as the reference's own Move/Rotate steps), their simple Extrudes come back as editable Extrude steps whose distance and draft go back into the reference's, and our exports reopen complete before the reference rebuilds them. Since: curved fillets (rolling-ball blends) read exactly; a Fillet or Chamfer on an extruded body (every edge or chosen ones) comes back as our own editable step whose radius or distance goes back into the reference's. Left: an imported project's steps appended one by one (ours is one step), splines out to the reference, Offset Face and longer step chains as editable steps, DWG, import options |
| 13 | Projects, preferences, shortcuts & help | 85% *(was 76%)* | done: project search, Keyboard Shortcuts customisation; since: the first-run question "Which device do you want to use for modeling?" (Mouse and Keyboard / Trackpad and Keyboard picks the navigation preset) and its Input Device row in Preferences, custom keys for tools that aren't registry commands (Line, Extrude, planes… start as their button would), Tessellation Quality takes effect (display meshes coarser at Medium / Low, from the next start). Left: languages, Rendering Quality |

**Weighted overall: ≈ 85.5%** (equal weights; was ≈ 77% on 2026-10-28). Every category is at 85% or more
(2026-10-29): the Phase G gate (bring projects over from the reference, round-trip other formats) is reached.

### Revised 2026-11-01: these scores are too high
The table above is self-assessed and landed at 85–86% in every category on the day the 85% gate was set, which is
the target showing through rather than a measurement. It also counts features at full value when they are partial
or built from the manual unobserved, weighs Visualization as much as sketching, and doesn't score kernel failures
on real parts. Revised estimate, usage-weighted, partial work at partial value: **feature coverage ≈ 70–75%**
(look & flow of existing parts ≈ 80–85%; feels-at-home over a real session ≈ 65–70%).

From now on the scores come from the **task benchmark** (`bench/`): scripted workflows driven through the real UI
offscreen (keys, clicks on model points, typed values) with checks on the resulting model, scored per category with
usage weights (`bench/categories.json`). Run `shapecraft-snap --bench bench/tasks`; results in `bench/RESULTS.md`.
The `.shapr` corpus (projects made by the reference's users) is the second, stricter benchmark once the tasks pass ≥ 85%.

## Phases

### Phase A — Import & export (12 → target 80%) — mostly done 2026-10-28
1. Export **OBJ** (with an .mtl of body colours), **3MF** (per-body objects and colours), **GLB** (binary glTF, one
   node per body, PBR material from the appearance), **IGES**. Pure Rust writers where the format is a mesh;
   IGES through the kernel.
2. Import **IGES**; **STL** and **OBJ** meshes as bodies (triangles sewn into a solid so they work with Booleans,
   like the reference's mesh bodies), with a triangle cap for hostile files.
3. Import **DXF** and **SVG** as a sketch on the top plane (lines, polylines with bulges, arcs, circles, ellipses,
   splines/Béziers); export the active sketch as DXF/SVG.
4. Export dialog like the reference's (format list, items, units, quality) and File ▸ Export to ▸ per-format entries.
5. Import another ShapeCraft project into the open one.

### Phase B — 2D Drawings (11 → target 75%)
Views flyout (base views of any standard/iso direction, projected views snapping to their parent, section views from a
cut line, detail circles with their own scale), drag views, delete views, hidden lines per view, Update drawing,
title-block layouts with editable fields, tolerances and prefix/suffix, images on the sheet, Min-Max distance.

### Phase C — Visualization (10 → target 75%)
Decals (place, scale, rotate, opacity, wrap across faces, cylindrical projection), more material properties
(roughness, transmission/IOR, emissive, scale, rotation), material categories + search, environment controls
(rotation, sun elevation, intensity, backdrop colour, ground), depth of field, Screenshot options (resolution
presets, transparent background, grid/edges toggles), custom materials from an image texture.

### Phase D — History & sketch constraints (7, 2 → target 80%)
Step menu (Breakpoint/Rollback, Suppress, Rename, Duplicate, Zoom To), error badges, Parameter Cards, typed
variables + expression functions with units; sketch definition colouring and DOF, angle / H / V dimensions,
linked offset/pattern/mirror constraints.

### Phase E — Modeling breadth (3, 4, 5 → target 90%)
Wrap & Emboss, Fill (Beta), Extrude extents/starts/symmetric, Boolean badges on every feature, chamfer and fillet
types, Loft guides/continuity, Sweep options, missing construction planes/axes, multi-copy Move.

### Phase F — Polish (6, 8, 9, 13)
Selection filters, Section View gizmo + modes, Measure extras, light theme, key customisation, display options,
SpaceMouse, Linux/Windows menus and packaging.

### Phase G — Bringing projects over from the reference (later, once parity is high)
Open a real `.shapr` file made in the reference and keep everything: bodies, sketches, history, names, folders, colours,
decals, drawings. High value: people could move their projects over without anything getting mangled. Method: make
test projects in the reference, save them as `.shapr`, open them here and compare attribute by attribute; every mismatch is
a lesson for the app too. Then the same round-trip check for other formats. **Gate:** only once overall parity is
high enough that the comparison mostly finds format gaps rather than missing features (≈ 85%+). Clean room: the
format is learned only from files we make ourselves (interoperability), never from inside the app; this needs the
`.shapr` line in AGENTS.md revisited first. Expected hurdle: the geometry may be stored as Parasolid, which OCCT
can't read.

**Status (session 18):** done for solids. `.shapr` = ZIP of a SQLite workspace + JSON; bodies are Parasolid partitions,
history is MessagePack. crates/xt reads/writes Parasolid XT from the published reference; crates/shapr reads projects
(names, folders, live bodies) and writes them (fresh, or the source file's history plus Delete/Import/rename steps,
caches dropped so the reference replays). Verified chain: the reference edits → our edits → the reference (its steps stay parametric and
editable) → ours, geometry equal to the reference's own STEP. Next: sketches, colours/materials, visibility, seams, text XT.

### Phase H — Robustness, reach and release (from research, 2026-10-30)
Ordered by value for effort; S/M/L = small/medium/large.
1. **Name decision (owner).** The reference's maker has trademark filings for its name and a near spelling of it, for
   CAD software, so a name close to it is a risk; App Store copycat rules point the same way. Pick and clear new
   candidates before any public release. (S; to be confirmed against the trademark office's own records.)
2. **Legal hygiene.** LICENSE files for the declared MIT OR Apache-2.0, a trademark notice, and the clean-room protocol
   written down (released builds only, never pre-release programs, nothing taken from inside the app). (S)
3. **Kernel robustness layer.** Booleans run non-destructive and parallel with an automatic fuzzy tolerance scaled to
   the model, inputs and outputs checked, a retry ladder, same-surface faces merged with history kept, and failing
   edges reported for fillets. (M)
4. **`.shapr` safety.** Import first; carry unknown tables through untouched; never save in place over a schema we
   don't know; a fingerprint test project re-saved by each new release of the reference. (M)
5. **GPU fallback and diagnostics.** A backend choice (Metal/Vulkan/DX12/GL), remembering the one that worked, a GL
   fallback, and a copyable adapter report; tested on 7th-gen Intel HD 620/UHD 630 (Windows, Linux) and Wayland. (S–M)
6. **Cloud folders.** Open and save inside the Google Drive / OneDrive / Dropbox / Box desktop sync folders with
   atomic saves and conflict detection — no credentials at all. Provider APIs later, only on demand: Dropbox and
   OneDrive with PKCE (no secret), Google with the non-sensitive `drive.file` scope; Box needs a client secret, so it
   stays sync-folder only. (S, then M each)
7. **Kernel-protocol crate.** One request/response API between UI and kernel (in-process first); it is what remote
   clients, plugins and headless tests share. (M–L)
8. **OCCT 8 upgrade spike.** Hundreds of fixes, crash and hang guards on degenerate input. (M)
9. **Release engineering like the Craft apps.** Universal signed and notarized DMG; MSI for x64, arm64 and x86;
   AppImage, deb, rpm, Flatpak; SHA256SUMS. (M)
10. **Launcher inclusion (owner's call).** Community apps enter the ArtCraft launcher only by its owner's approval
    through an RFC; disclose the C++/LGPL kernel. Needs 1, 2 and 9 first. (S)
11. **Serve mode.** `shapecraft --serve` with a thin WebGL2/WebGPU client for Chromebooks and weak laptops; in-browser
    kernel (occt-wasm) later. (L)
12. **Android spike** (NDK build of the kernel, read-only viewer first); register for developer verification early. (M)
13. **Plugins.** Wasm components on Wasmtime against the kernel protocol, capability-gated (no files or network unless
    granted), fuel/epoch limits; optional Rhai macros. (L)
14. **iPad.** Personal builds only until the name changes; App Review risk is high for a look-alike. (L)
