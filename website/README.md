# ShapeCraft website

A static page for `jackharvest.com/shapecraft`: `index.html` plus `media/`. It uses no build step and no
server code. Google Fonts (Inter, JetBrains Mono) is its only outside request.

## Layout idea
The page is laid out like the app's own workspace, with solid full-width bands in the middle and small patches of
the modeling grid showing through:
- the **title strip** across the top (icon, name, version pill), the **Items** column fixed to the left edge and the
  **History** column fixed to the right;
- a **view cube** at the top right, with faces, bevelled edges and corners. Dragging it turns the whole middle of the
  page like the front of a box (the other sides are panels of grid); letting go springs back. Clicking a face, edge
  or corner turns to it for a second;
- the **hint banner** under it, whose instruction changes with the section on screen, as the app's does with the tool;
- a **History panel** on the right that lists the page's sections as modeling steps (Sketch 01, Extrude 01, Fillet 01…)
  and highlights the one you're reading, with a progress bar;
- a **speed-lapse** at the top: a clamshell handheld modeled from a blank project in about thirty steps, every one a
  real command (`handheld_steps` in `xtask/src/site.rs`), with the camera orbiting through `ui.camera`;
- each feature shown as a recording of the real app next to a **Parameter Card** with the tool's settings;
- platforms still in progress shown as greyed, dashed cards, the way the app greys the rows it doesn't do yet.

## Media
Every GIF and still is our own app, recorded offscreen by `shapecraft-snap` (no window, no screen capture):

```sh
cargo build --release -p shapecraft-snap
cargo xtask site-media /tmp/sc-gifs             # writes website/media/*.gif, *.jpg, handheld.mp4 and rotate.mp4
```

`xtask/src/site.rs` holds the scenes (the clicks, drags and typed values per frame; pointer positions are model points in mm) and the speed-lapse model; `cargo xtask site-scenes DIR` writes them out to look at. Change a scene, run it again.

To look at the page without opening a window: `cargo xtask site-shot file://$PWD/website/index.html OUT 1600 1000 steps.json`, where the steps are
`{"scroll": y}`, `{"wait": ms}`, `{"drag": [[x0,y0],[x1,y1]], "hold": true}`, `{"up": [x,y]}`, `{"click": [x,y]}`, `{"eval": "js"}`, `{"shot": "name"}` and `{"fullshot": "name", "scale": 0.5}`
(set `THEME=dark` for the dark theme). It uses the headless Chromium that Playwright caches.
Never put screenshots or recordings of the reference app here.

## Deploying to IIS (jackharvest.com/shapecraft)
`sh website/deploy.sh` copies `media/`, `web.config` and `index.html` over ssh (key `~/.ssh/id_ed25519_web`).
`WEB_HOST` and `WEB_DIR` override the server and folder. `web.config` makes `index.html` the default document and
caches media for a week (the page itself for 5 minutes).

One-time server setup (Windows Server 2025, elevated PowerShell): a non-admin account that can write only the
site folder, so the key can't touch anything else.

```powershell
# 0. OpenSSH Server ships with Server 2025: start it, and let it through on the Domain profile too.
Start-Service sshd; Set-Service sshd -StartupType Automatic
Set-NetFirewallRule -Name OpenSSH-Server-In-TCP -Profile Domain,Private -RemoteAddress LocalSubnet
# 1. The deploy account (a long random password; it is never used, keys only).
New-LocalUser deploy -Password (Read-Host -AsSecureString "password") -PasswordNeverExpires -AccountNeverExpires
# 2. The ShapeCraft folder, writable by deploy and nothing else of the site.
$site = "C:\inetpub\wwwroot\jackharvest.com\shapecraft"
New-Item -ItemType Directory -Force $site | Out-Null
icacls $site /grant "deploy:(OI)(CI)M"
# 3. deploy's key in a fixed place (the C:\Users\deploy profile doesn't exist until a first logon).
$keys = "C:\ProgramData\ssh\deploy_authorized_keys"
Set-Content $keys "ssh-ed25519 AAAA… shapecraft-site-deploy"   # ~/.ssh/id_ed25519_web.pub from the Mac
icacls $keys /inheritance:r /grant "SYSTEM:F" /grant "Administrators:F"
# 4. sshd: deploy logs in with that key only, no tunnels (a Match block goes at the end of the file).
Add-Content C:\ProgramData\ssh\sshd_config "`nMatch User deploy`n    AuthorizedKeysFile C:/ProgramData/ssh/deploy_authorized_keys`n    PasswordAuthentication no`n    AllowTcpForwarding no`n    X11Forwarding no"
Restart-Service sshd
```

A domain-joined server's network is on the Domain firewall profile: the rule OpenSSH installs covers Private only.
Test from the Mac with `ssh -i ~/.ssh/id_ed25519_web deploy@SERVER whoami`. The server name goes in `website/deploy.env`
(`WEB_HOST=deploy@SERVER`), which git ignores.

Links point to `github.com/jackharvest/shapecraft`. Change them in one pass if the repo ends up somewhere else.

## Link preview card
`media/og-card.jpg` (1200×630) is what iMessage, Discord, Teams, Slack and the rest show when the link is pasted.
It is `gen/og-card.html` shot at 2× and scaled down. The model in it is the speed-lapse handheld rendered see-through:

```sh
cargo xtask site-scenes /tmp/scn             # /tmp/scn/handheld-model.jsonl is the model on its own
shapecraft-cli run /tmp/scn/handheld-model.jsonl --png /tmp/hh.png --size 1400x1200 --clear
# trim to the model and save as website/gen/og-handheld.png, then:
chrome-headless-shell --headless --hide-scrollbars --force-device-scale-factor=2 --window-size=1200,630 \
  --virtual-time-budget=4000 --screenshot=/tmp/card2x.png file://$PWD/website/gen/og-card.html
# scale /tmp/card2x.png to 1200×630, JPEG quality 90 → website/media/og-card.jpg
```
Previews are cached by each app; after changing the card, rename the file (or add `?v=2` to the og:image URL).
