//! Cloud storage through the apps people already have (Preferences ▸ Sync).
//!
//! ShapeCraft never signs in anywhere: Google Drive, OneDrive, Dropbox and iCloud Drive each keep a folder on the
//! computer in step with the cloud, so a project saved there is synced by that app (offline, retries and conflicts
//! included). This finds those folders. Each one gets a `ShapeCraft` folder inside, a location on the projects screen.
//!
//! Finding them only reads small files and folder names; it runs off the UI thread anyway (see [`Detector`]), because
//! the first look inside `~/Library/CloudStorage` may wait for macOS to ask the person for access.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    GoogleDrive,
    OneDrive,
    Dropbox,
    ICloud,
}

pub const PROVIDERS: [Provider; 4] = [Provider::GoogleDrive, Provider::OneDrive, Provider::Dropbox, Provider::ICloud];

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::GoogleDrive => "Google Drive",
            Self::OneDrive => "OneDrive",
            Self::Dropbox => "Dropbox",
            Self::ICloud => "iCloud Drive",
        }
    }

    /// A short id for settings (stable across renames of the display name).
    pub fn id(self) -> &'static str {
        match self {
            Self::GoogleDrive => "google",
            Self::OneDrive => "onedrive",
            Self::Dropbox => "dropbox",
            Self::ICloud => "icloud",
        }
    }

    /// Where to get the app (the official download page).
    pub fn download_url(self) -> &'static str {
        match self {
            Self::GoogleDrive => "https://www.google.com/drive/download/",
            Self::OneDrive => "https://www.microsoft.com/microsoft-365/onedrive/download",
            Self::Dropbox => "https://www.dropbox.com/install",
            Self::ICloud if cfg!(windows) => "https://apps.microsoft.com/detail/9pktq5699m62",
            Self::ICloud => "https://support.apple.com/en-us/109344",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// How far a storage app is set up here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Signed in: its synced folder is `root`.
    Ready,
    /// The app is here but there's no synced folder yet (not signed in, or still setting up).
    NotSignedIn,
    /// The app isn't on this computer.
    NotInstalled,
    /// There's no official app for this system (Google Drive and iCloud on Linux).
    Unsupported,
}

/// One storage app's synced folder (or why there isn't one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cloud {
    pub provider: Provider,
    /// Which account, when an app can have several ("Personal", a company, an email address); empty otherwise.
    pub account: String,
    /// The synced folder (empty unless Ready).
    pub root: PathBuf,
    pub state: State,
}

impl Cloud {
    /// Its name in the app: "Dropbox", "OneDrive (Contoso)".
    pub fn label(&self) -> String {
        if self.account.is_empty() { self.provider.name().to_string() } else { format!("{} ({})", self.provider.name(), self.account) }
    }

    /// The id a backup to it is saved under in settings: provider, plus the account when there is one.
    pub fn key(&self) -> String {
        if self.account.is_empty() { self.provider.id().to_string() } else { format!("{}:{}", self.provider.id(), self.account) }
    }

    fn missing(provider: Provider, state: State) -> Self {
        Self { provider, account: String::new(), root: PathBuf::new(), state }
    }

    fn ready(provider: Provider, account: impl Into<String>, root: PathBuf) -> Self {
        Self { provider, account: account.into(), root, state: State::Ready }
    }
}

/// What detection looks at, so tests can point it at a made-up home folder.
pub struct Probe {
    pub os: Os,
    pub home: PathBuf,
    pub vars: HashMap<String, OsString>,
    /// Folders an installed app would be found in (macOS: /Applications and ~/Applications).
    pub app_dirs: Vec<PathBuf>,
    /// Folders searched for a program (Linux: $PATH).
    pub path: Vec<PathBuf>,
    /// Drive roots (Windows: the letters that exist), where Google Drive's virtual drive appears.
    pub drives: Vec<PathBuf>,
    /// The Documents folder (to tell when it's already synced by one of the apps).
    pub documents: PathBuf,
    /// Windows: OneDrive's accounts as its registry lists them (`None` when it couldn't be read).
    pub onedrive_accounts: Option<Vec<OneDriveAccount>>,
    /// Windows: folders registered for this user, by id or name, wherever the person put them: the system's sync
    /// roots (`HKLM\…\Explorer\SyncRootManager\<Provider>!<SID>!<account>\UserSyncRoots`, where OneDrive, Dropbox
    /// and iCloud register) and Explorer's sidebar entries (`HKCU\…\Explorer\Desktop\NameSpace` → each CLSID's
    /// `Instance\InitPropertyBag\TargetFolderPath`).
    pub registered: Vec<(String, PathBuf)>,
    /// Windows: Google Drive's own settings (`HKCU\Software\Google\DriveFS`): DefaultMountPoint and the
    /// per-account preferences, as written.
    pub drivefs: Vec<String>,
}

