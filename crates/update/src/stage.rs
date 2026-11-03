//! Getting an update ready while ShapeCraft runs: download the file this installation updates from, check it against
//! the release's `SHA256SUMS.txt`, and unpack it into the update folder. Nothing in the installation changes here;
//! that waits for ShapeCraft to close (`apply`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use crate::install::Install;
use crate::release::Release;
use crate::{SUMS, net};

/// What a finished stage leaves in its folder, so a ready update survives a restart without downloading again.
const READY_FILE: &str = "ready.json";

/// The folder updates are prepared in: `<cache>/ShapeCraft/updates`.
pub fn updates_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("ShapeCraft").join("updates"))
}

/// An update to `release` that is already downloaded, checked and unpacked: the path `apply` puts in place.
pub fn already_staged(dir: &Path, release: &Release) -> Option<PathBuf> {
    let text = std::fs::read_to_string(dir.join(release.version_text()).join(READY_FILE)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    (v.get("tag")?.as_str()? == release.tag).then_some(())?;
    let path = PathBuf::from(v.get("path")?.as_str()?);
    path.exists().then_some(path)
}

/// Downloads, checks and unpacks `release` for `install` into `dir/<version>/`, removing any other version's folder.
pub fn stage(
    install: &Install,
    release: &Release,
    dir: &Path,
    stop: &AtomicBool,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<PathBuf, String> {
    let suffix = install.asset_suffix().ok_or("updates can't install themselves here")?;
    let asset =
        release.asset_ending(&suffix).ok_or_else(|| format!("ShapeCraft {} has no download for this computer", release.version_text()))?;
    let sums = release.asset_named(SUMS).ok_or("the release has no checksums, so its downloads can't be checked")?;
    let sums = net::get_text(&sums.url, false).map_err(|e| format!("couldn't read the release's checksums: {e}"))?;
    let hash = crate::release::parse_sums(&sums)
        .into_iter()
        .find(|(_, n)| *n == asset.name)
        .map(|(h, _)| h)
        .ok_or("the release's checksums don't list this download")?;

    let here = dir.join(release.version_text());
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.path() != here {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let _ = std::fs::remove_file(here.join(READY_FILE));
    std::fs::create_dir_all(&here).map_err(|e| format!("couldn't make the update folder: {e}"))?;
    let file = here.join(&asset.name);
    if net::sha256_file(&file).as_deref() != Some(hash.as_str()) {
        net::download(&asset.url, &file, &hash, stop, progress)?;
    }

    let ready = match install {
        Install::MacApp(_) => mac_app_from_dmg(&file, &here)?,
        Install::WindowsInstalled(_) => file,
        Install::WindowsPortable(_) => unpack(&file, &here, "shapecraft.exe")?,
        Install::AppImage(_) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            }
            file
        }
        Install::LinuxFolder(_) => unpack(&file, &here, "bin/shapecraft")?,
        _ => return Err("updates can't install themselves here".into()),
    };
    let marker = serde_json::json!({ "tag": release.tag, "path": ready, "asset": asset.name });
    std::fs::write(here.join(READY_FILE), marker.to_string()).map_err(|e| format!("couldn't write the update folder: {e}"))?;
    Ok(ready)
}

fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("couldn't {what}: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("couldn't {what}: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

/// Copies `ShapeCraft.app` out of the disk image.
fn mac_app_from_dmg(dmg: &Path, here: &Path) -> Result<PathBuf, String> {
    let mount = here.join("mount");
    let out = here.join("ShapeCraft.app");
    let _ = run(Command::new("hdiutil").args(["detach", "-force"]).arg(&mount), "detach");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    run(
        Command::new("hdiutil").args(["attach", "-nobrowse", "-noautoopen", "-readonly", "-quiet", "-mountpoint"]).arg(&mount).arg(dmg),
        "open the disk image",
    )?;
    let copied = run(Command::new("ditto").arg(mount.join("ShapeCraft.app")).arg(&out), "copy the app out of the disk image");
    let _ = run(Command::new("hdiutil").args(["detach", "-force", "-quiet"]).arg(&mount), "close the disk image");
    let _ = std::fs::remove_dir(&mount);
    copied?;
    // Downloaded by ShapeCraft itself, so nothing marks it as from the internet; make sure of it anyway.
    let _ = Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&out).output();
    if !out.join("Contents/MacOS/shapecraft").exists() {
        return Err("the disk image didn't hold ShapeCraft.app".into());
    }
    Ok(out)
}

/// Unpacks a zip or `.tar.gz` with the system's `tar` (Windows 10 and later have one that reads zips) and returns the
/// folder holding `marker`: the top, or the one folder the archive wraps everything in.
fn unpack(archive: &Path, here: &Path, marker: &str) -> Result<PathBuf, String> {
    let to = here.join("unpacked");
    let _ = std::fs::remove_dir_all(&to);
    std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
    let tar = if cfg!(windows) {
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from("tar"), |r| PathBuf::from(r).join("System32").join("tar.exe"))
    } else {
        PathBuf::from("tar")
    };
    run(Command::new(tar).arg("-xf").arg(archive).arg("-C").arg(&to), "unpack the update")?;
    if to.join(marker).exists() {
        return Ok(to);
    }
    let dirs: Vec<PathBuf> =
        std::fs::read_dir(&to).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    match dirs.as_slice() {
        [one] if one.join(marker).exists() => Ok(one.clone()),
        _ => Err("the update's archive isn't laid out as expected".into()),
    }
}
