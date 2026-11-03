# Architecture

ShapeCraft is a Rust workspace of small crates stacked in layers. Each layer only knows the ones below it, so the
geometry kernel can be swapped, the engine runs without a window, and the UI is one client of the engine among
several (CLI, control channel, MCP).

```
            apps/shapecraft      apps/shapecraft-cli      apps/shapecraft-snap
                  │                     │                       │
              ui-egui ──────────────────┼───────────────────────┘
                  │          mcp ───────┤
                  │                     │
               render                engine ──── formats (meshes, DXF/SVG sketches)
                  │                     │    └── shapr (.shapr) ── xt (Parasolid XT)
                  └──────── doc ────────┘
                             │
                          kernel  ◄──  kernel-occt (OpenCASCADE, the only unsafe code)
```

| Crate | What it owns |
|---|---|
| `kernel` | The solid-modeling interface: an opaque `Shape` handle, meshes, planar curves, axes, the neutral B-rep. No kernel types leak out. |
| `kernel-occt` | The OpenCASCADE implementation. A C++ shim (`cpp/`) with every call in `try/catch`; all `unsafe` lives at its FFI boundary. |
| `doc` | The project document: bodies, sketches (entities, regions, constraints), planes, axes, images, visual settings, project folders on disk. Cheap to clone, so undo is a list of snapshots. |
| `engine` | The editing session: document + kernel + undo/redo + selection, and **every command** (`cmd/`). Mesh and sketch file formats live in `formats/`; STEP and IGES go through the kernel. |
| `render` | The wgpu viewport: camera, scene assembly, shaders, picking, headless PNGs. Knows nothing about the UI toolkit. |
| `ui-egui` | The desktop UI: projects screen, workspace (sidebars, toolbars, viewport, tools), theme, icons drawn in code. |
| `mcp` | MCP server over stdio, headless or bridged to the running app. |
| `xt`, `shapr` | Parasolid XT (from the published format reference) and Shapr3D project files (learned from files we made). |

## Everything is a command

Every user-visible action is a `CommandSpec` (`crates/engine/src/cmd/mod.rs`): a stable id, the label and menu
path the user sees, a shortcut, one-line help, parameter docs for agents, and a `run` function taking JSON
parameters. The menus, toolbar, command search, scripts (`shapecraft-cli run`), the control channel and MCP all call
`Session::execute(id, params)`. Commands never open dialogs; a failing command restores the document.

Commands are grouped by area, each module returning its list: `edit`, `sketch`, `model` (a folder by feature
family), `construct`, `file` (a folder: import, export, drawings, `.shapr`, measure), `visual`. The registry
concatenates them in that order.

## The UI

`crates/ui-egui/src`:

- `app.rs` — switches between the projects screen and a workspace, autosave, the control channel, menus.
- `landing.rs` — the projects screen.
- `theme/` — colour tokens (`Tokens::text()`, `theme::c(0xRRGGBB)` for one-offs), sizes, fonts, and the Light
  theme's measured colour pairs. Colours are written as their dark-theme values; the theme maps them.
- `icons.rs` — every icon, drawn with vector paths in code (no image assets).
- `workspace/` — the modeling screen. `mod.rs` holds the `Workspace` state and the per-frame `ui`; its methods
  are grouped by topic beside it (`actions.rs`, `start_tool.rs`, `keys.rs`, `camera.rs`, `commands.rs`,
  `sketch.rs`). Then:
  - `panels/` — title bar, Items sidebar, the left toolbar column, the right column and view cube, popovers,
    popups and the hint banner, the context menu, command search.
  - `viewport/` — the 3D view: GPU frame and captures, what the scene draws, navigation, clicks, gizmos and
    handles, snapping and picking, dimensions and on-screen annotations.
  - `tools/` — the `Tool` enum (one variant per running tool) and its state, how each tool starts from the
    selection (`setup.rs`) and how it commits (`commit.rs`).
  - `toolbar.rs` (button sets), `history.rs` and `step_card.rs` (History), `visual.rs` (Visualization),
    `drawings.rs`/`sheet_views.rs`/`annotate.rs` (Drawings), dialogs (`prefs.rs`, `export.rs`, …).
- `menubar.rs` — the native macOS menu bar, or menus drawn in the window elsewhere (same tree, same shortcuts).

## Extension points

These are the seams a plugin system grows from; each is already a list or a table rather than scattered code.

| To add… | Do this |
|---|---|
| A command | Write a `run` fn in the right `engine/src/cmd/*` module and add its `CommandSpec` to that module's `commands()`. It is then in command search, scripts, the control channel and MCP. |
| A toolbar button | Add a `ToolDef` to the matching set in `ui-egui/src/workspace/toolbar.rs`, acting through an existing command or a `UiAction`. |
| An interactive tool | Add a `Tool` variant and its state in `workspace/tools/mod.rs`, its start in `tools/setup.rs`, handles in `viewport/handles.rs`, and run its command in `tools/commit.rs`. |
| An icon | Add an `Icon` variant and draw it in `icons.rs` (24-unit grid, dark-theme shades; the Light theme maps them). |
| A colour | Add a token in `theme/mod.rs` (dark value) and its measured light pair in `theme/light.rs`. |
| A file format | Add a reader/writer in `engine/src/formats/` and route it in the `file` commands. |
| A geometry kernel | Implement the `kernel` traits in a new crate; nothing above `kernel` changes. |

## Rules that keep it healthy

- No `unwrap`/`expect`/`panic!` outside tests (clippy denies them). Errors are `Result` and `?`.
- `unsafe` only in `kernel-occt`'s FFI layer.
- Numbers from files, MCP or typed input are validated (NaN, ranges, sizes).
- UI changes are checked against the reference side by side; `shapecraft-snap` renders the real UI offscreen, and
  refactors must leave its renders pixel-identical.
- `cargo fmt` (see `rustfmt.toml`) and `cargo clippy` with no warnings before a commit; CI checks both.
