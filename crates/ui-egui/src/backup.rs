//! Backups: copies of the Work Folder kept up to date in other places (cloud storage apps' folders, a NAS, a drive),
//! in the order the person chose. Each backup holds `ShapeCraft Backups/<this computer>/`, laid out exactly like the
//! Work Folder (same folder and project names), so it reads well in Finder or Explorer without ShapeCraft, and two
//! computers backing up to one cloud never write over each other.
//!
//! A backup only ever adds and updates: deleting or renaming a project on this computer leaves its old copy there.
//! The cloud apps keep their own version history on top.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use crate::cloud::Found;
use shapecraft_doc::project;

/// The folder each backup keeps its copies in.
pub const BACKUPS_DIR: &str = "ShapeCraft Backups";
/// Settings' prefix for a backup that's a folder (anything else is a cloud app's key).
const FOLDER: &str = "folder:";
/// How long after the last save a backup starts (saves come in bursts while someone works).
const QUIET: Duration = Duration::from_secs(15);
/// How often everything is checked anyway (a backup that was out of reach, changes made outside the app).
const EVERY: Duration = Duration::from_secs(10 * 60);

/// What the app passes for the thread to call when there's news (a repaint).
type Wake = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;
/// Each backup in order: its settings value, and the folder it backs up into when that's found now.
pub type Targets = Vec<(String, Option<PathBuf>)>;

/// A backup as settings keep it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A cloud app's key (`cloud::Cloud::key`): found again each time, wherever that app's folder is now.
    Cloud(String),
    Folder(PathBuf),
}

impl Target {
    pub fn parse(s: &str) -> Self {
        match s.strip_prefix(FOLDER) {
            Some(p) => Self::Folder(PathBuf::from(p)),
            None => Self::Cloud(s.to_string()),
        }
    }

    pub fn setting(&self) -> String {
        match self {
            Self::Cloud(k) => k.clone(),
            Self::Folder(p) => format!("{FOLDER}{}", p.display()),
        }
    }

    /// The folder it backs up into (its app's folder, or the folder), when it can be found now.
    pub fn root(&self, found: &Found) -> Option<PathBuf> {
        match self {
            Self::Cloud(k) => found.ready().find(|c| c.key() == *k).map(|c| c.root.clone()),
            Self::Folder(p) => Some(p.clone()),
        }
    }

    /// Its name in the app: "Dropbox", "OneDrive (Contoso)", or the folder's own name.
    pub fn label(&self, found: &Found) -> String {
        match self {
            Self::Cloud(k) => found.clouds.iter().find(|c| c.key() == *k).map_or_else(|| cloud_name(k), |c| c.label()),
            Self::Folder(p) => p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()),
        }
    }

    pub fn is_cloud(&self) -> bool {
        matches!(self, Self::Cloud(_))
    }
}

/// A cloud's name from its key alone (the app isn't there now): "dropbox" → "Dropbox", "onedrive:Contoso" → "OneDrive (Contoso)".
fn cloud_name(key: &str) -> String {
    let (id, account) = key.split_once(':').unwrap_or((key, ""));
    let name = crate::cloud::PROVIDERS.iter().find(|p| p.id() == id).map_or(id, |p| p.name());
    if account.is_empty() { name.to_string() } else { format!("{name} ({account})") }
}

/// This computer's name, for its folder in each backup ("Sam's MacBook Pro", "DESKTOP-4F2").
pub fn computer_name() -> String {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        let name = if cfg!(target_os = "macos") {
            std::process::Command::new("/usr/sbin/scutil")
                .args(["--get", "ComputerName"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else if cfg!(windows) {
            std::env::var("COMPUTERNAME").ok()
        } else {
            std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string())
        };
        clean_name(&name.unwrap_or_default())
    })
    .clone()
}

/// A name safe as a folder name on every system.
fn clean_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '-' } else { c }).collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() { "This Computer".to_string() } else { s }
}

