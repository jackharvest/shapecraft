# ShapeCraft Alpha for Linux

A local-first direct-modeling CAD app. This is an early alpha: expect rough edges, and keep copies of files you care
about. It carries its own OpenCASCADE libraries; your desktop only needs Vulkan or OpenGL graphics (Mesa is fine).
It runs on Ubuntu 26.04 / Kubuntu 26.04 or newer (x86_64).

## Install

In a terminal, in the folder this file is in:

```sh
./install.sh
```

That's all: no sudo, nothing else to install. ShapeCraft is now in your application launcher.

To remove it later, run `~/.local/lib/shapecraft/install.sh --uninstall` (or `./install.sh --uninstall` from this
folder). Your projects in `~/Documents/ShapeCraft` stay.

## Use it from an AI assistant (MCP)

Headless, without the app open (models, renders PNGs, saves projects):

```json
{ "mcpServers": { "shapecraft": { "command": "/home/YOU/.local/bin/shapecraft-cli", "args": ["mcp"] } } }
```

Driving the app you see on screen: start it with a control port, then point the MCP server at it.

```sh
shapecraft --control 7979 &
```

```json
{ "mcpServers": { "shapecraft": { "command": "/home/YOU/.local/bin/shapecraft-cli", "args": ["mcp", "--connect", "127.0.0.1:7979"] } } }
```

Clients that add servers from the command line take the same command: `~/.local/bin/shapecraft-cli mcp` (with
`--connect 127.0.0.1:7979` for the running app). The tools are `execute`, `list_commands`, `inspect`, `render`, `screenshot` and the project
files ones; `list_commands` describes every command and its parameters.

## If something goes wrong

- If `shapecraft` isn't found in a terminal, use `~/.local/bin/shapecraft`, or log out and back in.
- Graphics: `shapecraft --graphics opengl` (or `vulkan`, `cpu`) picks a path; Preferences shows the one in use.
- A crash leaves `crash.log` in `~/.config/ShapeCraft/`. Please send it along.
