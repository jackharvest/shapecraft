//! How this copy of ShapeCraft was installed, which decides the file an update downloads and how it is put in place,
//! and whether something else (a launcher) looks after updating it.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Set by a launcher that installed and updates this copy, to its display name ("ArtCraft Launcher"). ShapeCraft
/// then still checks, but leaves downloading and installing to it. The ArtCraft Launcher's community apps use the
/// same convention (`<APP>_MANAGED_BY`).
pub const MANAGED_ENV: &str = "SHAPECRAFT_MANAGED_BY";
/// The ArtCraft Launcher's name, and the key it would file ShapeCraft under in its `installed.json`.
const LAUNCHER_NAME: &str = "ArtCraft Launcher";
const LAUNCHER_APP_ID: &str = "shapecraft";
/// The longest manager name shown (the variable is untrusted text).
const MAX_MANAGER_NAME: usize = 64;
/// A launcher's record file is a few KB.
const MAX_RECORD_FILE: u64 = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// Built from source: never checks (unless `SHAPECRAFT_UPDATE_CHECK=1`), never installs.
    Dev,
    /// A launcher installs updates (its name).
    Managed(String),
    /// `ShapeCraft.app`, at this path.
    MacApp(PathBuf),
    /// Installed by the Windows setup program, into this folder.
    WindowsInstalled(PathBuf),
    /// The Windows portable zip, unpacked into this folder.
    WindowsPortable(PathBuf),
    /// The AppImage file.
    AppImage(PathBuf),
    /// The Linux download (`bin/`, `lib/`), unpacked or installed by its `install.sh`: this folder.
    LinuxFolder(PathBuf),
    /// A release build that can't update itself where it is, and why (shown to the user).
    Elsewhere(String),
}

impl Install {
    /// Short name for Preferences.
    pub fn label(&self) -> &'static str {
        match self {
            Install::Dev => "development build",
            Install::Managed(_) => "managed by a launcher",
            Install::MacApp(_) => "macOS app",
            Install::WindowsInstalled(_) => "Windows installer",
            Install::WindowsPortable(_) => "portable zip",
            Install::AppImage(_) => "AppImage",
            Install::LinuxFolder(_) => "Linux download",
            Install::Elsewhere(_) => "can't update in place",
        }
    }

    /// Updates download and install themselves.
    pub fn installs_itself(&self) -> bool {
        matches!(
            self,
            Install::MacApp(_)
                | Install::WindowsInstalled(_)
                | Install::WindowsPortable(_)
                | Install::AppImage(_)
                | Install::LinuxFolder(_)
        )
    }

    /// The end of the release file's name this installation updates from (Craft naming:
    /// `shapecraft-<version>-<os>-<arch>[-portable].<ext>`).
    pub fn asset_suffix(&self) -> Option<String> {
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x86_64",
            _ => return None,
        };
        match self {
            Install::MacApp(_) => Some(format!("-macos-{arch}.dmg")),
            Install::WindowsInstalled(_) if arch == "x86_64" => Some("-windows-x64-setup.exe".into()),
            Install::WindowsPortable(_) if arch == "x86_64" => Some("-windows-x64-portable.zip".into()),
            Install::AppImage(_) => Some(format!("-linux-{}.AppImage", std::env::consts::ARCH)),
            Install::LinuxFolder(_) => Some(format!("-linux-{}.tar.gz", std::env::consts::ARCH)),
            _ => None,
        }
    }
}

/// What [`detect`] looks at, so tests can describe any machine.
pub struct Probe<'a> {
    /// `std::env::consts::OS`.
    pub os: &'a str,
    pub release_build: bool,
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// The running executable.
    pub exe: Option<PathBuf>,
    pub exists: &'a dyn Fn(&Path) -> bool,
    pub read: &'a dyn Fn(&Path) -> Option<Vec<u8>>,
}