/// One OneDrive account from `HKCU\Software\Microsoft\OneDrive\Accounts\<key>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OneDriveAccount {
    /// "Personal", "Business1", …
    pub key: String,
    /// The organisation's name for a work account ("OneDrive - Contoso" shows as "Contoso").
    pub display: String,
    /// Where its folder is: OneDrive keeps it here, wherever the person chose to put it.
    pub folder: PathBuf,
    /// The key and folder are there before anyone signs in; an email (or Personal's cid) means someone did.
    pub signed_in: bool,
}

#[cfg(windows)]
fn read_onedrive_accounts() -> Option<Vec<OneDriveAccount>> {
    let accounts = windows_registry::CURRENT_USER.open(r"Software\Microsoft\OneDrive\Accounts").ok()?;
    let mut out = Vec::new();
    for key in accounts.keys().ok()? {
        let Ok(k) = accounts.open(&key) else { continue };
        let get = |name: &str| k.get_string(name).unwrap_or_default();
        let folder = get("UserFolder");
        if folder.is_empty() {
            continue;
        }
        let signed_in = !get("UserEmail").is_empty() || !get("cid").is_empty();
        out.push(OneDriveAccount { display: get("DisplayName"), key, folder: PathBuf::from(folder), signed_in });
    }
    Some(out)
}

#[cfg(not(windows))]
fn read_onedrive_accounts() -> Option<Vec<OneDriveAccount>> {
    None
}

/// The sync roots registered for this user, and Explorer's sidebar folders (see `Probe::registered`).
#[cfg(windows)]
fn read_registered(home: &Path, vars: &HashMap<String, OsString>) -> Vec<(String, PathBuf)> {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
    let mut out = Vec::new();
    // This user's SID: the profile whose folder is home (sync roots of other people on the machine are left out).
    let sid = LOCAL_MACHINE.open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList").ok().and_then(|list| {
        list.keys().ok()?.find(|sid| {
            list.open(sid)
                .and_then(|k| k.get_string("ProfileImagePath"))
                .is_ok_and(|p| PathBuf::from(expand_vars(&p, vars)).as_os_str().eq_ignore_ascii_case(home.as_os_str()))
        })
    });
    if let (Some(sid), Ok(roots)) = (&sid, LOCAL_MACHINE.open(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager")) {
        for id in roots.keys().into_iter().flatten() {
            if !id.contains(sid.as_str()) {
                continue;
            }
            if let Ok(path) = roots.open(format!(r"{id}\UserSyncRoots")).and_then(|k| k.get_string(sid)) {
                out.push((id, PathBuf::from(path)));
            }
        }
    }
    if let Ok(ns) = CURRENT_USER.open(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace") {
        for clsid in ns.keys().into_iter().flatten() {
            let name = ns.open(&clsid).and_then(|k| k.get_string("")).unwrap_or_default();
            let bag = format!(r"Software\Classes\CLSID\{clsid}\Instance\InitPropertyBag");
            if let Ok(path) = CURRENT_USER.open(&bag).and_then(|k| k.get_string("TargetFolderPath")) {
                out.push((name, PathBuf::from(expand_vars(&path, vars))));
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn read_registered(_: &Path, _: &HashMap<String, OsString>) -> Vec<(String, PathBuf)> {
    Vec::new()
}

#[cfg(windows)]
fn read_drivefs() -> Vec<String> {
    let Ok(k) = windows_registry::CURRENT_USER.open(r"Software\Google\DriveFS") else { return Vec::new() };
    ["DefaultMountPoint", "PerAccountPreferences"].iter().filter_map(|n| k.get_string(n).ok()).collect()
}

#[cfg(not(windows))]
fn read_drivefs() -> Vec<String> {
    Vec::new()
}

/// `%USERPROFILE%\x` → `C:\Users\me\x` (registry paths may name variables).
fn expand_vars(s: &str, vars: &HashMap<String, OsString>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('%') {
        let Some(b) = rest[a + 1..].find('%') else { break };
        let name = &rest[a + 1..a + 1 + b];
        out.push_str(&rest[..a]);
        match vars.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
            Some((_, v)) => out.push_str(&v.to_string_lossy()),
            None => out.push_str(&rest[a..a + b + 2]),
        }
        rest = &rest[a + b + 2..];
    }
    out.push_str(rest);
    out
}

/// Every path Google Drive's settings name: a drive letter ("G", "G:"), a folder, or paths inside its
/// per-account preferences (JSON), each as the folder that holds My Drive or is it.
fn drivefs_folders(settings: &[String], vars: &HashMap<String, OsString>) -> Vec<PathBuf> {
    fn strings(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
            serde_json::Value::Object(o) => o.values().for_each(|x| strings(x, out)),
            _ => {}
        }
    }
    let mut raw = Vec::new();
    for s in settings {
        match serde_json::from_str::<serde_json::Value>(s) {
            Ok(v @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) => strings(&v, &mut raw),
            _ => raw.push(s.clone()),
        }
    }
    raw.iter()
        .map(|s| expand_vars(s.trim(), vars))
        .filter_map(|s| {
            let letter = s.trim_end_matches([':', '\\']);
            if letter.len() == 1 && letter.chars().all(|c| c.is_ascii_alphabetic()) {
                Some(PathBuf::from(format!("{}:\\", letter.to_ascii_uppercase())))
            } else if Path::new(&s).is_absolute() || (s.len() > 2 && s.as_bytes()[1] == b':') || s.starts_with(r"\\") {
                Some(PathBuf::from(s))
            } else {
                None
            }
        })
        .collect()
}

/// A folder registered for a provider: by sync-root id ("OneDrive!S-1-…") or sidebar name ("iCloud Drive").
fn registered_for<'a>(p: &'a Probe, words: &'a [&str]) -> impl Iterator<Item = &'a PathBuf> + 'a {
    p.registered.iter().filter(move |(id, _)| words.iter().any(|w| id.to_lowercase().contains(w))).map(|(_, path)| path)
}

impl Probe {
    pub fn current() -> Self {
        let os = Os::current();
        let home = dirs::home_dir().unwrap_or_default();
        let vars: HashMap<String, OsString> = std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v))).collect();
        let path = vars.get("PATH").map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
        let drives = if os == Os::Windows {
            (b'C'..=b'Z').map(|c| PathBuf::from(format!("{}:\\", c as char))).filter(|d| d.exists()).collect()
        } else {
            Vec::new()
        };
        let app_dirs = vec![PathBuf::from("/Applications"), home.join("Applications")];
        let documents = dirs::document_dir().unwrap_or_else(|| home.join("Documents"));
        let onedrive_accounts = read_onedrive_accounts();
        let registered = read_registered(&home, &vars);
        let drivefs = read_drivefs();
        Self { os, home, vars, app_dirs, path, drives, documents, onedrive_accounts, registered, drivefs }
    }

    fn var(&self, k: &str) -> Option<PathBuf> {
        self.vars.get(k).filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    fn has_app(&self, name: &str) -> bool {
        self.app_dirs.iter().any(|d| d.join(name).exists())
    }

    fn has_program(&self, name: &str) -> bool {
        self.path.iter().any(|d| d.join(name).is_file())
    }
}

