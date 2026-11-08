//! A self-contained Linux bundle from a release build: the app, the CLI (MCP server), the OpenCASCADE libraries they
//! link (so the target needs no OCCT of its own, whatever version its distribution ships), the launcher entry and
//! icons, and an installer; then, from that folder, an AppImage.
//!
//!   cargo build --release -p shapecraft -p shapecraft-cli && cargo xtask bundle-linux
//!   → target/shapecraft-<version>-linux-<arch>.tar.gz
//!   cargo xtask appimage   (APPIMAGETOOL=path, else appimagetool on PATH)
//!   → target/shapecraft-<version>-linux-<arch>.AppImage
//!
//! Needs patchelf. The programs run on the glibc they were built against or newer, so build on the OLDEST system to
//! support. On the other machine: extract the bundle, cd into its folder, ./install.sh.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::util::{clear, copy, has, mb, output, root, run, s, target, version};

/// The system's own C runtime, graphics and windowing libraries: they must come from the target, not the bundle.
/// Name prefixes, exactly as the shell version matched them.
const SYSTEM: [&str; 32] = [
    "libc",
    "libm",
    "libdl",
    "librt",
    "libpthread",
    "ld-linux",
    "libX",
    "libxcb",
    "libwayland",
    "libGL",
    "libEGL",
    "libvulkan",
    "libdrm",
    "libgbm",
    "libudev",
    "libdbus",
    "libfontconfig",
    "libfreetype",
    "libexpat",
    "libz.",
    "libsystemd",
    "libcap",
    "libgcrypt",
    "liblzma",
    "libzstd",
    "liblz4",
    "libselinux",
    "libpcre",
    "libbsd",
    "libmd",
    "libffi",
    "libxkbcommon",
];
/// The C++ runtime: left to the system unless BUNDLE_CXX_RUNTIME=1 (conda's OCCT needs a newer one than older
/// distributions ship).
const CXX: [&str; 2] = ["libgcc_s", "libstdc++"];

/// `ldd`'s (soname, path) pairs for a program.
fn linked(exe: &Path) -> Result<Vec<(String, PathBuf)>> {
    Ok(output("ldd", &[s(exe)])?
        .lines()
        .filter_map(|l| {
            let (name, rest) = l.trim().split_once(" => ")?;
            let path = rest.split(" (").next()?.trim();
            path.starts_with('/').then(|| (name.trim().to_string(), PathBuf::from(path)))
        })
        .collect())
}

