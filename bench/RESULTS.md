# Task benchmark results

> Written by `shapecraft-snap --bench bench/tasks`. Do not edit by hand.

**Measured overall (usage-weighted): 98.9%** over 166 tasks

| Category | Weight | Tasks | Score |
|---|---:|---:|---:|
| Sketch constraints, dimensions & snapping | 10 | 12 | 100% |
| Construction geometry | 4 | 11 | 100% |
| 2D Drawings | 4 | 10 | 100% |
| History & parametrics | 12 | 30 | 100% |
| Import & export | 6 | 9 | 100% |
| Items & organization | 4 | 6 | 100% |
| Solid modeling tools | 18 | 30 | 100% |
| Projects, preferences, shortcuts & help | 4 | 8 | 97% |
| Selection & inspection | 6 | 7 | 100% |
| Sketch drawing tools | 14 | 23 | 99% |
| Transform, pattern & align | 7 | 11 | 100% |
| Viewport, navigation & display | 8 | 5 | 89% |
| Visualization | 3 | 8 | 100% |

## Hand-scored (not scriptable)

- projects: 78% — icons: redrawn in the reference's style, a few still differ in weight (docs/ui-parity.md)
- sketch: 85% — sketch feel: inference lines, hover labels, rubber bands checked side by side; snapping to distant edges missing
- viewport: 65% — navigation feel: Blender mouse preset observed and matched; trackpad gestures untuned against the reference; SpaceMouse missing
- viewport: 80% — look of shading, edges, selection colours, view cube (side-by-side snapshots, docs/ui-parity.md)

## Tasks

