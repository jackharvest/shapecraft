#!/usr/bin/env sh
# Makes a self-contained Linux bundle from a release build: the app, the CLI (MCP server), the OpenCASCADE
# libraries they link (so the target needs no OCCT of its own, whatever version its distribution ships), the launcher
# entry and icons, and an installer.
#
#   cargo build --release -p shapecraft -p shapecraft-cli && sh packaging/linux/bundle.sh
#   → target/shapecraft-<version>-linux-<arch>.tar.gz
#
# Needs patchelf (apt install patchelf). The programs run on the glibc they were built against or newer, so build on
# the OLDEST system to support: a bundle made on Ubuntu 26.10 refuses to start on 26.04. The script prints what it needs.
# On the other machine: extract the bundle, cd into its folder, ./install.sh.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
release="$target/release"
command -v patchelf >/dev/null 2>&1 || { echo "bundle.sh needs patchelf (sudo apt install patchelf)" >&2; exit 1; }
[ -x "$release/shapecraft" ] && [ -x "$release/shapecraft-cli" ] || {
  echo "No release build at $release (cargo build --release -p shapecraft -p shapecraft-cli)" >&2
  exit 1
}

version="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
arch="$(uname -m)"
name="shapecraft-${version:-dev}-linux-$arch"
folder="ShapeCraft-Alpha-${version:-dev}"
stage="$target/bundle/$folder"
rm -rf "$target/bundle"
mkdir -p "$stage/bin" "$stage/lib" "$stage/share/icons" "$stage/licenses"

install -m 0755 "$release/shapecraft" "$release/shapecraft-cli" "$stage/bin/"
strip "$stage/bin/shapecraft" "$stage/bin/shapecraft-cli" 2>/dev/null || true

# The OpenCASCADE libraries (libTK*) and what they bring that a desktop install may lack (TBB, FreeImage, …), not the
# system's own C/C++ runtime, graphics or windowing libraries, which must come from the target.
# With BUNDLE_CXX_RUNTIME=1 (release builds against conda's OCCT, which needs a newer C++ runtime than older
# distributions ship) libstdc++ and libgcc_s come along too.
cxx='libgcc_s|libstdc\+\+|'
[ "${BUNDLE_CXX_RUNTIME:-0}" = 1 ] && cxx=''
skip='^(libc|libm|libdl|librt|libpthread|'"$cxx"'ld-linux|libX|libxcb|libwayland|libGL|libEGL|libvulkan|libdrm|libgbm|libudev|libdbus|libfontconfig|libfreetype|libexpat|libz\.|libsystemd|libcap|libgcrypt|liblzma|libzstd|liblz4|libselinux|libpcre|libbsd|libmd|libffi|libxkbcommon)'
for exe in "$stage/bin/shapecraft" "$stage/bin/shapecraft-cli"; do
  ldd "$exe" | awk '/=> \// { print $1, $3 }' | while read -r soname path; do
    if echo "$soname" | grep -Eq "$skip"; then continue; fi
    [ -e "$stage/lib/$soname" ] || cp -L "$path" "$stage/lib/$soname"
  done
done
# The libraries find each other next to themselves; the programs find them in ../lib.
for lib in "$stage"/lib/*.so*; do
  patchelf --set-rpath '$ORIGIN' "$lib"
done
patchelf --set-rpath '$ORIGIN/../lib' "$stage/bin/shapecraft" "$stage/bin/shapecraft-cli"

cp "$here/shapecraft.desktop" "$stage/share/"
cp "$root/assets/icon/shapecraft.svg" "$stage/share/icons/"
for n in 16 32 48 64 128 256 512; do
  cp "$root/assets/icon/png/shapecraft-$n.png" "$stage/share/icons/"
done
cp "$here/install.sh" "$stage/install.sh"
cp "$here/README-bundle.md" "$stage/README.md"
for f in LICENSE LICENSE-MIT LICENSE-APACHE ATTRIBUTION.md; do
  [ -e "$root/$f" ] && cp "$root/$f" "$stage/"
done

# The bundled libraries' own licences (OpenCASCADE is LGPL-2.1 with an exception), from the packages they came from.
if command -v dpkg-query >/dev/null 2>&1; then
  for exe in "$stage/bin/shapecraft" "$stage/bin/shapecraft-cli"; do
    ldd "$exe" | awk '/=> \// { print $1, $3 }'
  done | sort -u | while read -r soname path; do
    [ -e "$stage/lib/$soname" ] || continue
    real="$(readlink -f "$path")"
    pkg="$(dpkg-query -S "$real" 2>/dev/null | head -n 1 | cut -d: -f1 || true)"
    [ -n "$pkg" ] && [ -e "/usr/share/doc/$pkg/copyright" ] && cp -L "/usr/share/doc/$pkg/copyright" "$stage/licenses/$pkg.copyright"
    [ -n "$pkg" ] && echo "$soname  $pkg  $(dpkg-query -W -f='${Version}' "$pkg")" >> "$stage/licenses/LIBRARIES.txt"
  done
fi

# Every bundled program must start with only what the bundle carries.
missing="$(LD_LIBRARY_PATH= ldd "$stage/bin/shapecraft" "$stage/bin/shapecraft-cli" | grep 'not found' || true)"
[ -z "$missing" ] || { echo "Unresolved libraries in the bundle:" >&2; echo "$missing" >&2; exit 1; }

glibc="$(objdump -T "$stage/bin/shapecraft" "$stage/bin/shapecraft-cli" "$stage"/lib/*.so* 2>/dev/null \
  | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | sort -t. -k2 -n | tail -n 1)"
(cd "$target/bundle" && tar -czf "$target/$name.tar.gz" "$folder")
echo "$stage" > "$target/bundle/STAGE"
echo "$target/$name.tar.gz ($(du -h "$target/$name.tar.gz" | cut -f1)), needs ${glibc:-unknown} or newer"