/// Where this computer's copies go in a backup: its own folder, or (several computers sharing one Work Folder
/// through a cloud app) straight in ShapeCraft Backups.
pub fn dest(root: &Path, per_computer: bool) -> PathBuf {
    let base = root.join(BACKUPS_DIR);
    if per_computer { base.join(computer_name()) } else { base }
}

/// Where the copies a backup replaced are kept, inside its folder: `Older Versions/<project path>/<date time>/`.
pub const OLDER: &str = "Older Versions";

/// How backups are made (Preferences ▸ Sync).
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub per_computer: bool,
    /// How many replaced copies of each project are kept (0: none).
    pub versions: u32,
    /// Back up a moment after every save, not only on Save (⌘S), closing a project and quitting.
    pub after_change: bool,
}

impl Default for Plan {
    fn default() -> Self {
        Self { per_computer: true, versions: 10, after_change: false }
    }
}

/// What one pass did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pass {
    pub projects: usize,
    /// Projects copied (new or changed since the last pass).
    pub copied: usize,
}

/// Makes `dest` hold everything in `work`: each project copied when it changed, folders made. Nothing in `dest`
/// that's gone from `work` is removed (but a project's own old files are, so each copy opens as it was saved).
pub fn mirror(work: &Path, dest: &Path, versions: u32, progress: &dyn Fn(usize, usize)) -> std::io::Result<Pass> {
    if dest.starts_with(work) || work.starts_with(dest) {
        return Err(std::io::Error::other("the backup and the Work Folder overlap"));
    }
    let mut projects = Vec::new();
    let mut folders = Vec::new();
    walk(work, Path::new(""), &mut projects, &mut folders);
    std::fs::create_dir_all(dest)?;
    for f in &folders {
        std::fs::create_dir_all(dest.join(f))?;
    }
    let mut pass = Pass { projects: projects.len(), copied: 0 };
    for (k, rel) in projects.iter().enumerate() {
        progress(k, projects.len());
        let keep = (versions > 0).then(|| (dest.join(OLDER).join(rel), versions as usize));
        if mirror_project(&work.join(rel), &dest.join(rel), keep)? {
            pass.copied += 1;
        }
    }
    progress(projects.len(), projects.len());
    Ok(pass)
}

/// The projects and plain folders under `dir` (relative), leaving out hidden ones (Recently Deleted, copies in progress).
fn walk(base: &Path, rel: &Path, projects: &mut Vec<PathBuf>, folders: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(base.join(rel)) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') || !e.path().is_dir() {
            continue;
        }
        let r = rel.join(&name);
        if e.path().join(project::PROJECT_FILE).is_file() {
            projects.push(r);
        } else {
            folders.push(r.clone());
            walk(base, &r, projects, folders);
        }
    }
}

fn same(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // Network and FAT drives keep times coarser than ours.
    match (a.modified(), b.modified()) {
        (Ok(x), Ok(y)) => x.duration_since(y).or_else(|_| y.duration_since(x)).is_ok_and(|d| d <= Duration::from_secs(2)),
        _ => false,
    }
}

/// Copies one file through a hidden temporary name and gives it the original's time (how the next pass knows it's done).
fn copy_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = to.with_file_name(format!(".~{name}.tmp"));
    std::fs::copy(from, &tmp)?;
    if let Ok(t) = std::fs::metadata(from).and_then(|m| m.modified()) {
        let _ = std::fs::File::options().write(true).open(&tmp).and_then(|f| f.set_modified(t));
    }
    project::replace(&tmp, to)
}

/// Files of a project, relative (images/ included), leaving out project.json, temporaries and hidden ones.
fn files_of(dir: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir.join(rel)) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.ends_with(".tmp") || (rel.as_os_str().is_empty() && name == project::PROJECT_FILE) {
            continue;
        }
        let r = rel.join(&name);
        if e.path().is_dir() { files_of(dir, &r, out) } else { out.push(r) }
    }
}

