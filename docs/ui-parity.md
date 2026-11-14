# UI parity

> **Last updated:** 2026-11-02 · **Target:** the reference, release 26.200 (macOS) · Feature coverage: `docs/roadmap.md` · Measured: `bench/RESULTS.md`

How close ShapeCraft looks and feels to the reference. Measured by driving both with the same steps and comparing
screenshots side by side (ours rendered offscreen with `shapecraft-snap`, the reference observed black-box).
The golden rule (AGENTS.md) is that this beats feature breadth.

**Headline (revised 2026-11-01): look & flow of the parts that exist ≈ 80–85%; "a user of the reference feels at home over
an hour of modeling" ≈ 65–70%.** The earlier ≈ 90% was self-scored and counted only what had been observed; the
badges crawl (sessions 25–26) found a whole layer of behaviour that the score never saw, several items are "partial"
or built from the manual without being observed, navigation is the weakest area and is felt constantly, and kernel
failures on real parts (OCCT, where the reference has Parasolid) were not scored at all. From now on the number comes
from the task benchmark (`bench/`, see `bench/README.md`), and the table below is only the look-and-feel part.

| Area | Weight | Now | Notes |
|---|---:|---:|---|
| Workspace layout (sidebars, toolbar, right column, title bar) | 15% | 95% | Measured pixel for pixel at 1000–3440 px wide and in full screen: the view under frosted sidebars, toolbar groups and chips, the More overflow on short windows, resizable Items sidebar, popovers, search, context menu, title bar |
| Contextual toolbar sets | 10% | 95% | All sets (nothing, sketch, region, face, edge, body) and flyouts with cascades as observed |
| Sketching flow (plane pick, camera focus, rubber bands, dims, selection after draw) | 15% | 85% | Line, rectangle, circle, polygon, arc, ellipse, spline; Trim (hover shows the piece), Offset Edge (single), Project, Text, Move/Mirror/Pattern of sketch curves; regions split like the reference; inference lines; click-to-edit dimensions; constraints (parallel, perpendicular, tangent, coincident, midpoint, concentric, horizontal/vertical, equal, symmetry, lock) kept by a solver, construction geometry |
| Modeling flow (extrude handle, typed values, push/pull, fillet) | 15% | 85% | Live geometry; extrude with draft (handles styled after the reference), fillet/chamfer, shell, revolve (regions or faces, around lines, edges or axes), Loft, Sweep, Offset Face (curved faces too), Scale gizmo (uniform/non-uniform, movable center), Split Body, Align, move/rotate gizmo, Translate (two points), Rotate Around Axis (handle), mirror, linear (Spacing/Total) and circular pattern, construction planes (incl. through an edge, on a curve) and axes, Offset Edge on body edges, tool-mode toolbar (options, Done, Cancel); booleans merge same-surface faces |
| Viewport (shading, edges, grid, axes, selection colors, view cube) | 15% | 82% | Shaded, Wireframe, X-Ray, Section View, Isolate, Measure; orthographic standard/face-on views, field of view slider; bevelled orientation cube with face names, axes, quarter-turn buttons; see-through selected bodies |
| Navigation (orbit, pan, zoom, trackpad, presets) | 10% | 65% | Blender mouse preset as the owner uses it; trackpad gestures untuned |
| Projects screen | 10% | 85% | Local folders and subfolders, back/forward, right-click rename/duplicate/delete, sort menu |
| Icons | 10% | 78% | Redrawn after the reference's style: solid grey/white block silhouettes for modeling tools, dotted 2D icons for sketch tools, thin constraint icons, Drawings and Visualization buttons; a few (Offset Edge in sketches, Project, Wrap & Emboss) still differ |

## Badges: change a tool's behaviour after acting (2026-11-01)
26.200 puts small white badges on a running tool's guide line; each opens a menu that changes how the result
behaves after the pull (`crates/ui-egui/src/workspace/badges.rs`). Every body tool was driven in the reference to
find them. Done: Extrude Sides + Result (and E again flips Sides with a bubble), Revolve's on-profile handle with
Result, Scale Mode, Fillet Continuity, Chamfer Type, Offset Edge Type (Single/Chain), Linear Pattern Result, Loft and
Sweep Result. Dragging Extrude's arrow off its axis onto a flat face extrudes up to it.
Open: G2 fillets and Two-Distance chamfers (listed, greyed); the sketch dimension's Distance Type badge
(Absolute/Horizontal/Vertical); where Loft and Sweep put their badge (unobserved); Circular Pattern, Rotate Around
Axis, Translate, Split Body and the boolean tool's role badges not yet driven in the reference.

## Light theme and third side-by-side pass (2026-10-30)
- **Preferences ▸ Theme: System / Light / Dark** (also the `ui.theme` control method). The reference was walked
  through the same nine states in both themes (idle, a selected sketch, sketch mode, search, Display Modes,
  context menu, hovers, Line) and every flat pixel paired dark→light; the pairs are the `LIGHT` table in
  `crates/ui-egui/src/theme.rs`. Colour tokens are functions of the theme; icons and the view cube are drawn in
  their dark shades and mapped (`theme::paint_themed`); the view has its own light colours (`SceneColors::light`).