pub fn bundle() -> Result<()> {
    if !has("patchelf") {
        bail!("bundle-linux needs patchelf (sudo apt install patchelf)");
    }
    let release = target().join("release");
    let exes = ["shapecraft", "shapecraft-cli"];
    for e in exes {
        if !release.join(e).exists() {
            bail!("no release build at {} (cargo build --release -p shapecraft -p shapecraft-cli)", release.join(e).display());
        }
    }
    let version = version()?;
    let arch = std::env::consts::ARCH;
    let name = format!("shapecraft-{version}-linux-{arch}");
    let folder = format!("ShapeCraft-Alpha-{version}");
    let bundle_dir = target().join("bundle");
    let stage = bundle_dir.join(&folder);
    clear(&bundle_dir)?;
    for d in ["bin", "lib", "share/icons", "licenses"] {
        std::fs::create_dir_all(stage.join(d))?;
    }
    let bins: Vec<PathBuf> = exes.iter().map(|e| stage.join("bin").join(e)).collect();
    for (e, b) in exes.iter().zip(&bins) {
        copy(&release.join(e), b)?;
        set_exec(b)?;
        let _ = std::process::Command::new("strip").arg(b).status();
    }

    // OpenCASCADE (libTK*) and what it brings that a desktop install may lack (TBB, FreeImage, …).
    let cxx = std::env::var("BUNDLE_CXX_RUNTIME").is_ok_and(|v| v == "1");
    let mut libs: BTreeSet<(String, PathBuf)> = BTreeSet::new();
    for b in &bins {
        for (soname, path) in linked(b)? {
            let system = SYSTEM.iter().any(|p| soname.starts_with(p)) || (!cxx && CXX.iter().any(|p| soname.starts_with(p)));
            if !system {
                libs.insert((soname, path));
            }
        }
    }
    for (soname, path) in &libs {
        let to = stage.join("lib").join(soname);
        if !to.exists() {
            // `cp -L`: the file itself, not a link to it.
            copy(&path.canonicalize().unwrap_or_else(|_| path.clone()), &to)?;
        }
    }
    // The libraries find each other next to themselves; the programs find them in ../lib.
    for entry in std::fs::read_dir(stage.join("lib"))? {
        run("patchelf", &["--set-rpath", "$ORIGIN", s(&entry?.path())])?;
    }
    for b in &bins {
        run("patchelf", &["--set-rpath", "$ORIGIN/../lib", s(b)])?;
    }

    let here = root().join("packaging/linux");
    copy(&here.join("shapecraft.desktop"), &stage.join("share/shapecraft.desktop"))?;
    copy(&root().join("assets/icon/shapecraft.svg"), &stage.join("share/icons/shapecraft.svg"))?;
    for n in [16, 32, 48, 64, 128, 256, 512] {
        let f = format!("shapecraft-{n}.png");
        copy(&root().join("assets/icon/png").join(&f), &stage.join("share/icons").join(&f))?;
    }
    copy(&here.join("install.sh"), &stage.join("install.sh"))?;
    set_exec(&stage.join("install.sh"))?;
    copy(&here.join("README-bundle.md"), &stage.join("README.md"))?;
    for f in ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "ATTRIBUTION.md"] {
        if root().join(f).exists() {
            copy(&root().join(f), &stage.join(f))?;
        }
    }

    // The bundled libraries' own licences (OpenCASCADE is LGPL-2.1 with an exception), from their packages.
    if has("dpkg-query") {
        let mut list = String::new();
        for (soname, path) in &libs {
            let real = path.canonicalize().unwrap_or_else(|_| path.clone());
            let Ok(owner) = output("dpkg-query", &["-S", s(&real)]) else { continue };
            let Some(pkg) = owner.lines().next().and_then(|l| l.split(':').next()).map(str::to_string) else { continue };
            let copyright = PathBuf::from(format!("/usr/share/doc/{pkg}/copyright"));
            if copyright.exists() {
                copy(&copyright, &stage.join(format!("licenses/{pkg}.copyright")))?;
                let ver = output("dpkg-query", &["-W", "-f=${Version}", &pkg]).unwrap_or_default();
                list.push_str(&format!("{soname}  {pkg}  {ver}\n"));
            }
        }
        if !list.is_empty() {
            std::fs::write(stage.join("licenses/LIBRARIES.txt"), list)?;
        }
    }

    // Every bundled program must start with only what the bundle carries.
    let missing: Vec<String> = bins
        .iter()
        .map(|b| {
            std::process::Command::new("ldd")
                .arg(b)
                .env("LD_LIBRARY_PATH", "")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        })
        .collect::<std::io::Result<Vec<_>>>()?
        .iter()
        .flat_map(|t| t.lines().filter(|l| l.contains("not found")).map(str::to_string).collect::<Vec<_>>())
        .collect();
    if !missing.is_empty() {
        bail!("unresolved libraries in the bundle:\n{}", missing.join("\n"));
    }
    let glibc = newest_glibc(&stage)?;
    let tarball = target().join(format!("{name}.tar.gz"));
    run("tar", &["-czf", s(&tarball), "-C", s(&bundle_dir), &folder])?;
    std::fs::write(bundle_dir.join("STAGE"), s(&stage))?;
    println!("{} ({}), needs {} or newer", tarball.display(), mb(&tarball), glibc.as_deref().unwrap_or("an unknown glibc"));
    Ok(())
}