/// Every storage app, as set up on this computer: one entry per signed-in account, or one saying why there's none.
pub fn detect(p: &Probe) -> Vec<Cloud> {
    let mut out = Vec::new();
    for provider in PROVIDERS {
        let found = match provider {
            Provider::GoogleDrive => google(p),
            Provider::OneDrive => onedrive(p),
            Provider::Dropbox => dropbox(p),
            Provider::ICloud => icloud(p),
        };
        out.extend(found);
    }
    out
}

/// Folders in `dir` whose names start with `prefix` (macOS keeps one per account in ~/Library/CloudStorage).
fn prefixed(dir: &Path, prefix: &str) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(String, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let rest = name.strip_prefix(prefix)?;
            (rest.is_empty() || rest.starts_with('-')).then(|| (rest.trim_start_matches('-').to_string(), e.path()))
        })
        .filter(|(_, p)| p.is_dir())
        .collect();
    v.sort();
    v
}

/// The "My Drive" folder inside a Google Drive account folder. Its name is translated ("Mi unidad"), so failing the
/// English name it's the one folder that isn't shared drives, other computers or hidden.
fn my_drive(account: &Path) -> Option<PathBuf> {
    let en = account.join("My Drive");
    if en.is_dir() {
        return Some(en);
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(account)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            !n.starts_with('.') && !["Shared drives", "Other computers"].contains(&n.as_str())
        })
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    (dirs.len() == 1).then(|| dirs.remove(0))
}

