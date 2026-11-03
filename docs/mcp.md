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

Claude Code, in one line (bridged to the open app; drop `--connect …` for headless). The CLI ships inside the
download: `/Applications/ShapeCraft.app/Contents/MacOS/shapecraft-cli` on a Mac, next to `shapecraft.exe` on
Windows, `~/.local/bin/shapecraft-cli` after the Linux install script.

```sh
claude mcp add shapecraft -- shapecraft-cli mcp --connect 127.0.0.1:47811
```

## Tools

| Tool | What it does |
|---|---|
| `execute {command, params}` | Any command (`list_commands` shows them). Never opens a dialog. |
| `list_commands {filter?}` | Ids, labels, shortcuts and parameter docs. |
| `inspect {body?, sketch?, faces?}` | Bodies (volume, area, bounds, planar faces with normals), sketches (entities, regions), selection, history. `body` or `sketch` narrows it to one; `faces: false` drops the face lists (a model with engraved text can have thousands). |
| `render {view?, width?, height?}` | A PNG from a standard view, framed to fit. Bridged: the app window. |
| `screenshot` | Bridged only: the whole window as the user sees it. |
| `new_project`, `open_project {path}`, `save_project {path?}`, `export {path}` | Files: projects are folders (`project.json` + a STEP file); export by extension (.step, .stl). |

Resources: `shapecraft://project` (inspect) and `shapecraft://commands`.

## Conventions

Millimeters, Z up. Sketch coordinates are 2D in the sketch's plane: `xy` → (X, Y), `xz` → (X, Z), `yz` → (Y, Z);
a sketch on a face is centred on that face. Each rectangle or circle is a region numbered by its entity index;
a closed chain of lines is one more region. Negative extrude distances cut.

**Sketch axes.** `sketch` returns the plane it made: `origin`, `x` (where u points), `y` (where v points) and
`normal`, all in world coordinates. A sketch on a face lines up with the standard view that looks at it (a top
face reads like the Top view, u along +X). A sketch from `{origin, normal}` uses this fixed frame unless you pass
`x_dir` (any vector not along the normal; u points that way):

| Normal | u | v |
|---|---|---|
| −Y (front) | +X | +Z |
| +Y (back) | −X | +Z |
| +X (right) | +Y | +Z |
| −X (left) | −Y | +Z |
| +Z (top) | −Y | +X |
| −Z (bottom) | +Y | +X |

**Regions.** Each region in `inspect` has `holes` (how many) and `inside` (the region whose hole it fills, else
null). Letters with counters ("o", "d", "e") come out as the letter with a hole plus the counter, which is
`inside` the letter: extrude the regions whose `inside` is null to get solid lettering. Two nested rectangles make
the ring (`holes: 1`) and the middle (`inside`: the ring).

**Which body a cut hits.** A profile on a body's face joins or cuts that body. From a free plane, `result:
"subtract"` cuts every body the solid runs into, and `union` / `intersect` work on the body it overlaps most. The
result lists the changed bodies in `bodies`.

**Other conventions found the hard way:**
- `text` is placed from the left end of its baseline.
- `ui.camera` yaw 0 looks at the front; negative yaw swings toward +X; ±π is the back.
- `sketch_fillet` with no `entities` and nothing picked rounds every corner in the sketch. Pass the entity ids a
  `rectangle` returned to round just that one.
- Environments and materials show only in Visualize: `ui.toggle {"panel": "visualization"}` before capturing.
- Body names count up and aren't reused, so tool bodies consumed by a boolean leave gaps ("Body 31" with 27
  bodies). Address bodies by `id`, not by name.

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

`shapecraft --control PORT` accepts JSON lines on 127.0.0.1: `engine.execute {command, params, look?}` (a
`sketch` turns the camera to its plane unless `look` is false, here or in its params; `ui.camera` also stops a
move under way), `engine.commands`,
`ui.screenshot {path}`, `ui.state`, `ui.frames` (frame pacing since the last call: frames, fps, p50/p95/p99/worst ms, over_33ms), `ui.setting {key, value}` (any Preferences value by its settings key), `ui.view {view}`, `ui.camera {target?, yaw?, pitch?, distance?, ortho?, fit?}` (puts the camera there at once; radians), `ui.tool {tool}`, `ui.toggle {panel: items|history|grid|drawings|visualization|help|preferences|nav_presets|variables, tab?}` (tab: the Preferences tab, 0 General, 1 ShapeCraft, 2 Plugins, 3 Sync), `ui.drawing {id}`, `ui.history_step {step}`, `ui.capture {path}`, `ui.draw_tool {index: 0 line length, 1 point-to-point, 2 radius, 3 diameter, 4 centerline, 5 center mark, 6 note, 7 point-to-line, 8 line-to-line, 9 line angle, 10 3-point angle, 11 arc angle, 12 2-line centerline, 13 2-point circular centerline, 14 3-point circular centerline, 15 intersection mark}`, `project.new`,
`project.open {path}`, `project.save`, `project.home`, `project.list {folder?}`, `project.new_folder {path?}`,
`project.rename {path, name}`, `project.duplicate {path}`, `project.delete {path}` (moves it to `.Recently Deleted`). Replies are `{"id", "result"}` or `{"id", "error"}`.

## Recording a build

The running app records itself: call `ui.capture {path}` (the 3D view) or `ui.screenshot {path}` (the whole
window) after each step, numbering the files, then join them with ffmpeg
(`ffmpeg -framerate 4 -i step-%03d.png -pix_fmt yuv420p build.mp4`). Set the camera with `ui.camera` and use
`"look": false` on sketches so every frame is framed the same.