/// Brings one project's copy up to date, the way a save does it: its other files first, project.json last (the
/// moment the copy changes over), then the files the new project.json no longer uses. Whether anything was copied.
/// With `keep`, the copy being replaced is moved into that folder first, dated, and only the newest few are kept.
fn mirror_project(src: &Path, dst: &Path, keep: Option<(PathBuf, usize)>) -> std::io::Result<bool> {
    let json = src.join(project::PROJECT_FILE);
    let dst_json = dst.join(project::PROJECT_FILE);
    match (std::fs::metadata(&json), std::fs::metadata(&dst_json)) {
        (Ok(a), Ok(b)) if same(&a, &b) => return Ok(false),
        (_, Ok(b)) if let Some((older, n)) = &keep => {
            // Its date: when the copy being replaced was saved. A move within the backup's own drive: instant.
            let t = b.modified().unwrap_or_else(|_| SystemTime::now());
            let ((y, m, d), (h, min)) = (shapecraft_doc::date::local_date(t), shapecraft_doc::date::local_clock(t));
            let mut to = older.join(format!("{y}-{m:02}-{d:02} {h:02}.{min:02}"));
            for k in 2.. {
                if !to.exists() {
                    break;
                }
                to = older.join(format!("{y}-{m:02}-{d:02} {h:02}.{min:02} ({k})"));
            }
            std::fs::create_dir_all(older)?;
            project::replace(dst, &to)?;
            prune(older, *n);
        }
        _ => {}
    }
    // A save landing midway through is copied again (it rewrote project.json); after three tries the next pass has it.
    for _ in 0..3 {
        let before = std::fs::read(&json)?;
        let stamp = std::fs::metadata(&json)?.modified().ok();
        std::fs::create_dir_all(dst)?;
        let mut files = Vec::new();
        files_of(src, Path::new(""), &mut files);
        for f in &files {
            let (from, to) = (src.join(f), dst.join(f));
            let fresh = match (std::fs::metadata(&from), std::fs::metadata(&to)) {
                (Ok(a), Ok(b)) => same(&a, &b),
                (Err(_), _) => true, // gone since the listing (a save's old bodies)
                _ => false,
            };
            if !fresh {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                match copy_file(&from, &to) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound && !from.exists() => {}
                    r => r?,
                }
            }
        }
        if std::fs::read(&json)? != before {
            continue;
        }
        let tmp = dst.join(format!(".~{}.tmp", project::PROJECT_FILE));
        std::fs::write(&tmp, &before)?;
        if let Some(t) = stamp {
            let _ = std::fs::File::options().write(true).open(&tmp).and_then(|f| f.set_modified(t));
        }
        project::replace(&tmp, &dst_json)?;
        // The copy's files the project no longer has (the other bodies name, removed pictures).
        let mut old = Vec::new();
        files_of(dst, Path::new(""), &mut old);
        for f in old.iter().filter(|f| !files.contains(f) && !src.join(f).exists()) {
            let _ = std::fs::remove_file(dst.join(f));
        }
        return Ok(true);
    }
    Ok(false)
}

/// Keeps the newest `n` dated copies in `older` (their names sort by date).
fn prune(older: &Path, n: usize) {
    let Ok(rd) = std::fs::read_dir(older) else { return };
    let mut all: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join(project::PROJECT_FILE).is_file()).collect();
    all.sort_by_key(|p| std::fs::metadata(p.join(project::PROJECT_FILE)).and_then(|m| m.modified()).ok());
    let extra = all.len().saturating_sub(n);
    for p in &all[..extra] {
        let _ = std::fs::remove_dir_all(p);
    }
}

/// How one backup is doing, for the Sync tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// When it last finished a pass (kept between runs).
    pub done: Option<SystemTime>,
    /// Copying now: projects done, of how many.
    pub busy: Option<(usize, usize)>,
    /// Why the last try failed (the app's folder isn't found, the drive isn't connected…).
    pub problem: Option<String>,
}

enum Msg {
    /// The Work Folder and the backups in order (settings value, folder to back up into when found).
    Setup { work: PathBuf, targets: Targets, state: Option<PathBuf>, plan: Plan },
    /// A project was saved.
    Changed,
    /// Back up now (Save, closing a project, quitting, Back Up Now).
    Now,
}