fn google(p: &Probe) -> Vec<Cloud> {
    let g = Provider::GoogleDrive;
    match p.os {
        Os::Mac => {
            let found: Vec<Cloud> = prefixed(&p.home.join("Library/CloudStorage"), "GoogleDrive")
                .into_iter()
                .filter_map(|(account, dir)| Some(Cloud::ready(g, account, my_drive(&dir)?)))
                .collect();
            if !found.is_empty() {
                found
            } else if p.has_app("Google Drive.app") {
                vec![Cloud::missing(g, State::NotSignedIn)]
            } else {
                vec![Cloud::missing(g, State::NotInstalled)]
            }
        }
        Os::Windows => {
            // Where its settings say (a drive letter or folder, the person's choice), then a folder registered with
            // Windows, then the defaults: streaming puts My Drive on its own drive letter (G:), mirroring in home.
            let named = drivefs_folders(&p.drivefs, &p.vars).into_iter().chain(registered_for(p, &["google", "drivefs"]).cloned());
            let mut candidates: Vec<PathBuf> = Vec::new();
            for d in named.chain(p.drives.iter().cloned()).chain(std::iter::once(p.home.clone())) {
                let inner = d.join("My Drive");
                let pick = if inner.is_dir() {
                    inner
                } else if d.file_name().is_some_and(|n| n == "My Drive") {
                    d
                } else {
                    continue;
                };
                if pick.is_dir() && !candidates.contains(&pick) {
                    candidates.push(pick);
                }
            }
            let found: Vec<Cloud> = candidates.into_iter().map(|d| Cloud::ready(g, "", d)).collect();
            let installed = p.var("ProgramFiles").is_some_and(|d| d.join("Google/Drive File Stream").is_dir())
                || p.var("LOCALAPPDATA").is_some_and(|d| d.join("Google/DriveFS").is_dir());
            if !found.is_empty() {
                found.into_iter().take(1).collect()
            } else if installed {
                vec![Cloud::missing(g, State::NotSignedIn)]
            } else {
                vec![Cloud::missing(g, State::NotInstalled)]
            }
        }
        Os::Linux => vec![Cloud::missing(g, State::Unsupported)],
    }
}

fn onedrive(p: &Probe) -> Vec<Cloud> {
    let o = Provider::OneDrive;
    match p.os {
        Os::Mac => {
            let found: Vec<Cloud> = prefixed(&p.home.join("Library/CloudStorage"), "OneDrive")
                .into_iter()
                .map(|(account, dir)| Cloud::ready(o, account, dir))
                .collect();
            if !found.is_empty() {
                found
            } else if p.has_app("OneDrive.app") {
                vec![Cloud::missing(o, State::NotSignedIn)]
            } else {
                vec![Cloud::missing(o, State::NotInstalled)]
            }
        }
        Os::Windows => {
            let mut found = Vec::new();
            if let Some(accounts) = &p.onedrive_accounts {
                // The registry is the record: the folder exists (and the variables are set) before anyone signs in.
                for a in accounts.iter().filter(|a| a.signed_in && a.folder.is_dir()) {
                    let account = if a.key == "Personal" {
                        "Personal".to_string()
                    } else if !a.display.is_empty() {
                        a.display.clone()
                    } else {
                        "Work or School".to_string()
                    };
                    if !found.iter().any(|c: &Cloud| c.root == a.folder) {
                        found.push(Cloud::ready(o, account, a.folder.clone()));
                    }
                }
            } else {
                // No registry to read: OneDrive sets these for its accounts; %OneDrive% repeats one of the other two.
                for (var, account) in [("OneDriveConsumer", "Personal"), ("OneDriveCommercial", "Work or School"), ("OneDrive", "")] {
                    if let Some(d) = p.var(var).filter(|d| d.is_dir())
                        && !found.iter().any(|c: &Cloud| c.root == d)
                    {
                        found.push(Cloud::ready(o, account, d));
                    }
                }
            }
            if found.len() == 1 {
                found[0].account.clear();
            }
            let installed = p.var("LOCALAPPDATA").is_some_and(|d| d.join("Microsoft/OneDrive/OneDrive.exe").is_file())
                || p.var("ProgramFiles").is_some_and(|d| d.join("Microsoft OneDrive/OneDrive.exe").is_file());
            if !found.is_empty() {
                found
            } else if installed {
                vec![Cloud::missing(o, State::NotSignedIn)]
            } else {
                vec![Cloud::missing(o, State::NotInstalled)]
            }
        }
        Os::Linux => {
            // The community client (abraunegg/onedrive): sync_dir in its config, else ~/OneDrive.
            let config = p.var("XDG_CONFIG_HOME").unwrap_or_else(|| p.home.join(".config")).join("onedrive/config");
            let dir = std::fs::read_to_string(config)
                .ok()
                .and_then(|text| onedrive_sync_dir(&text))
                .map(|d| expand_home(&d, &p.home))
                .unwrap_or_else(|| p.home.join("OneDrive"));
            if p.has_program("onedrive") {
                vec![if dir.is_dir() { Cloud::ready(o, "", dir) } else { Cloud::missing(o, State::NotSignedIn) }]
            } else {
                vec![Cloud::missing(o, State::NotInstalled)]
            }
        }
    }
}

/// `sync_dir = "~/OneDrive"` from the Linux client's config.
fn onedrive_sync_dir(config: &str) -> Option<String> {
    config.lines().map(str::trim).filter(|l| !l.starts_with('#')).find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "sync_dir").then(|| v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
    })
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if path == "~" => home.to_path_buf(),
        None => PathBuf::from(path),
    }
}

