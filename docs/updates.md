# Updates

ShapeCraft keeps itself up to date. Nobody has to visit a download page.

## What the user sees

- **Nothing, most of the time.** A few seconds after starting, and once a day while it stays open, ShapeCraft asks
  GitHub whether a newer version exists. A check that fails (offline, rate limited) stays silent and is tried again
  an hour later.
- **A quiet pill in the bottom-right corner** while a new version downloads ("Downloading ShapeCraft 0.4.1 · 42%"),
  then "ShapeCraft 0.4.1 is ready · Restart". Clicking its text opens a card with the release notes, Restart Now,
  Later and Skip This Version. Its × hides it for the session; the update still waits.
- **Closing with an update ready asks once**: Update and Close, Update and Reopen, or Not Now, with a box to always
  update when closing (Preferences ▸ ShapeCraft ▸ Updates ▸ When Closing: Always Update). Work is saved first.
- **Help ▸ Check for Updates…** and **Preferences ▸ ShapeCraft ▸ Updates ▸ Check Now** check at once and say the
  result even when nothing is new. Preferences also turns off automatic checks or background downloads.

## What it does

`crates/update` (`shapecraft-update`, no UI) holds the flow; `crates/ui-egui/src/updates.rs` draws it.

1. **Check:** one request to `api.github.com/repos/jackharvest/shapecraft/releases`. Nothing else is sent: no
   identifiers, no usage data. The newest release with a higher `MAJOR.MINOR.PATCH` is offered (the `-alpha` on tags
   names the stage and isn't compared). Drafts, and any release page or file outside this repository, are ignored.
2. **Download** in the background into `<cache>/ShapeCraft/updates/<version>/`: the one file this installation
   updates from, checked against the release's `SHA256SUMS.txt`. A file that doesn't match is deleted.
3. **Unpack** there: the `.app` out of the disk image, the portable zip or Linux tarball into a folder.
4. **Install when ShapeCraft closes:** a small helper script waits for ShapeCraft to exit, swaps the new version
   in, and starts it again if asked. Its log is `<cache>/ShapeCraft/updates/update.log`.

| Installed as | Downloads | Swap |
|---|---|---|
| macOS `ShapeCraft.app` (anywhere writable, usually Applications) | `…-macos-<arch>.dmg` | old bundle aside, new one in, old removed |
| Windows MSI (`installed-by-msi.txt` beside the program) | `…-windows-x64.msi` | `msiexec /i` with a progress bar only (a major upgrade of the same product) |
| Windows, by the old setup program (up to 0.7.10) | `…-windows-x64.msi` | the MSI into the same folder, then the setup program's uninstaller, Apps & features entry and Start menu folder are removed |
| Windows portable zip | `…-windows-x64-portable.zip` | new files copied over the folder |
| Linux AppImage | `…-linux-x86_64.AppImage` | new file moved over the old one |
| Linux download (`bin/`, `lib/`, unpacked or put in `~/.local/lib/shapecraft` by `install.sh`) | `…-linux-x86_64.tar.gz` | old folder aside, new one in, old removed |

A copy that can't update in place says why and offers the download page instead: one running from its disk image
or from App Translocation (macOS: move it to Applications), or a build installed by a package manager (the .deb and
.rpm: install the next version's package).

Development builds (anything not built by the release workflow, which sets `SHAPECRAFT_RELEASE=1`) never check on
their own. `SHAPECRAFT_UPDATE_CHECK=1` makes one check, `=0` stops a release build from checking.
`SHAPECRAFT_PRETEND_VERSION=0.4.0` makes a copy report that version for one run, to try an update against a
published release.

## Launchers

A launcher that installs and updates ShapeCraft itself (the ArtCraft Launcher, if ShapeCraft joins it as a community
app) sets `SHAPECRAFT_MANAGED_BY=<its name>` when it starts ShapeCraft, or records the installation in its
`installed.json` (`apps.shapecraft.path`). ShapeCraft then still checks, but says to update in that launcher and
never downloads or replaces files. Release files already follow the Craft App Suite's naming
(`shapecraft-<version>-<os>-<arch>[-portable].<ext>` and `SHA256SUMS.txt`), so a launcher can pick them as it picks
the suite's own.

## Control channel (for tests)

`update.state`, `update.check`, `update.download`, `app.close` (as the window's close button), and
`update.answer {choice: update | reopen | not_now | cancel}` for the closing question.