pub fn detect(p: &Probe) -> Install {
    if !p.release_build {
        return Install::Dev;
    }
    let env = |k: &str| (p.env)(k).filter(|v| !v.trim().is_empty());
    let Some(exe) = p.exe.clone() else { return Install::Elsewhere("ShapeCraft couldn't tell where it is running from.".into()) };
    let found = match p.os {
        "macos" => match app_bundle(&exe) {
            Some(_) if exe.to_string_lossy().contains("/AppTranslocation/") => {
                Install::Elsewhere("Move ShapeCraft to your Applications folder to get updates automatically.".into())
            }
            Some(b) if b.starts_with("/Volumes") => Install::Elsewhere(
                "ShapeCraft is running from its disk image. Drag it to Applications to get updates automatically.".into(),
            ),
            Some(b) => Install::MacApp(b),
            None => Install::Elsewhere("ShapeCraft isn't running from its app bundle.".into()),
        },
        "windows" => match exe.parent() {
            Some(dir) if (p.exists)(&dir.join("unins000.exe")) => Install::WindowsInstalled(dir.to_path_buf()),
            Some(dir) if (p.exists)(&dir.join("shapecraft-cli.exe")) => Install::WindowsPortable(dir.to_path_buf()),
            _ => Install::Elsewhere("This copy of ShapeCraft isn't in its installed or unzipped folder.".into()),
        },
        "linux" => {
            if env("FLATPAK_ID").is_some() || env("SNAP").is_some() {
                return Install::Managed("your software center".into());
            }
            if let Some(image) = env("APPIMAGE") {
                Install::AppImage(PathBuf::from(image))
            } else {
                match exe.parent().filter(|b| b.file_name().is_some_and(|n| n == "bin")).and_then(Path::parent) {
                    Some(root) if (p.exists)(&root.join("lib")) && !root.starts_with("/usr") => Install::LinuxFolder(root.to_path_buf()),
                    _ => Install::Elsewhere("This copy of ShapeCraft was installed by something that updates it.".into()),
                }
            }
        }
        _ => Install::Elsewhere("Updates install themselves on macOS, Windows and Linux.".into()),
    };
    match managed_by(p, &exe, &found) {
        Some(name) => Install::Managed(name),
        None => found,
    }
}

/// The `.app` bundle an executable at `…/X.app/Contents/MacOS/x` belongs to.
fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// What manages this copy: the variable's value, else "ArtCraft Launcher" when its install record names this
/// installation. Read-only: nothing is written or created.
fn managed_by(p: &Probe, exe: &Path, found: &Install) -> Option<String> {
    if let Some(name) = (p.env)(MANAGED_ENV) {
        let name: String = name.chars().filter(|c| !c.is_control()).take(MAX_MANAGER_NAME).collect();
        if !name.trim().is_empty() {
            return Some(name.trim().to_string());
        }
    }
    let mut places = vec![exe.to_path_buf()];
    match found {
        Install::MacApp(b)
        | Install::AppImage(b)
        | Install::LinuxFolder(b)
        | Install::WindowsInstalled(b)
        | Install::WindowsPortable(b) => places.push(b.clone()),
        _ => {}
    }
    // A launcher that unpacks apps under `<its data>/apps/<id>/` keeps its record two levels up.
    let beside_apps = exe.ancestors().find(|a| a.file_name().is_some_and(|n| n == "apps")).and_then(Path::parent);
    let records = beside_apps.map(|d| d.join("installed.json")).into_iter().chain(launcher_record_path(p));
    let ignore_case = matches!(p.os, "windows" | "macos");
    records.filter_map(|path| (p.read)(&path)).any(|bytes| record_matches(&bytes, &places, ignore_case)).then(|| LAUNCHER_NAME.to_string())
}