/// The backup thread and what it reports. One per app (`backups()`).
pub struct Backups {
    tx: Mutex<Option<Sender<Msg>>>,
    status: Arc<Mutex<HashMap<String, Status>>>,
    wake: Wake,
    /// Passes finished, and whether saves are waiting for one (for quitting).
    passes: Arc<std::sync::atomic::AtomicU64>,
    pending: Arc<std::sync::atomic::AtomicBool>,
}

pub fn backups() -> &'static Backups {
    static B: OnceLock<Backups> = OnceLock::new();
    B.get_or_init(|| Backups {
        tx: Mutex::new(None),
        status: Arc::default(),
        wake: Arc::default(),
        passes: Arc::default(),
        pending: Arc::default(),
    })
}

impl Backups {
    fn send(&self, msg: Msg) {
        let mut tx = self.tx.lock().unwrap_or_else(|e| e.into_inner());
        if tx.is_none() {
            let (s, r) = channel();
            let shared =
                Shared { status: self.status.clone(), wake: self.wake.clone(), passes: self.passes.clone(), pending: self.pending.clone() };
            if std::thread::Builder::new().name("backups".into()).spawn(move || run(r, shared)).is_err() {
                return;
            }
            *tx = Some(s);
        }
        if let Some(tx) = tx.as_ref() {
            let _ = tx.send(msg);
        }
    }

    /// What to back up and where (called whenever settings or the found folders change). `state` is the small file
    /// that remembers when each backup last finished.
    pub fn setup(&self, work: PathBuf, targets: Targets, state: Option<PathBuf>, plan: Plan, wake: impl Fn() + Send + 'static) {
        *self.wake.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(wake));
        self.send(Msg::Setup { work, targets, state, plan });
    }

    fn running(&self) -> bool {
        self.tx.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// A project was saved (backed up then, or at the next Save, close or quit, as Preferences says).
    pub fn changed(&self) {
        if self.running() {
            self.pending.store(true, std::sync::atomic::Ordering::SeqCst);
            self.send(Msg::Changed);
        }
    }

    pub fn now(&self) {
        if self.running() {
            self.send(Msg::Now);
        }
    }

    /// Quitting: backs up what was saved since the last pass, waiting at most `limit` (a backup out of reach
    /// shouldn't hold the app open; the next start catches up).
    pub fn flush(&self, limit: Duration) {
        use std::sync::atomic::Ordering;
        if !self.running() || !self.pending.load(Ordering::SeqCst) {
            return;
        }
        let before = self.passes.load(Ordering::SeqCst);
        self.send(Msg::Now);
        let start = Instant::now();
        while self.passes.load(Ordering::SeqCst) == before && start.elapsed() < limit {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn status(&self, target: &str) -> Status {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).get(target).cloned().unwrap_or_default()
    }

    pub fn busy(&self) -> bool {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).values().any(|s| s.busy.is_some())
    }
}

/// When each backup last finished, as saved between runs (seconds since 1970, by settings value).
fn load_state(path: &Path) -> HashMap<String, u64> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// What the thread shares with the app.
struct Shared {
    status: Arc<Mutex<HashMap<String, Status>>>,
    wake: Wake,
    passes: Arc<std::sync::atomic::AtomicU64>,
    pending: Arc<std::sync::atomic::AtomicBool>,
}

