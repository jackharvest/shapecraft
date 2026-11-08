//! ShapeCraft.app and a DMG from a release build: the app, the CLI (MCP server) beside it, and every library they
//! load that macOS doesn't ship (OpenCASCADE and what it brings), so the Mac needs nothing else installed.
//!
//!   cargo build --release -p shapecraft -p shapecraft-cli && cargo xtask bundle-mac
//!   → target/shapecraft-<version>-macos-<arch>.dmg
//!
//! Libraries are found the way the loader would (absolute paths, @rpath through each file's LC_RPATH, @loader_path),
//! with OCCT_LIB_DIR and $CONDA_PREFIX/lib as fallbacks, copied into Contents/Frameworks and pointed at from there.
//! The app is signed ad hoc (unsigned builds: first launch needs Open Anyway in Privacy & Security).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::util::{clear, copy, mb, output, root, run, s, target, version};

const SYSTEM: [&str; 2] = ["/usr/lib/", "/System/"];
const INSIDE: &str = "@executable_path/../Frameworks/";

/// The libraries a Mach-O file loads (not its own id).
fn deps(path: &Path) -> Result<Vec<String>> {
    let names: Vec<String> = output("otool", &["-L", s(path)])?
        .lines()
        .skip(1)
        .filter_map(|l| l.trim().split(" (").next().map(str::to_string))
        .filter(|n| !n.is_empty())
        .collect();
    let own = output("otool", &["-D", s(path)])?.lines().nth(1).map(|l| l.trim().to_string());
    Ok(names.into_iter().filter(|n| own.as_deref() != Some(n.as_str())).collect())
}

/// The LC_RPATH entries of a Mach-O file, as written.
fn rpath_entries(path: &Path) -> Result<Vec<String>> {
    let text = output("otool", &["-l", s(path)])?;
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    Ok(lines
        .windows(3)
        .filter(|w| w[0] == "cmd LC_RPATH")
        .filter_map(|w| w[2].strip_prefix("path ").and_then(|p| p.split(" (").next()).map(str::to_string))
        .collect())
}

fn rpaths(path: &Path) -> Result<Vec<PathBuf>> {
    let dir = path.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    Ok(rpath_entries(path)?
        .into_iter()
        .map(|r| PathBuf::from(r.replace("@loader_path", s(&dir)).replace("@executable_path", s(&dir))))
        .collect())
}

fn fallbacks() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = std::env::var_os("OCCT_LIB_DIR") {
        v.push(PathBuf::from(d));
    }
    if let Some(c) = std::env::var_os("CONDA_PREFIX") {
        v.push(PathBuf::from(c).join("lib"));
    }
    v
}

/// Where the loader would find `name`, asked for by the file at `origin`.
fn resolve(name: &str, origin: &Path) -> Result<PathBuf> {
    let found = |p: PathBuf| p.exists().then(|| p.canonicalize().unwrap_or(p));
    if let Some(rest) = name.strip_prefix("@rpath/") {
        for d in rpaths(origin)?.into_iter().chain(fallbacks()) {
            if let Some(p) = found(d.join(rest)) {
                return Ok(p);
            }
        }
    } else if let Some(rest) = name.strip_prefix("@loader_path/").or_else(|| name.strip_prefix("@executable_path/")) {
        if let Some(p) = origin.parent().and_then(|d| found(d.join(rest))) {
            return Ok(p);
        }
    } else if let Some(p) = found(PathBuf::from(name)) {
        return Ok(p);
    }
    let base = Path::new(name).file_name().unwrap_or_default();
    for d in fallbacks() {
        if let Some(p) = found(d.join(base)) {
            return Ok(p);
        }
    }
    bail!("can't find {name} (needed by {})", origin.display())
}

fn plist(version: &str) -> String {
    let exts = ["shapr", "step", "stp", "iges", "igs", "x_t", "x_b", "stl", "obj"]
        .iter()
        .map(|e| format!("\t\t\t\t<string>{e}</string>\n"))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDisplayName</key>
	<string>ShapeCraft</string>
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeExtensions</key>
			<array>
{exts}			</array>
			<key>CFBundleTypeName</key>
			<string>CAD file</string>
			<key>CFBundleTypeRole</key>
			<string>Viewer</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
		</dict>
	</array>
	<key>CFBundleExecutable</key>
	<string>shapecraft</string>
	<key>CFBundleIconFile</key>
	<string>ShapeCraft</string>
	<key>CFBundleIdentifier</key>
	<string>com.jackharvest.shapecraft</string>
	<key>CFBundleName</key>
	<string>ShapeCraft</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.graphics-design</string>
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#
    )
}

