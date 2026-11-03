#!/usr/bin/env sh
# Turns the folder bundle.sh made into a single AppImage: download, mark executable, double-click.
#
#   sh packaging/linux/bundle.sh && sh packaging/linux/appimage.sh
#   → target/shapecraft-<version>-linux-<arch>.AppImage
#
# Needs appimagetool on PATH (or APPIMAGETOOL=path); release.yml downloads it.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
stage="$(cat "$target/bundle/STAGE")"
tool="${APPIMAGETOOL:-appimagetool}"
version="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
arch="$(uname -m)"

app="$target/bundle/ShapeCraft.AppDir"
rm -rf "$app"
mkdir -p "$app/usr"
cp -a "$stage/bin" "$stage/lib" "$app/usr/"
mkdir -p "$app/usr/share/licenses/shapecraft"
cp -a "$stage/licenses/." "$app/usr/share/licenses/shapecraft/" 2>/dev/null || true
for f in LICENSE-MIT LICENSE-APACHE; do [ -e "$stage/$f" ] && cp "$stage/$f" "$app/usr/share/licenses/shapecraft/"; done

# The launcher entry and icon sit at the top of an AppDir; Exec is the program inside it.
sed -e 's|^Exec=.*|Exec=shapecraft %F|' -e 's|^Icon=.*|Icon=shapecraft|' -e 's|^TryExec=.*||' "$here/shapecraft.desktop" > "$app/shapecraft.desktop"
cp "$root/assets/icon/png/shapecraft-256.png" "$app/shapecraft.png"
cp "$root/assets/icon/png/shapecraft-256.png" "$app/.DirIcon"

cat > "$app/AppRun" <<'RUN'
#!/bin/sh
# `shapecraft-cli` as the first argument runs the CLI (the MCP server) instead of the app.
here="$(dirname "$(readlink -f "$0")")"
if [ "${1:-}" = "shapecraft-cli" ]; then shift; exec "$here/usr/bin/shapecraft-cli" "$@"; fi
exec "$here/usr/bin/shapecraft" "$@"
RUN
chmod +x "$app/AppRun"

out="$target/shapecraft-$version-linux-$arch.AppImage"
ARCH="$arch" "$tool" --no-appstream "$app" "$out"
echo "$out ($(du -h "$out" | cut -f1))"