fn run(rx: Receiver<Msg>, shared: Shared) {
    use std::sync::atomic::Ordering;
    let Shared { status, wake, passes, pending } = shared;
    let poke = || {
        if let Some(w) = wake.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            w();
        }
    };
    let set = |key: &str, f: &dyn Fn(&mut Status)| {
        f(status.lock().unwrap_or_else(|e| e.into_inner()).entry(key.to_string()).or_default());
        poke();
    };
    let mut work = PathBuf::new();
    let mut targets: Targets = Vec::new();
    let mut state: Option<PathBuf> = None;
    let mut plan = Plan::default();
    // A first pass shortly after starting (catches up with work done while a backup was out of reach).
    let mut due: Option<Instant> = None;
    let mut last = Instant::now();
    // A backup that couldn't be reached is tried again every so often; so is everything when backing up after every change.
    let mut retry = false;
    loop {
        let wait = due.map_or(EVERY.saturating_sub(last.elapsed()), |d| d.saturating_duration_since(Instant::now()));
        match rx.recv_timeout(wait) {
            Ok(Msg::Setup { work: w, targets: t, state: s, plan: pl }) => {
                let first = work.as_os_str().is_empty();
                let new_targets = t.iter().any(|x| !targets.contains(x));
                if let Some(path) = &s {
                    let saved = load_state(path);
                    let mut st = status.lock().unwrap_or_else(|e| e.into_inner());
                    for (k, secs) in saved {
                        let e = st.entry(k).or_default();
                        if e.done.is_none() {
                            e.done = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs));
                        }
                    }
                }
                let changed = w != work || new_targets || pl.per_computer != plan.per_computer;
                (work, targets, state, plan) = (w, t, s, pl);
                if first || changed {
                    due = Some(Instant::now() + Duration::from_secs(5));
                }
            }
            Ok(Msg::Changed) if plan.after_change => due = Some(Instant::now() + QUIET),
            Ok(Msg::Changed) => {}
            Ok(Msg::Now) => due = Some(Instant::now()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let timed = due.is_none();
                due = None;
                last = Instant::now();
                if work.as_os_str().is_empty() || !work.is_dir() || (timed && !retry && !plan.after_change) {
                    continue;
                }
                pending.store(false, Ordering::SeqCst);
                retry = false;
                // In the order chosen: the first backup is brought up to date first.
                for (key, root) in &targets {
                    let Some(root) = root else {
                        set(key, &|s| s.problem = Some("Its app isn't signed in on this computer.".into()));
                        retry = true;
                        continue;
                    };
                    if !root.is_dir() {
                        set(key, &|s| s.problem = Some("Can't reach it now. ShapeCraft tries again every 10 minutes.".into()));
                        retry = true;
                        continue;
                    }
                    set(key, &|s| s.busy = Some((0, 0)));
                    let r = mirror(&work, &dest(root, plan.per_computer), plan.versions, &|k, n| {
                        status.lock().unwrap_or_else(|e| e.into_inner()).entry(key.clone()).or_default().busy = Some((k, n));
                    });
                    poke();
                    match r {
                        Ok(_) => {
                            let now = SystemTime::now();
                            set(key, &|s| {
                                s.busy = None;
                                s.problem = None;
                                s.done = Some(now);
                            });
                            if let Some(path) = &state {
                                let mut saved = load_state(path);
                                saved.insert(key.clone(), now.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()));
                                if let Ok(bytes) = serde_json::to_vec_pretty(&saved) {
                                    let _ = std::fs::write(path, bytes);
                                }
                            }
                        }
                        Err(e) => {
                            let msg = format!("Couldn't back up: {e}");
                            set(key, &|s| {
                                s.busy = None;
                                s.problem = Some(msg.clone());
                            });
                            retry = true;
                        }
                    }
                }
                passes.fetch_add(1, Ordering::SeqCst);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// A change of Work Folder asked for in Preferences ▸ Sync (the app carries it out: it has the open project).
pub struct Switch {
    pub to: PathBuf,
    /// Move the projects from the old folder too.
    pub bring: bool,
}

static SWITCH: Mutex<Option<Switch>> = Mutex::new(None);
static WORK: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The Work Folder in use (the app sets it every frame; Preferences shows it).
pub fn set_work(dir: &Path) {
    let mut w = WORK.lock().unwrap_or_else(|e| e.into_inner());
    if w.as_deref() != Some(dir) {
        *w = Some(dir.to_path_buf());
    }
}