pub fn bundle() -> Result<()> {
    let version = version()?;
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        a => a,
    };
    let release = target().join("release");
    let exes = ["shapecraft", "shapecraft-cli"];
    for e in exes {
        if !release.join(e).exists() {
            bail!("no release build at {} (cargo build --release -p shapecraft -p shapecraft-cli)", release.join(e).display());
        }
    }
    let stage = target().join("macos-bundle");
    clear(&stage)?;
    let app = stage.join("ShapeCraft.app");
    let (macos, frameworks, resources) = (app.join("Contents/MacOS"), app.join("Contents/Frameworks"), app.join("Contents/Resources"));
    for d in [&macos, &frameworks, &resources] {
        std::fs::create_dir_all(d)?;
    }
    for e in exes {
        copy(&release.join(e), &macos.join(e))?;
    }
    copy(&root().join("assets/icon/shapecraft.icns"), &resources.join("ShapeCraft.icns"))?;
    for lic in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"] {
        copy(&root().join(lic), &resources.join(lic))?;
    }
    std::fs::write(app.join("Contents/Info.plist"), plist(&version))?;

    // Every non-system library, transitively, copied flat into Frameworks.
    let mut queue: Vec<(PathBuf, PathBuf)> = exes.iter().map(|e| (macos.join(e), release.join(e))).collect();
    let mut copied: BTreeMap<String, PathBuf> = BTreeMap::new();
    while let Some((inside, origin)) = queue.pop() {
        for name in deps(&inside)? {
            if SYSTEM.iter().any(|p| name.starts_with(p)) {
                continue;
            }
            let base = Path::new(&name).file_name().map(|b| b.to_string_lossy().into_owned()).context("a library without a name")?;
            if !copied.contains_key(&base) {
                let src = resolve(&name, &origin)?;
                let dst = frameworks.join(&base);
                copy(&src, &dst)?;
                set_mode(&dst)?;
                queue.push((dst, src.clone()));
                copied.insert(base.clone(), src);
            }
            run("install_name_tool", &["-change", &name, &format!("{INSIDE}{base}"), s(&inside)])?;
        }
    }
    for base in copied.keys() {
        run("install_name_tool", &["-id", &format!("{INSIDE}{base}"), s(&frameworks.join(base))])?;
    }
    // Old search paths point at the build machine; drop them so nothing outside the app is ever loaded.
    let files: Vec<PathBuf> = exes.iter().map(|e| macos.join(e)).chain(copied.keys().map(|b| frameworks.join(b))).collect();
    for f in &files {
        for rp in rpath_entries(f)? {
            let _ = std::process::Command::new("install_name_tool").args(["-delete_rpath", &rp, s(f)]).output();
        }
    }
    let mut leaks = Vec::new();
    for f in &files {
        for d in deps(f)? {
            if !SYSTEM.iter().any(|p| d.starts_with(p)) && !d.starts_with(INSIDE) {
                leaks.push(format!("{}: {d}", f.display()));
            }
        }
    }
    if !leaks.is_empty() {
        bail!("still pointing outside the app:\n  {}", leaks.join("\n  "));
    }
    run("codesign", &["--force", "--deep", "--sign", "-", s(&app)])?;
    applications_link(&stage)?;
    let dmg = target().join(format!("shapecraft-{version}-macos-{arch}.dmg"));
    let _ = std::fs::remove_file(&dmg);
    run(
        "hdiutil",
        &[
            "create",
            "-quiet",
            "-volname",
            &format!("ShapeCraft {version}"),
            "-srcfolder",
            s(&stage),
            "-fs",
            "HFS+",
            "-format",
            "UDZO",
            "-imagekey",
            "zlib-level=9",
            s(&dmg),
        ],
    )?;
    println!("{}  ({} libraries inside, {})", dmg.display(), copied.len(), mb(&dmg));
    Ok(())
}

#[cfg(unix)]
fn set_mode(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn applications_link(stage: &Path) -> Result<()> {
    std::os::unix::fs::symlink("/Applications", stage.join("Applications")).context("the Applications link")
}

#[cfg(not(unix))]
fn applications_link(_: &Path) -> Result<()> {
    bail!("bundle-mac runs on macOS")
}
