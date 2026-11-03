#!/usr/bin/env python3
"""Makes ShapeCraft.app and a DMG from a release build: the app, the CLI (MCP server) beside it, and every library
they load that macOS doesn't ship (OpenCASCADE and what it brings), so the Mac needs nothing else installed.

  cargo build --release -p shapecraft -p shapecraft-cli && python3 packaging/macos/bundle.py
  → target/shapecraft-<version>-macos-<arch>.dmg

Libraries are found the way the loader would (absolute paths, @rpath through each file's LC_RPATH, @loader_path),
with OCCT_LIB_DIR and $CONDA_PREFIX/lib as fallbacks, copied into Contents/Frameworks and pointed at from there.
The app is signed ad hoc (unsigned builds: first launch needs Open Anyway in Privacy & Security).
"""
import os
import platform
import plistlib
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
RELEASE = TARGET / "release"
SYSTEM = ("/usr/lib/", "/System/")


def run(*args, capture=False):
    r = subprocess.run(args, check=True, text=True, capture_output=capture)
    return r.stdout if capture else None


def deps(path):
    """The libraries a Mach-O file loads (not its own id)."""
    out = run("otool", "-L", str(path), capture=True).splitlines()[1:]
    names = [line.strip().split(" (")[0] for line in out if line.strip()]
    own = run("otool", "-D", str(path), capture=True).splitlines()[1:]
    return [n for n in names if not own or n != own[0].strip()]


def rpaths(path):
    out = run("otool", "-l", str(path), capture=True)
    return [Path(m.replace("@loader_path", str(path.parent)).replace("@executable_path", str(path.parent)))
            for m in re.findall(r"cmd LC_RPATH\n\s+cmdsize \d+\n\s+path (\S+)", out)]


def resolve(name, origin):
    fallbacks = [Path(p) for p in (os.environ.get("OCCT_LIB_DIR"), os.environ.get("CONDA_PREFIX") and f"{os.environ['CONDA_PREFIX']}/lib") if p]
    if name.startswith("@rpath/"):
        for d in rpaths(origin) + fallbacks:
            if (d / name[7:]).exists():
                return (d / name[7:]).resolve()
    elif name.startswith("@loader_path/") or name.startswith("@executable_path/"):
        p = origin.parent / name.split("/", 1)[1]
        if p.exists():
            return p.resolve()
    elif Path(name).exists():
        return Path(name).resolve()
    for d in fallbacks:
        if (d / Path(name).name).exists():
            return (d / Path(name).name).resolve()
    sys.exit(f"can't find {name} (needed by {origin})")


def main():
    version = re.search(r'^version\s*=\s*"([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
    arch = {"arm64": "arm64", "x86_64": "x86_64"}.get(platform.machine(), platform.machine())
    exes = [RELEASE / "shapecraft", RELEASE / "shapecraft-cli"]
    for e in exes:
        if not e.exists():
            sys.exit(f"no release build at {e} (cargo build --release -p shapecraft -p shapecraft-cli)")

    stage = TARGET / "macos-bundle"
    shutil.rmtree(stage, ignore_errors=True)
    app = stage / "ShapeCraft.app"
    macos, frameworks, resources = app / "Contents/MacOS", app / "Contents/Frameworks", app / "Contents/Resources"
    for d in (macos, frameworks, resources):
        d.mkdir(parents=True)
    for e in exes:
        shutil.copy2(e, macos / e.name)
    shutil.copy2(ROOT / "assets/icon/shapecraft.icns", resources / "ShapeCraft.icns")
    for lic in ("LICENSE-MIT", "LICENSE-APACHE", "NOTICE"):
        shutil.copy2(ROOT / lic, resources / lic)

    plist = {
        "CFBundleName": "ShapeCraft", "CFBundleDisplayName": "ShapeCraft", "CFBundleExecutable": "shapecraft",
        "CFBundleIdentifier": "com.jackharvest.shapecraft", "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version, "CFBundleVersion": version, "CFBundleIconFile": "ShapeCraft",
        "LSMinimumSystemVersion": "13.0", "NSHighResolutionCapable": True, "LSApplicationCategoryType": "public.app-category.graphics-design",
        "CFBundleDocumentTypes": [{"CFBundleTypeName": "CAD file", "CFBundleTypeRole": "Viewer", "LSHandlerRank": "Alternate",
                                   "CFBundleTypeExtensions": ["shapr", "step", "stp", "iges", "igs", "x_t", "x_b", "stl", "obj"]}],
    }
    with open(app / "Contents/Info.plist", "wb") as f:
        plistlib.dump(plist, f)

    # Every non-system library, transitively, copied flat into Frameworks.
    queue = [(macos / e.name, e) for e in exes]  # (file in the bundle, where its own @rpath/@loader_path resolve)
    copied = {}
    while queue:
        inside, origin = queue.pop()
        for name in deps(inside):
            if name.startswith(SYSTEM):
                continue
            base = Path(name).name
            if base not in copied:
                src = resolve(name, origin)
                dst = frameworks / base
                shutil.copy2(src, dst)
                os.chmod(dst, 0o755)
                copied[base] = src
                queue.append((dst, src))
            run("install_name_tool", "-change", name, f"@executable_path/../Frameworks/{base}", str(inside))
    for base in copied:
        run("install_name_tool", "-id", f"@executable_path/../Frameworks/{base}", str(frameworks / base))
    # Old search paths point at the build machine; drop them so nothing outside the app is ever loaded.
    for f in [macos / e.name for e in exes] + [frameworks / b for b in copied]:
        for rp in re.findall(r"cmd LC_RPATH\n\s+cmdsize \d+\n\s+path (\S+)", run("otool", "-l", str(f), capture=True)):
            subprocess.run(["install_name_tool", "-delete_rpath", rp, str(f)], capture_output=True)

    leaks = [f"{f.name}: {d}" for f in [macos / e.name for e in exes] + [frameworks / b for b in copied]
             for d in deps(f) if not d.startswith(SYSTEM) and not d.startswith("@executable_path/../Frameworks/")]
    if leaks:
        sys.exit("still pointing outside the app:\n  " + "\n  ".join(leaks))

    run("codesign", "--force", "--deep", "--sign", "-", str(app))
    os.symlink("/Applications", stage / "Applications")
    dmg = TARGET / f"shapecraft-{version}-macos-{arch}.dmg"
    dmg.unlink(missing_ok=True)
    run("hdiutil", "create", "-quiet", "-volname", f"ShapeCraft {version}", "-srcfolder", str(stage), "-fs", "HFS+",
        "-format", "UDZO", "-imagekey", "zlib-level=9", str(dmg))
    print(f"{dmg}  ({len(copied)} libraries inside, {dmg.stat().st_size / 1e6:.0f} MB)")


if __name__ == "__main__":
    main()