pub fn work() -> Option<PathBuf> {
    WORK.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
static NOTE: Mutex<String> = Mutex::new(String::new());

pub fn request_switch(to: PathBuf, bring: bool) {
    *SWITCH.lock().unwrap_or_else(|e| e.into_inner()) = Some(Switch { to, bring });
}

pub fn take_switch() -> Option<Switch> {
    SWITCH.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// How the last change of Work Folder went, for the Sync tab.
pub fn set_note(text: String) {
    *NOTE.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

pub fn note() -> String {
    NOTE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// "just now", "5 min ago", "3 h ago", "2 days ago".
pub fn ago(t: SystemTime) -> String {
    let s = SystemTime::now().duration_since(t).map_or(0, |d| d.as_secs());
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        86400..172800 => "yesterday".into(),
        _ => format!("{} days ago", s / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sc-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn targets_round_trip_and_name_themselves() {
        let f = Target::parse("folder:/Volumes/NAS/Backups");
        assert_eq!(f, Target::Folder("/Volumes/NAS/Backups".into()));
        assert_eq!(Target::parse(&f.setting()), f);
        assert_eq!(f.label(&Found::default()), "Backups");
        assert_eq!(Target::parse("onedrive:Contoso").label(&Found::default()), "OneDrive (Contoso)");
        assert_eq!(Target::parse("dropbox").root(&Found::default()), None);
        assert_eq!(dest(Path::new("/b"), false), Path::new("/b").join(BACKUPS_DIR));
        assert_eq!(dest(Path::new("/b"), true), Path::new("/b").join(BACKUPS_DIR).join(computer_name()));
        assert_eq!(clean_name("Sam's: PC/2"), "Sam's- PC-2");
        assert_eq!(clean_name("  "), "This Computer");
    }

    #[test]
    fn mirror_copies_changes_and_keeps_the_copy_whole() {
        let d = tmp("mirror");
        let (work, dest) = (d.join("work"), d.join("backup"));
        put(&work.join("Bracket/project.json"), r#"{"bodies_file":"bodies.step"}"#);
        put(&work.join("Bracket/bodies.step"), "one");
        put(&work.join("Bracket/images/7.png"), "png");
        put(&work.join("Jobs/Lid/project.json"), "{}");
        std::fs::create_dir_all(work.join("Empty")).unwrap();
        put(&work.join(".Recently Deleted/Old/project.json"), "{}");
        let pass = mirror(&work, &dest, 0, &|_, _| {}).unwrap();
        assert_eq!(pass, Pass { projects: 2, copied: 2 });
        assert_eq!(std::fs::read_to_string(dest.join("Bracket/bodies.step")).unwrap(), "one");
        assert!(dest.join("Bracket/images/7.png").is_file() && dest.join("Jobs/Lid/project.json").is_file());
        assert!(dest.join("Empty").is_dir() && !dest.join(".Recently Deleted").exists());
        // Nothing changed: nothing copied.
        assert_eq!(mirror(&work, &dest, 0, &|_, _| {}).unwrap().copied, 0);
        // A save: bodies under the other name, project.json last, the old name gone.
        std::thread::sleep(Duration::from_millis(2100));
        put(&work.join("Bracket/bodies-next.step"), "two");
        put(&work.join("Bracket/project.json"), r#"{"bodies_file":"bodies-next.step","x":1}"#);
        std::fs::remove_file(work.join("Bracket/bodies.step")).unwrap();
        assert_eq!(mirror(&work, &dest, 0, &|_, _| {}).unwrap().copied, 1);
        assert_eq!(std::fs::read_to_string(dest.join("Bracket/bodies-next.step")).unwrap(), "two");
        assert!(!dest.join("Bracket/bodies.step").exists());
        assert_eq!(std::fs::read(dest.join("Bracket/project.json")).unwrap(), std::fs::read(work.join("Bracket/project.json")).unwrap());
        // Deleted here, kept in the backup.
        std::fs::remove_dir_all(work.join("Jobs")).unwrap();
        mirror(&work, &dest, 0, &|_, _| {}).unwrap();
        assert!(dest.join("Jobs/Lid/project.json").is_file());
        // No temporaries left behind.
        let mut all = Vec::new();
        files_of(&dest.join("Bracket"), Path::new(""), &mut all);
        assert!(std::fs::read_dir(dest.join("Bracket")).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(".~")));
        // Kept versions: each replaced copy goes to Older Versions, dated; only the newest two stay.
        let dest2 = d.join("backup2");
        for k in 0..4 {
            std::thread::sleep(Duration::from_millis(1100));
            put(&work.join("Bracket/project.json"), &format!(r#"{{"bodies_file":"bodies-next.step","k":"{}"}}"#, "x".repeat(k + 1)));
            mirror(&work, &dest2, 2, &|_, _| {}).unwrap();
        }
        let older: Vec<_> = std::fs::read_dir(dest2.join(OLDER).join("Bracket")).unwrap().flatten().collect();
        assert_eq!(older.len(), 2);
        assert!(std::fs::read_to_string(dest2.join("Bracket/project.json")).unwrap().contains(r#""k":"xxxx""#));
        assert!(older.iter().all(|e| e.path().join("bodies-next.step").is_file()));
        // A backup inside the Work Folder is refused.
        assert!(mirror(&work, &work.join("Backups"), 0, &|_, _| {}).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A project shared between computers through a cloud app: what a half-arrived save, two saves at once, and
    /// another computer having it open look like here.
    #[test]
    fn shared_work_folder_safety() {
        use shapecraft_kernel::Kernel;
        let d = tmp("shared");
        let k = shapecraft_kernel_occt::OcctKernel::new();
        let mut doc = shapecraft_doc::Document::new("Box");
        let shape = k.make_box(glam::DVec3::ZERO, glam::DVec3::splat(10.0)).unwrap();
        let id = doc.alloc_id();
        doc.bodies.push(shapecraft_doc::Body {
            id,
            name: "Body 1".into(),
            shape,
            mesh: Arc::new(Default::default()),
            visible: true,
            appearance: None,
            face_colors: Vec::new(),
        });
        let dir = d.join("Box");
        project::save(&doc, &k, &dir).unwrap();
        let old = std::fs::read(dir.join("bodies.step")).unwrap();
        // Two saves later the same name holds new shapes; the sync app has brought project.json but not them yet.
        let big = k.make_box(glam::DVec3::ZERO, glam::DVec3::splat(30.0)).unwrap();
        doc.bodies[0].shape = big;
        project::save(&doc, &k, &dir).unwrap();
        project::save(&doc, &k, &dir).unwrap();
        assert!(project::load(&k, &dir).is_ok());
        std::fs::write(dir.join("bodies.step"), &old).unwrap();
        assert!(matches!(project::load(&k, &dir), Err(project::ProjectError::StillSyncing)));
        // A missing file waits too, rather than rebuilding (which would save over the other computer's work).
        std::fs::remove_file(dir.join("bodies.step")).unwrap();
        assert!(matches!(project::load(&k, &dir), Err(project::ProjectError::StillSyncing)));
        project::save(&doc, &k, &dir).unwrap();
        // No temporaries a sync app would upload: they're hidden while being written, and gone after.
        assert!(std::fs::read_dir(&dir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")));

        // Saved on two computers at once: Dropbox keeps the other as "project (… conflicted copy …).json".
        let theirs = std::fs::read(dir.join("project.json")).unwrap();
        let other = dir.join("project (Workshop PC's conflicted copy 2026-11-06).json");
        std::fs::write(&other, &theirs).unwrap();
        assert_eq!(project::list(&d)[0].versions, 1);
        let split = project::split_version(&dir, &other).unwrap();
        assert_eq!(split.file_name().unwrap(), "Box - Other Version");
        assert!(!other.exists() && project::other_versions(&dir).is_empty());
        let copy = project::load(&k, &split).unwrap();
        assert_eq!((copy.bodies.len(), copy.name.as_str()), (1, "Box - Other Version"));

        // Open on another computer: seen until its mark is three minutes old.
        project::mark_open(&dir, "Workshop PC");
        let seen = project::open_elsewhere(&dir, "Studio Mac", Duration::from_secs(180));
        assert_eq!(seen.map(|(who, _)| who), Some("Workshop PC".to_string()));
        assert!(project::open_elsewhere(&dir, "Workshop PC", Duration::from_secs(180)).is_none(), "not our own mark");
        project::mark_closed(&dir, "Workshop PC");
        assert!(project::open_elsewhere(&dir, "Studio Mac", Duration::from_secs(180)).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
