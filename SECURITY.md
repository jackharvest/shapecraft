# Security policy

## Reporting a vulnerability

Please report it privately through
[GitHub's private reporting](https://github.com/jackharvest/shapecraft/security/advisories/new), not in a public
issue. Say which version you ran, on which system, and how to reproduce it. A small sample file helps, as long as
it holds nothing private.

Reports are read as they arrive, but there is no fixed response time.

## Supported versions

Only the latest release gets fixes. ShapeCraft is alpha and moves quickly, so please check that a problem still
exists there before reporting it.

## What counts

ShapeCraft treats these as untrusted, and a way past these defences is a vulnerability:

- **Files you open or import.** A project, .shapr, STEP, IGES, Parasolid XT, STL, 3MF, OBJ, image or panorama
  must not run code, write outside the paths you chose, or exhaust memory without limit.
- **The control channel.** It only exists when the app is started with `--control PORT` and listens on loopback
  only.
- **Settings and autosave files.** Another account must not be able to read your projects or redirect a write
  through them.
- **Releases.** A release must not be replaceable by someone other than the project.

## Known limits

- Anything connected through MCP or the control channel can read and write any path your account can.
- The control channel has no access token yet: any local process can use it while it is running. Start it only
  when you need it.
- Release builds are not code-signed yet. Download them only from the project's GitHub releases page, and
  compare the SHA-256 checksums published beside them.
