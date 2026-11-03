#!/usr/bin/env sh
# Installs a built ShapeCraft for one user (or, as root, for everyone): the app and CLI, its launcher entry and icons.
# The entry's name matches the window's app id (`shapecraft`), so Wayland docks and taskbars show the right icon.
#
#   cargo build --release -p shapecraft -p shapecraft-cli && sh packaging/linux/install.sh
#   sh packaging/linux/install.sh --uninstall
# From an unpacked bundle (bundle.sh): ./install.sh inside its folder — the bundle goes to <prefix>/lib/shapecraft, with
# its OpenCASCADE libraries, and the commands are linked into <prefix>/bin.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
bundle=""
[ -x "$here/bin/shapecraft" ] && bundle="$here"
root="$(cd "$here/../.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}/release"
if [ "$(id -u)" -eq 0 ]; then
  prefix="${PREFIX:-/usr/local}"
else
  prefix="${PREFIX:-$HOME/.local}"
fi
bin="$prefix/bin"
apps="$prefix/share/applications"
icons="$prefix/share/icons/hicolor"

refresh() {
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$apps" || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$icons" || true
  # KDE's application menu reads its own cache.
  command -v kbuildsycoca6 >/dev/null 2>&1 && kbuildsycoca6 >/dev/null 2>&1 || true
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$bin/shapecraft" "$bin/shapecraft-cli" "$apps/shapecraft.desktop" "$icons/scalable/apps/shapecraft.svg"
  rm -rf "$prefix/lib/shapecraft"
  for n in 16 32 48 64 128 256 512; do rm -f "$icons/${n}x${n}/apps/shapecraft.png"; done
  refresh
  echo "ShapeCraft removed from $prefix"
  exit 0
fi

install -d "$bin" "$apps" "$icons/scalable/apps"
if [ -n "$bundle" ]; then
  # The programs find their libraries beside them (../lib), so the bundle moves as a whole.
  dest="$prefix/lib/shapecraft"
  rm -rf "$dest"
  install -d "$dest"
  cp -R "$bundle/bin" "$bundle/lib" "$dest/"
  # A copy of this script stays with the app, so it can be removed after the download folder is gone.
  install -m 0755 "$here/install.sh" "$dest/install.sh"
  # Say so now, in words, rather than let the launcher entry fail silently later.
  if ! why="$("$dest/bin/shapecraft-cli" commands 2>&1 >/dev/null)"; then
    rm -rf "$dest"
    echo "ShapeCraft can't run on this system, so nothing was installed:" >&2
    echo "$why" >&2
    exit 1
  fi
  ln -sf "$dest/bin/shapecraft" "$bin/shapecraft"
  ln -sf "$dest/bin/shapecraft-cli" "$bin/shapecraft-cli"
  desktop="$bundle/share/shapecraft.desktop"
  svg="$bundle/share/icons/shapecraft.svg"
  png() { echo "$bundle/share/icons/shapecraft-$1.png"; }
else
  [ -x "$target/shapecraft" ] || { echo "No release build at $target (cargo build --release -p shapecraft -p shapecraft-cli)" >&2; exit 1; }
  install -m 0755 "$target/shapecraft" "$bin/shapecraft"
  [ -x "$target/shapecraft-cli" ] && install -m 0755 "$target/shapecraft-cli" "$bin/shapecraft-cli"
  desktop="$here/shapecraft.desktop"
  svg="$root/assets/icon/shapecraft.svg"
  png() { echo "$root/assets/icon/png/shapecraft-$1.png"; }
fi
sed "s|^Exec=shapecraft|Exec=$bin/shapecraft|" "$desktop" > "$apps/shapecraft.desktop"
chmod 0644 "$apps/shapecraft.desktop"
install -m 0644 "$svg" "$icons/scalable/apps/shapecraft.svg"
for n in 16 32 48 64 128 256 512; do
  install -d "$icons/${n}x${n}/apps"
  install -m 0644 "$(png "$n")" "$icons/${n}x${n}/apps/shapecraft.png"
done
refresh
echo "ShapeCraft is installed. Find it in your application launcher, or run: $bin/shapecraft"
case ":$PATH:" in *":$bin:"*) ;; *) echo "Add $bin to your PATH to run shapecraft and shapecraft-cli from a terminal." ;; esac