| Task | Category | Score | What failed |
|---|---|---:|---|
| con-01-offset-plane-sketch — Offset plane 15 up, sketch on it, extrude | construction | 100% |  |
| con-02-midplane-mirror — Midplane between two faces | construction | 100% |  |
| con-03-plane-3points — Plane through 3 points, sketch on it | construction | 100% |  |
| con-04-axis-revolve — Axis along an edge; revolve a profile around that edge | construction | 100% |  |
| con-06-plane-resize — Resize a construction plane | construction | 100% |  |
| con-07-plane-through-edge — Plane through an edge at 45°, sketch on it | construction | 100% |  |
| con-08-plane-on-curve — Plane on a curve at its middle, perpendicular to it | construction | 100% |  |
| con-09-axis-2planes-rotate — Axis where XY and XZ meet (the X axis), rotate a body 90° around it | construction | 100% |  |
| con-10-plane-perp-edge — Plane perpendicular to an edge at a point | construction | 100% |  |
| con-11-plane-parallel-face — Plane parallel to the top face through a point 5 above it | construction | 100% |  |
| con-12-axis-perp-face — Axis perpendicular to a face at a point, circular pattern around it | construction | 100% |  |
| cons-01-dimension-width — Dimension a rectangle's bottom side to 30: the region follows | constraints | 100% |  |
| cons-02-horizontal — Horizontal on a slanted line levels it | constraints | 100% |  |
| cons-03-equal-circles — Equal makes the second circle's radius the first's | constraints | 100% |  |
| cons-04-perpendicular — Perpendicular turns the second line square to the first | constraints | 100% |  |
| cons-05-radius — Radius dimension 8 on a circle | constraints | 100% |  |
| cons-06-dim-keeps-rect — Changing one side keeps the rectangle a rectangle (perpendicular kept) | constraints | 100% |  |
| cons-07-angle — Angle 45° between two lines | constraints | 100% |  |
| cons-08-ui-type-dimension — Click a rectangle's dimension in the sketch and type a new value | constraints | 100% |  |
| cons-09-concentric — Concentric moves the second circle onto the first's center | constraints | 100% |  |
| cons-10-symmetry — Symmetry of two points about a line | constraints | 100% |  |
| cons-11-linked-pattern — A sketch pattern stays linked: making the source circle bigger makes the copies bigger | constraints | 100% |  |
| cons-12-fully-defined — R from the origin with both sizes typed: the rectangle is fully defined (green) | constraints | 100% |  |
| draw-01-create-export — Create a drawing and export it as PDF | drawings | 100% |  |
| draw-02-dimension — Put a distance dimension on the front view | drawings | 100% |  |
| draw-03-section-view — A section view from a cut line | drawings | 100% |  |
| draw-04-dwg — Export DWG | drawings | 100% |  |
| draw-05-update — Drawing follows a model edit (Update) | drawings | 100% |  |
| draw-06-projected — A projected view to the right of the front view | drawings | 100% |  |
| draw-07-detail — A detail view of a corner at 2× | drawings | 100% |  |
| draw-08-tolerance — A dimension with a ± tolerance and a prefix | drawings | 100% |  |
| draw-09-view-bodies — A base view showing only some bodies (the view's Bodies… pill) | drawings | 100% |  |
| draw-10-four-views — Four standard views on an A4 sheet, first-angle | drawings | 100% |  |
| hist-01-edit-extrude-keeps-fillet — Fillet the top front edge, then make the box taller: the fillet stays on that edge | history | 100% |  |
| hist-02-edit-under-hole — Hole sketched on the top face; the box gets taller: the hole follows the top | history | 100% |  |
| hist-03-pushpull-after-renumber — Cut a side hole, push the top face up, then edit the box: the push stays on the top | history | 100% |  |
| hist-04-suppress — Suppress a fillet and bring it back | history | 100% |  |
| hist-05-delete-middle — Push the top up twice, delete the first push: the second still pushes the top | history | 100% |  |
| hist-06-variable — Extrude by a variable, then change it | history | 100% |  |
| hist-07-breakpoint — A breakpoint before the fillet rolls it back; removing it restores | history | 100% |  |
| hist-08-undo-redo-chain — Five steps, undo three, redo two | history | 100% |  |
| hist-09-edit-sketch-past — Edit the first sketch after a fillet: widen the rectangle, the fillet survives | history | 100% |  |
| hist-10-pattern-count — Edit a pattern's count from 3 to 5 | history | 100% |  |
| hist-11-edit-breaks-downstream — Make the box too short for its 4 mm fillet: the edit applies, the fillet is marked | history | 100% |  |
| hist-12-long-chain — Eight steps; editing the first replays them all | history | 100% |  |
| hist-13-rename-and-merge — Rename a step, then Merge Modeling Steps | history | 100% |  |
| hist-14-step-card-ui — Open the extrude's step card from History, click Distance, type a new value | history | 100% |  |
| hist-15-edit-fillet-radius-after-shell — Fillet, shell, then change the fillet radius | history | 100% |  |
| hist-16-delete-sketch-used — Delete the step a later feature needs: refused or the later step marked | history | 100% |  |
| hist-17-chamfer-after-edit-width — Chamfer the top right edge, then widen the box via its sketch: chamfer moves with the edge | history | 100% |  |
| hist-18-edit-fillet-radius-after-hole — Fillet the top front edge, cut a hole in the top, then change the fillet to 3 | history | 100% |  |
| hist-19-edit-hole-radius — Edit the hole's sketch (circle 2 → 3) under a later fillet | history | 100% |  |
| hist-20-suppress-hole — Suppress the hole: the fillet after it stays on its edge | history | 100% |  |
| hist-21-two-bodies — Editing one body's extrude leaves the other body's fillet alone | history | 100% |  |
| hist-22-delete-first-hole — Two holes in the top; delete the first: the second stays where it was | history | 100% |  |
| hist-23-undo-edit — Undo an edited step: the model goes back | history | 100% |  |
| hist-24-region-after-sketch-edit — Extrude the ring around a circle, then move the circle: still the ring | history | 100% |  |
| hist-25-reopen-then-edit — Save, reopen, then edit the first extrude: the fillet replays | history | 100% |  |
| hist-26-breakpoint-insert — Breakpoint before the fillet, add a hole there, remove it: the fillet is still on its edge | history | 100% |  |
| hist-27-offset-face-after-widen — Offset the right face out 3, then widen the sketch: the offset stays on the right face | history | 100% |  |
| hist-28-chamfer-then-taller — Chamfer the top front edge by clicking it, then make the box taller | history | 100% |  |
| hist-29-shell-after-push — Shell the top open, then push the box taller: the open face is still the top | history | 100% |  |
| hist-30-variable-chain — Width and depth variables drive two steps; change both | history | 100% |  |
| io-dxf-sketch — Export a sketch as DXF and import it back | io | 100% |  |
| io-iges-roundtrip — Export IGES, import it into a new project: same volume | io | 100% |  |
| io-import-options — Import an STL whose numbers are inches (import options: Unit) | io | 100% |  |
| io-obj-3mf-glb — Export OBJ, 3MF, GLB and USDZ | io | 100% |  |
| io-parasolid — Export Parasolid | io | 100% |  |
| io-shapr-roundtrip — Export .shapr, import it into a new project: same body, and its Fillet comes back as an editable step | io | 100% |  |
| io-step-roundtrip — Export STEP, import it into a new project: same volume | io | 100% |  |
| io-stl-roundtrip — Export STL, import it into a new project: same volume | io | 100% |  |
| io-x_t-import — Import Parasolid text (x_t) | io | 100% |  |
| items-01-rename — Rename a body | items | 100% |  |
| items-02-hide-show — Hide a body, then Show All | items | 100% |  |
| items-03-folder — Put two bodies in a folder | items | 100% |  |
| items-04-delete — Delete a body | items | 100% |  |
| items-05-nested-folder — A folder inside a folder | items | 100% |  |
| items-06-reorder — Drag a body to reorder the Items list | items | 100% |  |
| model-01-box-ui — Rectangle then E: a 20 × 10 × 12 box, all by input | modeling | 100% |  |
| model-02-pushpull-face — Click the top face, type 15 (Offset Face measures the Total thickness, as observed): the box is 15 tall | modeling | 100% |  |
| model-03-cut-hole — Circle on the top face extruded -10 cuts a through hole | modeling | 100% |  |
| model-04-fillet-edge-ui — Click the top front edge, type 2: a 2 mm fillet | modeling | 100% |  |
| model-05-fillet-all — Fillet every edge of a 20 × 10 × 10 box by 1 | modeling | 100% |  |
| model-06-chamfer-ui — Edge, then Chamfer from the toolbar, type 2 | modeling | 100% |  |
| model-07-two-distance-chamfer — Two-distance chamfer 2 and 4 on one edge | modeling | 100% |  |
| model-08-shell — Shell the top face open with 1 mm walls | modeling | 100% |  |
| model-09-revolve — Revolve a 5 × 10 rectangle 5 mm off the axis: a tube | modeling | 100% |  |
| model-10-loft — Loft a 20 square at z=0 to a 10 square at z=10 | modeling | 100% |  |
| model-11-sweep — Sweep a circle along a straight path: a cylinder | modeling | 100% |  |
| model-12-draft — Extrude with a 10° draft | modeling | 100% |  |
| model-13-symmetric — Symmetric extrude 10 puts 10 on each side (observed s16-009) | modeling | 100% |  |
| model-14-through-all — Through All cut from a sketch above the part | modeling | 100% |  |
| model-15-booleans — Union, then Subtract a third box | modeling | 100% |  |
| model-16-intersect — Intersect two overlapping boxes | modeling | 100% |  |
| model-17-offset-face — Offset Face moves a side face out by 3 | modeling | 100% |  |
| model-18-split — Split a box with the YZ plane at x=10: two bodies | modeling | 100% |  |
| model-19-g2-fillet — G2 fillet on a box's top edges | modeling | 100% |  |
| model-20-wrap-emboss — Wrap & Emboss a sketch onto a cylinder | modeling | 100% |  |
| model-22-extrude-to-object — Extrude a region up to a face of another body | modeling | 100% |  |
| model-23-variable-fillet — Variable-radius fillet (2 to 4 along an edge) | modeling | 100% |  |
| model-24-fillet-chain-corner — Fillet three edges meeting at a corner (corner blend) | modeling | 100% |  |
| model-25-fillet-curvature — Fillet the top edges with the card's Curvature -1: a flat bevel | modeling | 100% |  |
| model-25-fillet-setback — Fillet with Setback corners | modeling | 100% |  |
| model-26-offset-face-radius — Offset Face 'Radius' distance type on a cylinder | modeling | 100% |  |
| model-27-split-by-sketch — Split a body with a sketch line | modeling | 100% |  |
| model-28-sweep-twist — Sweep a square along a line with 90° twist | modeling | 100% |  |
| model-29-revolve-ui — Select a region, V, pick the sketch's axis line, type 180 | modeling | 100% |  |
| model-30-chordal-fillet — Chordal fillet (constant width) | modeling | 100% |  |
| proj-01-save-reopen — Save, go home, open again | projects | 100% |  |
| proj-02-rename — Rename the project | projects | 100% |  |
| proj-03-units — Switch units to inches | projects | 100% |  |
| proj-04-search — ⌘F opens command search; typing 'fil' and Return runs Fillet | projects | 100% |  |
| proj-05-theme — Light theme | projects | 100% |  |
| proj-06-language — Switch the language | projects | 100% |  |
| proj-07-recents — Projects screen lists a saved project | projects | 100% |  |
| sel-01-select-all — ⌘A selects every body | selection | 100% |  |
| sel-02-measure-faces — Measure two parallel faces: 20 apart | selection | 100% |  |
| sel-03-measure-volume — Measure a body: its volume | selection | 100% |  |
| sel-04-click-deselect — Click a face, then empty space: nothing stays selected | selection | 100% |  |
| sel-05-selection-filter — Selection filter Bodies only: clicking a face picks its body | selection | 100% |  |
| sel-06-interference — Interference check finds overlapping bodies | selection | 100% |  |
| sel-07-tab-cycle — Tab cycles what's under the cursor (face → edge → body) | selection | 100% |  |
| sketch-01-rect-typed — R, click a corner, type 12 Tab 8 Enter; click the region, E 10 | sketch | 100% |  |
| sketch-02-circle-typed — C, click the center, type the diameter 10; extrude 10 | sketch | 100% |  |
| sketch-03-line-loop — L, click three corners and back to the first: a closed triangle region | sketch | 100% |  |
| sketch-04-drag-rect — R, press-drag-release draws a rectangle | sketch | 100% |  |
| sketch-05-polygon — Hexagon of radius 10, extruded 1 | sketch | 100% |  |
| sketch-06-circle-in-rect — A circle inside a rectangle splits into a ring and a disc | sketch | 100% |  |
| sketch-07-trim — A line across a rectangle, then Trim the piece outside: two regions | sketch | 100% |  |
| sketch-08-sketch-fillet — Round every corner of a 20 × 10 rectangle by 2; extrude 1 | sketch | 100% |  |
| sketch-09-offset-region — Offset a rectangle's outline 2 outwards: a ring region (corner style unobserved: round or sharp both pass) | sketch | 100% |  |
| sketch-10-ellipse — Ellipse 10 × 5, extruded 2 | sketch | 100% |  |
| sketch-11-arc-slot — Two lines and two 3-point arcs make a slot region | sketch | 100% |  |
| sketch-12-text — Text makes regions that extrude | sketch | 100% |  |
| sketch-13-sketch-on-face — Sketch on a box's top face (click it with Sketch armed), circle, cut | sketch | 100% |  |
| sketch-14-spline — A closed fit-point spline makes a region | sketch | 100% |  |
| sketch-15-project-edges — Project a box's top face outline into a sketch above it | sketch | 100% |  |
| sketch-16-circle-two-clicks — C, click the center, click a rim point 5 away: a Ø10 circle | sketch | 100% |  |
| sketch-17-line-typed-length — L, click, type 8 Enter: an 8 mm line | sketch | 100% |  |
| sketch-18-undo-in-sketch — Draw two rectangles, ⌘Z removes the second | sketch | 100% |  |
| sketch-19-change-sketch-plane — Move a sketch to another plane (Change Sketch Plane) | sketch | 100% |  |
| sketch-20-spline-break — Break a spline at a point | sketch | 100% |  |
| sketch-21-text-font — Text in a named font | sketch | 100% |  |
| sketch-22-face-sketch-axes — A sketch on a top face lines up with the Top view (u along +X, v along +Y) | sketch | 100% |  |
| view-01-standard-views — Top view looks straight down | viewport | 100% |  |
| view-02-front — Front view: level camera | viewport | 100% |  |
| view-03-section — Preferences values can be changed (placeholder until Section View has a control) | viewport | 100% |  |
| vis-01-appearance — Aluminum appearance on a body | visualization | 100% |  |
| vis-02-face-color — Colour one face | visualization | 100% |  |
| vis-03-decal — A picture as a decal on the top face | visualization | 100% |  |
| vis-04-roughness — Set a material's roughness to 0.8 | visualization | 100% |  |
| vis-05-panorama — Import a panorama environment | visualization | 100% |  |
| vis-06-environment — Pick another studio environment and turn the light | visualization | 100% |  |
| vis-07-custom-material — A custom material from a picture, on a body | visualization | 100% |  |
| vis-08-depth-of-field — Depth of field on a point of the model | visualization | 100% |  |
| xf-01-translate — Translate a body by (5, 0, 0) | transform | 100% |  |
| xf-02-rotate — Rotate 90° about Z | transform | 100% |  |
| xf-03-mirror-copy — Mirror a copy across x=0 | transform | 100% |  |
| xf-04-linear-pattern — Linear pattern of 4, spacing 30 | transform | 100% |  |
| xf-05-circular-pattern — Circular pattern of 6 around Z | transform | 100% |  |
| xf-06-scale — Scale 2× about the origin | transform | 100% |  |
| xf-07-scale-nonuniform — Scale only along X by 3 | transform | 100% |  |
| xf-08-align-face — Align a box's bottom onto another box's top | transform | 100% |  |
| xf-10-move-copies — Translate with 3 copies | transform | 100% |  |
| xf-11-move-copy-count — Copy count in the Move gizmo (Copy ×3) | transform | 100% |  |
| xf-12-unlinked-copy — Move with Copy and the Link badge off: an Unlinked Copy that keeps its shape | transform | 100% |  |