/// Dropbox writes where its folders are to info.json: `{"personal": {"path": …}, "business": {"path": …}}`.
fn dropbox_info(file: &Path) -> Vec<(String, PathBuf)> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    let mut out = Vec::new();
    for (kind, account) in [("personal", "Personal"), ("business", "Business")] {
        if let Some(path) = v.get(kind).and_then(|a| a.get("path")).and_then(|p| p.as_str()) {
            out.push((account.to_string(), PathBuf::from(path)));
        }
    }
    out
}

fn dropbox(p: &Probe) -> Vec<Cloud> {
    let d = Provider::Dropbox;
    let infos: Vec<PathBuf> = match p.os {
        Os::Windows => ["APPDATA", "LOCALAPPDATA"].iter().filter_map(|v| p.var(v)).map(|d| d.join("Dropbox/info.json")).collect(),
        _ => vec![p.home.join(".dropbox/info.json")],
    };
    let mut found: Vec<Cloud> = Vec::new();
    for (account, root) in infos.iter().flat_map(|f| dropbox_info(f)) {
        if root.is_dir() && !found.iter().any(|c| c.root == root) {
            found.push(Cloud::ready(d, account, root));
        }
    }
    // No info.json: where it registered with Windows, then the usual places.
    if found.is_empty() {
        let usual = match p.os {
            Os::Mac => prefixed(&p.home.join("Library/CloudStorage"), "Dropbox").into_iter().map(|(_, dir)| dir).collect(),
            Os::Windows => registered_for(p, &["dropbox"]).cloned().chain(std::iter::once(p.home.join("Dropbox"))).collect(),
            Os::Linux => vec![p.home.join("Dropbox")],
        };
        found.extend(usual.into_iter().filter(|r| r.is_dir()).take(1).map(|r| Cloud::ready(d, "", r)));
    }
    if found.len() == 1 {
        found[0].account.clear();
    }
    let installed = match p.os {
        Os::Mac => p.has_app("Dropbox.app"),
        Os::Windows => {
            ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(|v| p.var(v)).any(|d| d.join("Dropbox/Client").is_dir())
                || p.var("LOCALAPPDATA").is_some_and(|d| d.join("Dropbox/Client").is_dir())
        }
        Os::Linux => p.home.join(".dropbox-dist").is_dir() || p.has_program("dropbox"),
    };
    if !found.is_empty() {
        found
    } else if installed {
        vec![Cloud::missing(d, State::NotSignedIn)]
    } else {
        vec![Cloud::missing(d, State::NotInstalled)]
    }
}

fn icloud(p: &Probe) -> Vec<Cloud> {
    let i = Provider::ICloud;
    match p.os {
        // Built into macOS: "not signed in" means iCloud Drive is off in System Settings.
        Os::Mac => {
            let docs = p.home.join("Library/Mobile Documents/com~apple~CloudDocs");
            vec![if docs.is_dir() { Cloud::ready(i, "", docs) } else { Cloud::missing(i, State::NotSignedIn) }]
        }
        Os::Windows => {
            // Wherever it was moved to (iCloud for Windows 14 and later can), as registered; then the defaults (Apple
            // documents "iCloud Drive"; older versions made "iCloudDrive").
            let docs = registered_for(p, &["icloud"])
                .cloned()
                .chain([p.home.join("iCloud Drive"), p.home.join("iCloudDrive")])
                .find(|d| d.is_dir())
                .unwrap_or_default();
            let installed = p.var("LOCALAPPDATA").is_some_and(|d| {
                std::fs::read_dir(d.join("Packages"))
                    .map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("AppleInc.iCloud")))
                    .unwrap_or(false)
            });
            vec![if !docs.as_os_str().is_empty() {
                Cloud::ready(i, "", docs)
            } else if installed {
                Cloud::missing(i, State::NotSignedIn)
            } else {
                Cloud::missing(i, State::NotInstalled)
            }]
        }
        Os::Linux => vec![Cloud::missing(i, State::Unsupported)],
    }
}

/// Which app already syncs the Documents folder (so projects "on this computer" are in the cloud too): OneDrive's
/// folder backup moves Documents inside its folder; iCloud's "Desktop & Documents Folders" keeps it in place.
pub fn documents_synced_by(p: &Probe, clouds: &[Cloud], icloud_desktop: bool) -> Option<Provider> {
    if let Some(c) = clouds.iter().find(|c| c.state == State::Ready && !c.root.as_os_str().is_empty() && p.documents.starts_with(&c.root)) {
        return Some(c.provider);
    }
    (p.os == Os::Mac && icloud_desktop).then_some(Provider::ICloud)
}

/// Whether iCloud's "Desktop & Documents Folders" is on (macOS keeps it in the iCloud account's settings).
pub fn icloud_desktop_on() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let Ok(out) = std::process::Command::new("/usr/bin/defaults").args(["read", "MobileMeAccounts"]).output() else { return false };
    icloud_desktop_in(&String::from_utf8_lossy(&out.stdout))
}