- Our window was captured on the same screen through the same pipeline as the reference: our colours land
  exactly as written, so the differences below were real.
- Fixed: History sidebar 280 wide (was 300) so the right column sits where the reference's does; title bar
  #363535, sidebars #252424 with a #434245 edge and a soft 22 px shadow; active tool cell #575757; window title
  #EBEAEB; Items filter field #302F2F with #A2A1A1 text; disabled labels #737374; icons #E0E0E0.
- Context menu over empty space lists Select All · the view toggles · Show Hidden Bodies · Invert Body
  Visibility · Search Commands, sized to its longest label; Escape closes it. Display Modes popover #252525
  with a grey row highlight; its label (and Screenshot's, History's) is a button like the icon.
- Search opens from its button (the opening click used to close it again); the Sketch button waits for a plane
  with no tool armed and no banner (Line arms once a plane is picked).
- Still different: the grid while waiting for a plane (the reference showed it with an existing sketch on XY;
  an empty project showed none, s11-020); the reference's Display Modes and History icons; the default camera
  after opening a file; the History list for a `.shapr` project.

## Measured side by side on 2026-10-30 (second pass)
- Same project open in both (a `.shapr` file with two extrudes, an import and a hidden body): text across the app is
  #DFDFDF in the reference, never pure white (only text on blue buttons is white) — ours now matches; Items icons
  are a hairline cube, a triangle with ringed corners and a stacked hexagon (All Items), now drawn at the same pixel
  boxes; body edges are black; the world axes are hidden by bodies (ours used to draw them on top).
- Ten more colours had been read from wide-gamut captures and are now converted (Onto / Replace Face / Split Body
  tints, the error banner, toggles, the History warning badge, the drawing view frame and others).
- Still different: the History lists one Import step for a `.shapr` project where the reference lists its own
  sketches, imports and features; the default camera after opening a file.

- Shading: the light turns with the camera as in the reference (a wall facing the viewer brightens); walls at the
  default view read 57/118 against the reference's 66/114 (our default camera angle differs a little). Tops keep
  the body colour in both (a #DE3737 top reads #DF3E3E in the reference once its screenshot is converted from the
  display's wide-gamut profile to sRGB; the raw capture had suggested muted colours).
- Colours measured from wide-gamut captures were corrected to sRGB: fully defined sketch geometry #2ECD05, sketch
  point dots #B266FF / #7D4CAF, region fill #0034FF, Constraint Settings blue #2E68FF.
- Items list: rows in the reference's order for projects brought over from the reference; the hidden-row eye matches
  (a solid eye, struck through, #3E3D3D).
- Constraint Settings, Import Project (Insert menu) and the device question at first start are all present.

## Visual pass, tutorials and home screen (2026-11-02, 0.3.0)
- The view cube orbits when dragged (it only took clicks); Default View fits the model; hover, specular, icons
  (Display Modes, Screenshot, History, Sketch, Insert, Tools, Visualize) and the History dash matched; view moves 0.33 s
  as measured; grid during plane pick; readable sketch thumbnails.
- Home screen colours and spacing measured in both themes and both window-focus states; Get Started and Tutorials
  pages; What's new cards with our own pictures.
- In-app tutorials built after the reference's (driven end to end in it): three 7-minute lessons of our own.

## Gaps (pick work from the top; refreshed 2026-11-01)
Done and removed from this list: the macOS menu bar (fires since 2026-11-01), the Items filter and ⋯ footer menu, the
History ⋯ step menu, the Help popover. The measured list is `bench/RESULTS.md`; this one is what it can't measure.
1. Navigation: trackpad gestures untuned against the reference; only the Blender mouse preset was observed.
2. Unobserved badges: where Loft and Sweep put theirs, Wrap & Emboss, Fill, Translate. (Split Body's role badges and
   card observed and matched 2026-11-02.)
3. Parameter Cards: Extrude, Fillet and Split Body cards, the ⋯ Show on Tool Start / History Edit menu and dragging
   are built (2026-11-02); the other tools' cards (Chamfer, Shell, Revolve, …) are not. Link badge / Unlinked Copy built
   for Move and Rotate Around Axis. Fillet Overflow (Cliff/Smooth/Notch) and Y-Shaped Blend are listed, not built.
4. G2 fillets and the Curvature profile beyond straight outside edges between flat faces (no corner blends).
   Chordal fillets keep their chord where the angle varies (2026-11-02); setback distance unobserved.
5. Languages: 12 interface languages (Slovak added 2026-11-14) for the toolbar, right column, sidebars and cards (2026-11-02); hint banners,
   menus, the projects screen and Preferences are still English.
6. Section View caps as real geometry. (DWG export built 2026-11-01, checked with the ODA converter, not AutoCAD.)
7. Icons: heavier, partly filled silhouettes closer to the reference's weight (Offset Edge, Project, Wrap & Emboss).
8. Items: nested folders, drag to reorder; pattern groups.
9. A the reference project's History: its steps are listed under the Import step but not rebuilt (only simple extrudes are);
   our own .shapr files come back with their editable steps. Default camera after opening a file unobserved.
