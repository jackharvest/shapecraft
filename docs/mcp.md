# ShapeCraft and AI agents (MCP)

ShapeCraft speaks the [Model Context Protocol](https://modelcontextprotocol.io) over stdio. Everything an agent
does goes through the same commands as the toolbar and shortcuts.

## Start it

```sh
# Headless: an in-process project, no window. `render` returns PNGs.
shapecraft-cli mcp

# Bridged to the running app, so you can watch the agent model:
shapecraft --control 7979 &
shapecraft-cli mcp --connect 127.0.0.1:7979
```

Client configuration (any MCP client):

```json
{ "mcpServers": { "shapecraft": { "command": "shapecraft-cli", "args": ["mcp"] } } }
```

## Tools

| Tool | What it does |
|---|---|
| `execute {command, params}` | Any command (`list_commands` shows them). Never opens a dialog. |
| `list_commands {filter?}` | Ids, labels, shortcuts and parameter docs. |
| `inspect` | Bodies (volume, area, bounds, planar faces with normals), sketches (entities, regions), selection, history. |
| `render {view?, width?, height?}` | A PNG from a standard view, framed to fit. Bridged: the app window. |
| `screenshot` | Bridged only: the whole window as the user sees it. |
| `new_project`, `open_project {path}`, `save_project {path?}`, `export {path}` | Files: projects are folders (`project.json` + a STEP file); export by extension (.step, .stl). |

Resources: `shapecraft://project` (inspect) and `shapecraft://commands`.

## Conventions

Millimeters, Z up. Sketch coordinates are 2D in the sketch's plane: `xy` → (X, Y), `xz` → (X, Z), `yz` → (Y, Z);
a sketch on a face is centred on that face. Each rectangle or circle is a region numbered by its entity index;
a closed chain of lines is one more region. Negative extrude distances cut.

## A session

1. `execute sketch {"plane": "xy"}`
2. `execute rectangle {"center": [0, 0], "size": [40, 20]}`
3. `execute extrude {"sketch": 1, "region": 0, "distance": 5}`
4. `inspect` → find the top face (normal `[0, 0, 1]`)
5. `execute sketch {"body": 2, "face": <top>}` then `circle {"center": [0, 0], "radius": 3}`
6. `execute extrude {"sketch": 3, "region": 0, "distance": -5}` (a through-hole)
7. `render`

`crates/mcp/src/tests.rs` runs this session and checks the volume only through MCP.

## Parametric history

Every step keeps the commands it ran (parameters, selection, active sketch). `edit_step {step, params}` changes a
step's parameters and rebuilds the model by replaying the history from an empty document; if any step fails,
nothing changes. `edit_sketch_step {step}` opens a sketch as it was at its step (later steps are set aside in
`future`); leaving the sketch replays them. `inspect` lists the step labels ("Sketch 01", "Extrusion 01").

## Variables

`set_variable {name, value, rename?}` and `delete_variable {name}` keep named values; any numeric parameter can be
an expression string ("depth / 2 + 3 mm") using them, and steps that do rebuild when a variable changes.

## Control channel

`shapecraft --control PORT` accepts JSON lines on 127.0.0.1: `engine.execute {command, params}`, `engine.commands`,
`ui.screenshot {path}`, `ui.state`, `ui.frames` (frame pacing since the last call: frames, fps, p50/p95/p99/worst ms, over_33ms), `ui.setting {key, value}` (any Preferences value by its settings key), `ui.view {view}`, `ui.camera {target?, yaw?, pitch?, distance?, ortho?, fit?}` (puts the camera there at once; radians), `ui.tool {tool}`, `ui.toggle {panel: items|history|grid|drawings|visualization|help|preferences|nav_presets|variables, tab?}` (tab: the Preferences tab, 0 General, 1 ShapeCraft, 2 Plugins, 3 Sync), `ui.drawing {id}`, `ui.history_step {step}`, `ui.capture {path}`, `ui.draw_tool {index: 0 line length, 1 point-to-point, 2 radius, 3 diameter, 4 centerline, 5 center mark, 6 note, 7 point-to-line, 8 line-to-line, 9 line angle, 10 3-point angle, 11 arc angle, 12 2-line centerline, 13 2-point circular centerline, 14 3-point circular centerline, 15 intersection mark}`, `project.new`,
`project.open {path}`, `project.save`, `project.home`, `project.list {folder?}`, `project.new_folder {path?}`,
`project.rename {path, name}`, `project.duplicate {path}`, `project.delete {path}` (moves it to `.Recently Deleted`). Replies are `{"id", "result"}` or `{"id", "error"}`.