/// The CloudDesktop service, with `status = active` in the same entry.
fn icloud_desktop_in(defaults: &str) -> bool {
    let lines: Vec<&str> = defaults.lines().collect();
    lines.iter().enumerate().any(|(i, l)| {
        if !l.contains("com.apple.Dataclass.CloudDesktop") {
            return false;
        }
        // The entry runs from its `{` to its `}`.
        let start = lines[..i].iter().rposition(|l| l.contains('{')).unwrap_or(0);
        let end = lines[i..].iter().position(|l| l.contains('}')).map_or(lines.len(), |k| i + k);
        lines[start..end].iter().any(|l| l.contains("status = active"))
    })
}

/// What detection found, with the Documents note.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub clouds: Vec<Cloud>,
    pub documents_synced_by: Option<Provider>,
}

impl Found {
    pub fn ready(&self) -> impl Iterator<Item = &Cloud> {
        self.clouds.iter().filter(|c| c.state == State::Ready)
    }
}

/// Detection off the UI thread: started once, run again on request (the Sync tab's Refresh, the app regaining focus).
#[derive(Clone, Default)]
pub struct Detector {
    found: Arc<Mutex<Option<Found>>>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl Detector {
    /// Starts a look (unless one is running); `wake` runs when the answer is in.
    pub fn refresh(&self, wake: impl Fn() + Send + 'static) {
        use std::sync::atomic::Ordering;
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (out, running) = (self.found.clone(), self.running.clone());
        let spawned = std::thread::Builder::new().name("cloud-detect".into()).spawn(move || {
            let p = Probe::current();
            let clouds = detect(&p);
            let documents_synced_by = documents_synced_by(&p, &clouds, icloud_desktop_on());
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(Found { clouds, documents_synced_by });
            running.store(false, Ordering::SeqCst);
            wake();
        });
        if spawned.is_err() {
            self.running.store(false, Ordering::SeqCst);
        }
    }

    /// The latest answer (None until the first look finishes).
    pub fn found(&self) -> Option<Found> {
        self.found.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn busy(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// The app's one detector (the Sync tab and the projects screen share it).
pub fn detector() -> &'static Detector {
    static D: std::sync::OnceLock<Detector> = std::sync::OnceLock::new();
    D.get_or_init(Detector::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        dir: PathBuf,
    }

    impl Fake {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sc-cloud-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn mkdir(&self, rel: &str) -> PathBuf {
            let p = self.dir.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            p
        }

        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let p = self.dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
            p
        }

        fn probe(&self, os: Os) -> Probe {
            let home = self.dir.join("home");
            Probe {
                os,
                documents: home.join("Documents"),
                home,
                vars: HashMap::new(),
                app_dirs: vec![self.dir.join("Applications")],
                path: vec![self.dir.join("bin")],
                drives: Vec::new(),
                onedrive_accounts: None,
                registered: Vec::new(),
                drivefs: Vec::new(),
            }
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn of(found: &[Cloud], p: Provider) -> Vec<&Cloud> {
        found.iter().filter(|c| c.provider == p).collect()
    }

    #[test]
    fn nothing_installed_says_so_on_each_system() {
        let f = Fake::new("none");
        for (os, google, icloud) in [
            (Os::Mac, State::NotInstalled, State::NotSignedIn),
            (Os::Windows, State::NotInstalled, State::NotInstalled),
            (Os::Linux, State::Unsupported, State::Unsupported),
        ] {
            let found = detect(&f.probe(os));
            assert_eq!(found.len(), 4, "{os:?}");
            assert_eq!(of(&found, Provider::GoogleDrive)[0].state, google);
            assert_eq!(of(&found, Provider::OneDrive)[0].state, State::NotInstalled);
            assert_eq!(of(&found, Provider::Dropbox)[0].state, State::NotInstalled);
            assert_eq!(of(&found, Provider::ICloud)[0].state, icloud);
        }
    }

    #[test]
    fn mac_cloud_storage_folders() {
        let f = Fake::new("mac");
        let me = f.mkdir("home/Library/CloudStorage/GoogleDrive-me@example.com/My Drive");
        f.mkdir("home/Library/CloudStorage/GoogleDrive-me@example.com/Shared drives");
        let es = f.mkdir("home/Library/CloudStorage/GoogleDrive-otra@example.com/Mi unidad");
        let personal = f.mkdir("home/Library/CloudStorage/OneDrive-Personal");
        let work = f.mkdir("home/Library/CloudStorage/OneDrive-Contoso");
        let dbx = f.mkdir("home/Library/CloudStorage/Dropbox");
        let icloud = f.mkdir("home/Library/Mobile Documents/com~apple~CloudDocs");
        let found = detect(&f.probe(Os::Mac));
        let g = of(&found, Provider::GoogleDrive);
        assert_eq!(g.len(), 2);
        assert_eq!((g[0].account.as_str(), &g[0].root), ("me@example.com", &me));
        assert_eq!((g[1].account.as_str(), &g[1].root), ("otra@example.com", &es));
        let o = of(&found, Provider::OneDrive);
        assert_eq!(o.iter().map(|c| (c.account.as_str(), &c.root)).collect::<Vec<_>>(), [("Contoso", &work), ("Personal", &personal)]);
        assert_eq!(o[0].label(), "OneDrive (Contoso)");
        let d = of(&found, Provider::Dropbox);
        assert_eq!((d[0].state.clone(), &d[0].root, d[0].label()), (State::Ready, &dbx, "Dropbox".to_string()));
        assert_eq!(of(&found, Provider::ICloud)[0].root, icloud);
    }

    #[test]
    fn installed_but_not_signed_in() {
        let f = Fake::new("signin");
        for app in ["Google Drive.app", "OneDrive.app", "Dropbox.app"] {
            f.mkdir(&format!("Applications/{app}"));
        }
        let found = detect(&f.probe(Os::Mac));
        for p in [Provider::GoogleDrive, Provider::OneDrive, Provider::Dropbox] {
            assert_eq!(of(&found, p)[0].state, State::NotSignedIn, "{p:?}");
        }
    }

    #[test]
    fn dropbox_info_json_wins_and_names_both_accounts() {
        let f = Fake::new("dbx");
        let personal = f.mkdir("elsewhere/Dropbox (Personal)");
        let business = f.mkdir("elsewhere/Dropbox (Acme)");
        f.mkdir("home/Dropbox");
        let json = serde_json::json!({
            "personal": { "path": personal, "host": 1, "is_team": false },
            "business": { "path": business, "host": 2, "is_team": true },
        });
        f.write("home/.dropbox/info.json", &json.to_string());
        let found = detect(&f.probe(Os::Linux));
        let d = of(&found, Provider::Dropbox);
        assert_eq!(d.iter().map(|c| (c.account.as_str(), &c.root)).collect::<Vec<_>>(), [("Personal", &personal), ("Business", &business)]);
    }

    #[test]
    fn windows_folders_from_environment_and_drive_letters() {
        let f = Fake::new("win");
        let mut p = f.probe(Os::Windows);
        let consumer = f.mkdir("home/OneDrive");
        let commercial = f.mkdir("home/OneDrive - Contoso");
        p.vars.insert("OneDriveConsumer".into(), consumer.clone().into());
        p.vars.insert("OneDriveCommercial".into(), commercial.clone().into());
        p.vars.insert("OneDrive".into(), commercial.clone().into());
        let g = f.mkdir("G/My Drive");
        p.drives = vec![f.dir.join("C"), f.dir.join("G")];
        let appdata = f.mkdir("AppData/Roaming");
        let dbx = f.mkdir("D/Dropbox");
        f.write("AppData/Roaming/Dropbox/info.json", &serde_json::json!({ "personal": { "path": dbx } }).to_string());
        p.vars.insert("APPDATA".into(), appdata.into());
        let icloud = f.mkdir("home/iCloudDrive");
        let found = detect(&p);
        assert_eq!(of(&found, Provider::GoogleDrive)[0].root, g);
        let o = of(&found, Provider::OneDrive);
        assert_eq!(
            o.iter().map(|c| (c.account.as_str(), &c.root)).collect::<Vec<_>>(),
            [("Personal", &consumer), ("Work or School", &commercial)]
        );
        assert_eq!(of(&found, Provider::Dropbox)[0].root, dbx);
        assert_eq!(of(&found, Provider::ICloud)[0].root, icloud);
        // Documents moved into OneDrive by its folder backup.
        p.documents = consumer.join("Documents");
        assert_eq!(documents_synced_by(&p, &found, false), Some(Provider::OneDrive));
    }

    #[test]
    fn windows_onedrive_registry_beats_the_variables() {
        let f = Fake::new("winreg");
        let mut p = f.probe(Os::Windows);
        let personal = f.mkdir("home/OneDrive");
        let work = f.mkdir("E/Work files");
        p.vars.insert("OneDrive".into(), personal.clone().into());
        let acct = |key: &str, display: &str, folder: &PathBuf, signed_in| OneDriveAccount {
            key: key.into(),
            display: display.into(),
            folder: folder.clone(),
            signed_in,
        };
        // Installed, folder made, nobody signed in yet.
        p.onedrive_accounts = Some(vec![acct("Personal", "", &personal, false)]);
        assert_eq!(of(&detect(&p), Provider::OneDrive)[0].state, State::NotInstalled);
        p.vars.insert("ProgramFiles".into(), f.dir.join("PF").into());
        f.write("PF/Microsoft OneDrive/OneDrive.exe", "");
        assert_eq!(of(&detect(&p), Provider::OneDrive)[0].state, State::NotSignedIn);
        // Signed in to both; the work folder was moved to another drive.
        p.onedrive_accounts = Some(vec![acct("Personal", "", &personal, true), acct("Business1", "Contoso", &work, true)]);
        let found = detect(&p);
        let o = of(&found, Provider::OneDrive);
        assert_eq!(o.iter().map(|c| (c.account.as_str(), &c.root)).collect::<Vec<_>>(), [("Personal", &personal), ("Contoso", &work)]);
    }

    #[test]
    fn windows_folders_wherever_they_were_moved() {
        let f = Fake::new("winmoved");
        let mut p = f.probe(Os::Windows);
        p.vars.insert("USERPROFILE".into(), p.home.clone().into());
        // Google Drive mirroring into a folder of the person's choosing, named in its per-account preferences.
        let mirror = f.mkdir("E/Sync/Google/My Drive");
        let prefs =
            serde_json::json!({ "per_account_preferences": [{ "key": "1234", "value": { "mirror_path": f.dir.join("E/Sync/Google") } }] });
        p.drivefs = vec!["".into(), prefs.to_string()];
        // iCloud moved to another drive, and Dropbox known only by its sync root.
        let icloud = f.mkdir("D/Apple/iCloud Drive");
        let dbx = f.mkdir("D/Work Dropbox");
        p.registered = vec![("iCloudDrive!S-1-5-21-1!".into(), icloud.clone()), ("Dropbox!S-1-5-21-1!Personal".into(), dbx.clone())];
        let found = detect(&p);
        assert_eq!(of(&found, Provider::GoogleDrive)[0].root, mirror);
        assert_eq!(of(&found, Provider::ICloud)[0].root, icloud);
        assert_eq!(of(&found, Provider::Dropbox)[0].root, dbx);
        // A drive letter alone, as DefaultMountPoint holds it.
        assert_eq!(drivefs_folders(&["G".into(), "H:".into()], &p.vars), [PathBuf::from("G:\\"), PathBuf::from("H:\\")]);
        assert_eq!(expand_vars(r"%USERPROFILE%\x", &p.vars), format!(r"{}\x", p.home.display()));
        // Apple's documented default name.
        let f2 = Fake::new("winicloud");
        let p2 = f2.probe(Os::Windows);
        let default = f2.mkdir("home/iCloud Drive");
        assert_eq!(of(&detect(&p2), Provider::ICloud)[0].root, default);
    }

    #[test]
    fn linux_onedrive_client_reads_its_config() {
        let f = Fake::new("linux");
        f.write("bin/onedrive", "");
        let mut p = f.probe(Os::Linux);
        assert_eq!(of(&detect(&p), Provider::OneDrive)[0].state, State::NotSignedIn);
        f.write("home/.config/onedrive/config", "# sync_dir = \"~/nope\"\nsync_dir = \"~/Cloud/OneDrive\"\n");
        let d = f.mkdir("home/Cloud/OneDrive");
        assert_eq!(of(&detect(&p), Provider::OneDrive)[0].root, d);
        p.vars.insert("XDG_CONFIG_HOME".into(), f.dir.join("xdg").into());
        f.mkdir("home/OneDrive");
        assert_eq!(of(&detect(&p), Provider::OneDrive)[0].root, f.dir.join("home/OneDrive"));
    }

    /// What this computer has, for checking a machine by hand: `cargo test -p shapecraft-ui cloud::tests::here -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the real home folder"]
    fn here() {
        let p = Probe::current();
        let clouds = detect(&p);
        for c in &clouds {
            println!("{:<28} {:?} {}", c.label(), c.state, c.root.display());
        }
        println!("Documents synced by: {:?}", documents_synced_by(&p, &clouds, icloud_desktop_on()));
        println!("Registered: {:?}", p.registered);
        println!("Google settings: {:?}", p.drivefs);
        println!("OneDrive accounts: {:?}", p.onedrive_accounts);
    }

    #[test]
    fn icloud_desktop_and_documents_flag() {
        let on = "{\n Services = (\n {\n Name = CLOUDDESKTOP;\n ServiceID = \"com.apple.Dataclass.CloudDesktop\";\n status = active;\n },";
        let off = "{\n Name = CLOUDDESKTOP;\n ServiceID = \"com.apple.Dataclass.CloudDesktop\";\n },\n {\n Name = X;\n";
        assert!(icloud_desktop_in(on));
        assert!(!icloud_desktop_in(off));
        let f = Fake::new("docs");
        let p = f.probe(Os::Mac);
        assert_eq!(documents_synced_by(&p, &[], true), Some(Provider::ICloud));
        assert_eq!(documents_synced_by(&p, &[], false), None);
    }
}