/// Where the ArtCraft Launcher keeps `installed.json` for this user.
fn launcher_record_path(p: &Probe) -> Option<PathBuf> {
    let var = |k: &str| (p.env)(k).filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    let (base, dir) = match p.os {
        "windows" => (var("LOCALAPPDATA")?, "ArtCraft Launcher"),
        "macos" => (var("HOME")?.join("Library/Application Support"), "ArtCraft Launcher"),
        _ => (var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))?, "artcraft-launcher"),
    };
    Some(base.join(dir).join("installed.json"))
}

/// The record's `apps.shapecraft.path` is one of `places` or contains one.
fn record_matches(bytes: &[u8], places: &[PathBuf], ignore_case: bool) -> bool {
    let Ok(json) = serde_json::from_slice::<Value>(bytes) else { return false };
    let Some(path) = json.get("apps").and_then(|a| a.get(LAUNCHER_APP_ID)).and_then(|r| r.get("path")).and_then(Value::as_str) else {
        return false;
    };
    let absolute = path.starts_with(['/', '\\']) || path.as_bytes().get(1..3).is_some_and(|d| d[0] == b':' && matches!(d[1], b'/' | b'\\'));
    absolute && places.iter().any(|p| same_or_inside(p, Path::new(path), ignore_case))
}

/// `a` is `b` or inside it (separators, a trailing separator and `\\?\` don't count; case only where ignored).
fn same_or_inside(a: &Path, b: &Path, ignore_case: bool) -> bool {
    let norm = |p: &Path| {
        let mut t = p.to_string_lossy().replace('\\', "/");
        if ignore_case {
            t = t.to_lowercase();
        }
        t.strip_prefix("//?/").map_or(t.clone(), str::to_string).trim_end_matches('/').to_string()
    };
    let (a, b) = (norm(a), norm(b));
    // The bare root would hold every install.
    !b.is_empty() && !b.ends_with(':') && (a == b || a.strip_prefix(&b).is_some_and(|rest| rest.starts_with('/')))
}

/// A path without Windows' `\\?\` prefix (canonicalize adds it), which robocopy and some programs don't take.
fn plain(p: PathBuf) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

