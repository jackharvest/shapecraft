<p align="center">
  <img src="assets/icon/png/shapecraft-256.png" width="112" height="112" alt="ShapeCraft icon: a blue rounded tile with a white solid">
</p>

<h1 align="center">ShapeCraft</h1>

<p align="center">
  <b>Push, pull, done. Free, open-source direct-modeling CAD for macOS, Windows and Linux, built in Rust.</b><br>
  Sketch on any face, pull it into a solid, round the edges, and type a number whenever you like.
  No account, no cloud, no seats. Your projects are plain folders on your disk.
</p>

<p align="center">
  <a href="https://jackharvest.com/shapecraft">Website</a> ·
  <a href="https://github.com/jackharvest/shapecraft/releases/latest">Download</a> ·
  <a href="https://github.com/jackharvest/shapecraft/issues/new/choose">Report an issue</a> ·
  <a href="https://www.buymeacoffee.com/jackharvest">Prove my wife wrong ☕</a>
</p>

<p align="center">
  <a href="https://github.com/jackharvest/shapecraft/actions/workflows/build.yml"><img src="https://github.com/jackharvest/shapecraft/actions/workflows/build.yml/badge.svg" alt="Build"></a>
  <a href="https://github.com/jackharvest/shapecraft/releases/latest"><img src="https://img.shields.io/github/v/release/jackharvest/shapecraft?include_prereleases&label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="MIT OR Apache-2.0">
</p>

<p align="center"><img src="website/media/handheld.gif" width="720" alt="Speed-lapse: a clamshell handheld console modeled from a blank project, then its lid closes and opens"></p>

## Download

Alpha builds are on the [releases page](https://github.com/jackharvest/shapecraft/releases/latest): macOS (Apple
silicon and Intel), Windows x64 and Linux x86_64. Everything the app needs is inside the download, and from then on
ShapeCraft updates itself: it downloads new versions in the background and installs them when you close it
([how](docs/updates.md)).

The builds aren't signed with a paid developer certificate yet, so your computer asks once:
- **macOS:** open the app, then System Settings ▸ Privacy & Security ▸ **Open Anyway**.
- **Windows:** in SmartScreen choose **More info**, then **Run anyway**.
- **Linux:** allow the AppImage to run as a program (Properties, or `chmod +x`), then double-click it.

## What works

- **Sketching:** lines, arcs, splines, rectangles, circles, polygons and text, with snapping, inference,
  constraints, driving dimensions and linked patterns. Type a size at any time.
- **Modeling:** extrude, revolve, sweep, loft, fillet (radius or chordal, G1 or G2, setback corners, curvature),
  chamfer, shell, offset face, booleans, split, mirror, patterns, move and rotate with linked or unlinked copies,
  wrap and emboss.
- **History:** every step stays editable on its Parameter Card; change a number and the model rebuilds.
- **Files:** projects are folders (`project.json` plus STEP). Open and save STEP, IGES, Parasolid XT, STL, 3MF, OBJ
  and `.shapr`; export glTF, and 2D drawings to PDF, SVG, DXF and DWG.
- **Visualize:** materials you drag onto bodies, HDR panoramas, display modes, section views.
- **Learning:** three in-app tutorials, about seven minutes each.
- **Eleven interface languages**, light and dark themes, Vulkan, Metal, DirectX 12 and OpenGL.

Status and plans are in [docs/roadmap.md](docs/roadmap.md); what changed in each version is in
[CHANGELOG.md](CHANGELOG.md).

## Drive it from scripts and agents

Every menu item, tool and key is a registered command. The same commands are reached by the CLI, a JSON control
channel and a built-in MCP server.

```sh
shapecraft-cli commands                      # list every command and its parameters
shapecraft-cli mcp                           # MCP server on stdio, headless
shapecraft --control 47811                   # start the app with the control channel on
shapecraft-cli mcp --connect 47811           # …and drive the open app, so you can watch
shapecraft-cli run part.jsonl --save out/ --png part.png
```

Details: [docs/mcp.md](docs/mcp.md).

## Build from source

Needs Rust (stable) and Open CASCADE 7.8 or newer, linked dynamically. Set `OCCT_INCLUDE_DIR` (the folder holding
`Standard.hxx`) and `OCCT_LIB_DIR` if it's somewhere the build doesn't look.

- macOS: `brew install opencascade`.
- Linux: your distribution's OCCT development package when it is 7.8+ (Fedora 41+, Arch, Debian 13+), or OCCT from
  conda-forge (`micromamba create -n occt -c conda-forge occt`, then point the two variables at
  `$CONDA_PREFIX/include/opencascade` and `$CONDA_PREFIX/lib`). The window also needs the usual X11/Wayland
  development libraries (xkbcommon, wayland, X11, Xcursor, Xrandr, Xi, GL).
- Windows (MSVC): OCCT from conda-forge (`Library\include\opencascade`, `Library\lib`) or vcpkg; put the OCCT DLLs
  on `PATH` to run.

```sh
git clone https://github.com/jackharvest/shapecraft
cd shapecraft
cargo run --release -p shapecraft
```

On Linux, `sh packaging/linux/install.sh` installs a release build into `~/.local` with a launcher entry.
`packaging/proxmox/shapecraft-lxc.sh` makes a Proxmox VE container for headless modeling over MCP (`--help`).

## Architecture

```text
geom ─ kernel (traits) ─ kernel-occt (the only crate that touches Open CASCADE)
                │
               doc ─ engine (commands, undo, History) ─ render (wgpu)
                                    │
                         ui-egui · mcp ─ apps: shapecraft · shapecraft-cli · shapecraft-snap
```

The geometry kernel sits behind one set of Rust traits, so it can be replaced. Nothing below the UI knows about
egui. See [docs/architecture.md](docs/architecture.md).

## How it's made

ShapeCraft is built by one applications engineer with more than twelve years in the trade, working with AI coding
agents. The agents write much of the code. A person decides what gets built and how it should behave, and checks that
it's right: by hand on real parts, with automated tests, on all three operating systems, and side by side with the
tools it's measured against. It's clean-room: no code, files or artwork from other CAD products.

## Contributing

Bug reports are the most useful help: [open an issue](https://github.com/jackharvest/shapecraft/issues/new/choose)
with the steps and, if you can, the project folder. For code, read [CONTRIBUTING.md](CONTRIBUTING.md) and the
clean-room rules in [AGENTS.md](AGENTS.md) first.

## Disclaimer

The files in this repository are provided as is, without warranty, liability or indemnity. Check anything you'll
manufacture. Read [DISCLAIMER.md](DISCLAIMER.md).

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)). Open CASCADE
Technology is LGPL-2.1 with its exception and is linked dynamically; required notices are in [NOTICE](NOTICE). Every
file asset is listed in [ATTRIBUTION.md](ATTRIBUTION.md).

ShapeCraft is independent and clean-room: behaviour comes from using software as any user would, from public
documentation and from public format specifications. It is not affiliated with, sponsored by or endorsed by
Shapr3D Zrt. or any other CAD vendor. Shapr3D is a trademark of its owner, named only to describe file
compatibility.