/// The newest GLIBC_x.y symbol version any bundled file asks for.
fn newest_glibc(stage: &Path) -> Result<Option<String>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(stage.join("bin"))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.extend(std::fs::read_dir(stage.join("lib"))?.filter_map(|e| e.ok().map(|e| e.path())));
    let mut best: Option<(u32, u32)> = None;
    for f in files {
        let Ok(text) = output("objdump", &["-T", s(&f)]) else { continue };
        for word in text.split_whitespace() {
            let Some(v) = word.strip_prefix("GLIBC_").or_else(|| word.strip_prefix("(GLIBC_")) else { continue };
            let v = v.trim_end_matches(')');
            let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
            if let (Some(Some(a)), Some(Some(b))) = (parts.next(), parts.next())
                && best.is_none_or(|x| (a, b) > x)
            {
                best = Some((a, b));
            }
        }
    }
    Ok(best.map(|(a, b)| format!("GLIBC_{a}.{b}")))
}

pub fn appimage() -> Result<()> {
    let bundle_dir = target().join("bundle");
    let stage = PathBuf::from(std::fs::read_to_string(bundle_dir.join("STAGE")).context("run cargo xtask bundle-linux first")?.trim());
    let tool = std::env::var("APPIMAGETOOL").unwrap_or_else(|_| "appimagetool".into());
    let version = version()?;
    let arch = std::env::consts::ARCH;
    let app = bundle_dir.join("ShapeCraft.AppDir");
    clear(&app)?;
    std::fs::create_dir_all(app.join("usr/share/licenses/shapecraft"))?;
    run("cp", &["-a", s(&stage.join("bin")), s(&stage.join("lib")), s(&app.join("usr"))])?;
    if let Ok(entries) = std::fs::read_dir(stage.join("licenses")) {
        for e in entries.flatten() {
            copy(&e.path(), &app.join("usr/share/licenses/shapecraft").join(e.file_name()))?;
        }
    }
    for f in ["LICENSE-MIT", "LICENSE-APACHE"] {
        if stage.join(f).exists() {
            copy(&stage.join(f), &app.join("usr/share/licenses/shapecraft").join(f))?;
        }
    }
    // The launcher entry and icon sit at the top of an AppDir; Exec is the program inside it.
    let desktop = std::fs::read_to_string(root().join("packaging/linux/shapecraft.desktop"))?;
    let desktop: String = desktop
        .lines()
        .map(|l| {
            if l.starts_with("Exec=") {
                "Exec=shapecraft %F".to_string()
            } else if l.starts_with("Icon=") {
                "Icon=shapecraft".to_string()
            } else if l.starts_with("TryExec=") {
                String::new()
            } else {
                l.to_string()
            }
        })
        .map(|l| l + "\n")
        .collect();
    std::fs::write(app.join("shapecraft.desktop"), desktop)?;
    let icon = root().join("assets/icon/png/shapecraft-256.png");
    copy(&icon, &app.join("shapecraft.png"))?;
    copy(&icon, &app.join(".DirIcon"))?;
    std::fs::write(
        app.join("AppRun"),
        "#!/bin/sh\n# `shapecraft-cli` as the first argument runs the CLI (the MCP server) instead of the app.\n\
         here=\"$(dirname \"$(readlink -f \"$0\")\")\"\n\
         if [ \"${1:-}\" = \"shapecraft-cli\" ]; then shift; exec \"$here/usr/bin/shapecraft-cli\" \"$@\"; fi\n\
         exec \"$here/usr/bin/shapecraft\" \"$@\"\n",
    )?;
    set_exec(&app.join("AppRun"))?;
    let out = target().join(format!("shapecraft-{version}-linux-{arch}.AppImage"));
    let status = std::process::Command::new(&tool)
        .args(["--no-appstream", s(&app), s(&out)])
        .env("ARCH", arch)
        .status()
        .with_context(|| format!("couldn't start {tool}"))?;
    if !status.success() {
        bail!("{tool} failed ({status})");
    }
    println!("{} ({})", out.display(), mb(&out));
    Ok(())
}

#[cfg(unix)]
fn set_exec(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_exec(_: &Path) -> Result<()> {
    Ok(())
}