/// This machine's installation.
pub fn this_install() -> Install {
    use std::io::Read;
    let env = |k: &str| std::env::var(k).ok();
    let exists = |p: &Path| p.exists();
    let read = |p: &Path| {
        let mut bytes = Vec::new();
        std::fs::File::open(p).ok()?.take(MAX_RECORD_FILE).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    };
    detect(&Probe {
        os: std::env::consts::OS,
        release_build: crate::release_build(),
        env: &env,
        exe: std::env::current_exe().ok().map(|e| plain(e.canonicalize().unwrap_or(e))),
        exists: &exists,
        read: &read,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(os: &str, exe: &str, vars: &[(&str, &str)], files: &[&str], record: Option<&str>) -> Install {
        let vars: Vec<(String, String)> = vars.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect();
        let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        let env = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let exists = |p: &Path| files.iter().any(|f| f == p);
        let read = |p: &Path| record.filter(|_| p.ends_with("installed.json")).map(|r| r.as_bytes().to_vec());
        detect(&Probe { os, release_build: true, env: &env, exe: Some(PathBuf::from(exe)), exists: &exists, read: &read })
    }

    #[test]
    fn each_kind_of_installation() {
        assert_eq!(
            probe("macos", "/Applications/ShapeCraft.app/Contents/MacOS/shapecraft", &[], &[], None),
            Install::MacApp("/Applications/ShapeCraft.app".into())
        );
        assert!(matches!(
            probe("macos", "/private/var/folders/x/AppTranslocation/1/d/ShapeCraft.app/Contents/MacOS/shapecraft", &[], &[], None),
            Install::Elsewhere(_)
        ));
        assert!(matches!(
            probe("macos", "/Volumes/ShapeCraft 0.4.0/ShapeCraft.app/Contents/MacOS/shapecraft", &[], &[], None),
            Install::Elsewhere(_)
        ));
        assert_eq!(
            probe(
                "windows",
                "C:/Users/a/AppData/Local/Programs/ShapeCraft/shapecraft.exe",
                &[],
                &["C:/Users/a/AppData/Local/Programs/ShapeCraft/unins000.exe"],
                None
            ),
            Install::WindowsInstalled("C:/Users/a/AppData/Local/Programs/ShapeCraft".into())
        );
        assert_eq!(
            probe("windows", "D:/Tools/ShapeCraft/shapecraft.exe", &[], &["D:/Tools/ShapeCraft/shapecraft-cli.exe"], None),
            Install::WindowsPortable("D:/Tools/ShapeCraft".into())
        );
        assert_eq!(
            probe("linux", "/tmp/.mount_x/usr/bin/shapecraft", &[("APPIMAGE", "/home/a/Apps/ShapeCraft.AppImage")], &[], None),
            Install::AppImage("/home/a/Apps/ShapeCraft.AppImage".into())
        );
        assert_eq!(
            probe("linux", "/home/a/.local/lib/shapecraft/bin/shapecraft", &[], &["/home/a/.local/lib/shapecraft/lib"], None),
            Install::LinuxFolder("/home/a/.local/lib/shapecraft".into())
        );
        assert!(matches!(probe("linux", "/usr/bin/shapecraft", &[], &["/usr/lib"], None), Install::Elsewhere(_)));
    }

    #[test]
    fn a_launcher_takes_over() {
        let exe = "/home/a/.local/share/artcraft-launcher/apps/shapecraft/ShapeCraft.AppImage";
        assert_eq!(
            probe("linux", exe, &[(MANAGED_ENV, "ArtCraft Launcher\u{7}")], &[], None),
            Install::Managed("ArtCraft Launcher".into())
        );
        let record = r#"{"apps":{"shapecraft":{"path":"/home/a/.local/share/artcraft-launcher/apps/shapecraft/ShapeCraft.AppImage"}}}"#;
        assert_eq!(
            probe("linux", "/x/usr/bin/shapecraft", &[("APPIMAGE", exe), ("HOME", "/home/a")], &[], Some(record)),
            Install::Managed(LAUNCHER_NAME.into())
        );
        // A record for somewhere else, or a relative path, claims nothing.
        let other = r#"{"apps":{"shapecraft":{"path":"/opt/elsewhere"}}}"#;
        assert_eq!(
            probe("linux", "/x/usr/bin/shapecraft", &[("APPIMAGE", exe), ("HOME", "/home/a")], &[], Some(other)),
            Install::AppImage(exe.into())
        );
        let relative = r#"{"apps":{"shapecraft":{"path":"apps"}}}"#;
        assert_eq!(
            probe("linux", "/x/usr/bin/shapecraft", &[("APPIMAGE", exe), ("HOME", "/home/a")], &[], Some(relative)),
            Install::AppImage(exe.into())
        );
    }

    #[test]
    fn the_long_path_prefix_goes() {
        assert_eq!(plain(PathBuf::from(r"\\?\C:\sc\portable")), PathBuf::from(r"C:\sc\portable"));
        assert_eq!(plain(PathBuf::from(r"\\?\UNC\srv\share")), PathBuf::from(r"\\?\UNC\srv\share"));
        assert_eq!(plain(PathBuf::from("/Applications/ShapeCraft.app")), PathBuf::from("/Applications/ShapeCraft.app"));
    }

    #[test]
    fn windows_paths_compare_without_case() {
        assert!(same_or_inside(
            Path::new(r"C:\Users\A\Programs\ShapeCraft\shapecraft.exe"),
            Path::new(r"c:\users\a\programs\shapecraft"),
            true
        ));
        assert!(!same_or_inside(Path::new("/opt/shapecraft-evil/x"), Path::new("/opt/shapecraft"), false));
        assert!(!same_or_inside(Path::new("C:/x"), Path::new("C:"), true));
    }
}
