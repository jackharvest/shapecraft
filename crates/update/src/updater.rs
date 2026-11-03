//! The update flow, shared by every window: automatic checks (a few seconds after starting, then once a day), the
//! download in the background, and the ready update waiting for ShapeCraft to close.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::install::{Install, this_install};
use crate::net::{self, NetError};
use crate::release::{self, Release, Version};
use crate::stage;

/// The first automatic check waits this long after starting, so it never competes with opening a project.
const FIRST_CHECK: Duration = Duration::from_secs(4);
/// While ShapeCraft stays open, automatic checks run once a day.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
/// After a failed automatic check (offline, rate limited), try again after an hour.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(3600);
/// "Check Now" can't be pressed more often than this (GitHub allows 60 anonymous requests an hour).
const MANUAL_COOLDOWN: Duration = Duration::from_secs(10);

/// The user's choices, from Preferences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub auto_check: bool,
    pub auto_download: bool,
    /// A version the user chose to skip (`0.4.1`), or empty.
    pub skipped: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Idle,
    Checking,
    /// A check found nothing newer.
    UpToDate,
    /// A newer version exists; downloading waits for the user (automatic downloads are off).
    Available,
    Downloading {
        done: u64,
        total: Option<u64>,
    },
    /// Downloaded, checked and unpacked: installs when ShapeCraft closes.
    Ready,
    /// A newer version exists, but something else installs it here (the name), or it can't install itself (why).
    Elsewhere(String),
    Failed(String),
}

/// What the window shows.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    /// The version this copy reports.
    pub current: String,
    /// The newer version on offer, with its notes and page.
    pub offered: Option<(String, String, String)>,
    /// The last check was asked for (Check for Updates…), so its outcome is worth saying even when nothing is new.
    pub manual: bool,
    pub install: Install,
    /// When the last check finished, in seconds since then.
    pub checked_ago: Option<u64>,
}

struct Inner {
    phase: Phase,
    offered: Option<Release>,
    staged: Option<PathBuf>,
    manual: bool,
    busy: bool,
    last_check: Option<Instant>,
    last_failed: bool,
    started: Instant,
    prefs: Option<Prefs>,
}

pub struct Updater {
    inner: Mutex<Inner>,
    install: OnceLock<Install>,
    stop: Arc<AtomicBool>,
    waker: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl Updater {
    /// The one updater of this process.
    pub fn global() -> &'static Updater {
        static UPDATER: OnceLock<Updater> = OnceLock::new();
        UPDATER.get_or_init(|| Updater {
            inner: Mutex::new(Inner {
                phase: Phase::Idle,
                offered: None,
                staged: None,
                manual: false,
                busy: false,
                last_check: None,
                last_failed: false,
                started: Instant::now(),
                prefs: None,
            }),
            install: OnceLock::new(),
            stop: Arc::new(AtomicBool::new(false)),
            waker: Mutex::new(None),
        })
    }

    /// Called when something changes, from whichever thread (the app asks for a repaint).
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut w) = self.waker.lock() {
            *w = Some(Box::new(wake));
        }
    }

    fn wake(&self) {
        if let Ok(w) = self.waker.lock()
            && let Some(w) = w.as_ref()
        {
            w();
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> Option<R> {
        self.inner.lock().ok().map(|mut i| f(&mut i))
    }

    pub fn install(&self) -> &Install {
        self.install.get_or_init(this_install)
    }

    /// Whether this copy checks at all: release builds do; development builds only with `SHAPECRAFT_UPDATE_CHECK=1`.
    pub fn checks(&self) -> bool {
        match std::env::var("SHAPECRAFT_UPDATE_CHECK").ok().as_deref() {
            Some("0") => false,
            Some("1") => true,
            _ => *self.install() != Install::Dev,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let install = self.install().clone();
        self.with(|i| Snapshot {
            phase: i.phase.clone(),
            current: crate::current_text(),
            offered: i.offered.as_ref().map(|r| (r.version_text(), r.notes.clone(), r.page.clone())),
            manual: i.manual,
            install: install.clone(),
            checked_ago: i.last_check.map(|t| t.elapsed().as_secs()),
        })
        .unwrap_or(Snapshot {
            phase: Phase::Idle,
            current: crate::current_text(),
            offered: None,
            manual: false,
            install,
            checked_ago: None,
        })
    }

    /// Every frame: keeps the preferences and starts an automatic check when one is due.
    pub fn tick(&self, prefs: &Prefs) {
        let checks = self.checks();
        let due = self
            .with(|i| {
                i.prefs = Some(prefs.clone());
                if !checks || !prefs.auto_check || i.busy || matches!(i.phase, Phase::Ready) {
                    return false;
                }
                match i.last_check {
                    None => i.started.elapsed() >= FIRST_CHECK,
                    Some(t) => t.elapsed() >= if i.last_failed { RETRY_AFTER_FAILURE } else { CHECK_INTERVAL },
                }
            })
            .unwrap_or(false);
        if due {
            self.start_check(false);
        }
    }

    /// Help ▸ Check for Updates… and Preferences' Check Now. Development builds check too when asked.
    pub fn check_now(&self) {
        let cooling =
            self.with(|i| i.busy || i.last_check.is_some_and(|t| t.elapsed() < MANUAL_COOLDOWN && !i.last_failed)).unwrap_or(true);
        if cooling {
            // Say the last answer again rather than ask GitHub twice in a row.
            self.with(|i| i.manual = true);
            self.wake();
            return;
        }
        self.start_check(true);
    }

    /// Hides a manual check's "up to date" or failure note.
    pub fn dismiss(&self) {
        self.with(|i| {
            i.manual = false;
            if matches!(i.phase, Phase::UpToDate | Phase::Failed(_)) {
                i.phase = Phase::Idle;
            }
        });
    }

    /// Skip this version: nothing is offered again until a newer one.
    pub fn skip(&self) {
        self.with(|i| {
            i.offered = None;
            i.staged = None;
            i.phase = Phase::Idle;
        });
    }

    /// Downloads the offered version now (Download, with automatic downloads off).
    pub fn download(&self) {
        let start = self
            .with(|i| {
                if i.busy || i.offered.is_none() || !matches!(i.phase, Phase::Available | Phase::Failed(_)) {
                    return None;
                }
                i.busy = true;
                i.offered.clone()
            })
            .flatten();
        if let Some(release) = start {
            let me = Self::global();
            std::thread::spawn(move || me.fetch(&release));
        }
    }

    fn start_check(&self, manual: bool) {
        let go = self
            .with(|i| {
                if i.busy {
                    return false;
                }
                i.busy = true;
                i.manual = manual;
                if !matches!(i.phase, Phase::Ready) {
                    i.phase = Phase::Checking;
                }
                true
            })
            .unwrap_or(false);
        if go {
            self.wake();
            let me = Self::global();
            std::thread::spawn(move || me.run_check(manual));
        }
    }

    fn run_check(&self, manual: bool) {
        let prefs =
            self.with(|i| i.prefs.clone()).flatten().unwrap_or(Prefs { auto_check: true, auto_download: true, skipped: String::new() });
        let skipped = if manual { None } else { release::parse_version(&prefs.skipped) };
        let answer = net::get_text(crate::RELEASES_API, true).and_then(|t| release::parse_list(&t).map_err(NetError::Failed));
        let offer = match answer {
            Err(e) => {
                self.with(|i| {
                    i.busy = false;
                    i.last_check = Some(Instant::now());
                    i.last_failed = true;
                    if !matches!(i.phase, Phase::Ready) {
                        i.phase = Phase::Failed(e.to_string());
                    }
                });
                self.wake();
                return;
            }
            Ok(list) => release::offer(&list, crate::current(), skipped),
        };
        let install = self.install().clone();
        let Some(release) = offer else {
            self.with(|i| {
                i.busy = false;
                i.last_check = Some(Instant::now());
                i.last_failed = false;
                i.offered = None;
                i.staged = None;
                i.phase = Phase::UpToDate;
            });
            self.wake();
            return;
        };
        let staged = stage::updates_dir().and_then(|d| stage::already_staged(&d, &release));
        let fetch = self
            .with(|i| {
                i.last_check = Some(Instant::now());
                i.last_failed = false;
                let same = i.offered.as_ref().is_some_and(|o| o.tag == release.tag) && i.staged.is_some();
                i.offered = Some(release.clone());
                if same {
                    i.phase = Phase::Ready;
                    i.busy = false;
                    return false;
                }
                match (&install, &staged) {
                    (Install::Managed(name), _) => {
                        i.phase = Phase::Elsewhere(format!("Update it in {name}."));
                        false
                    }
                    (Install::Elsewhere(why), _) => {
                        i.phase = Phase::Elsewhere(why.clone());
                        false
                    }
                    (Install::Dev, _) => {
                        i.phase = Phase::Elsewhere("This is a development build: pull and rebuild to update.".into());
                        false
                    }
                    (_, Some(path)) => {
                        i.staged = Some(path.clone());
                        i.phase = Phase::Ready;
                        false
                    }
                    _ if prefs.auto_download || manual => true,
                    _ => {
                        i.phase = Phase::Available;
                        false
                    }
                }
            })
            .unwrap_or(false);
        if fetch {
            self.fetch(&release);
        } else {
            self.with(|i| i.busy = false);
            self.wake();
        }
    }

    /// Downloads and stages `release` on this thread.
    fn fetch(&self, release: &Release) {
        self.with(|i| i.phase = Phase::Downloading { done: 0, total: None });
        self.wake();
        let install = self.install().clone();
        let result = match stage::updates_dir() {
            None => Err("there's no cache folder to download into".to_string()),
            Some(dir) => stage::stage(&install, release, &dir, &self.stop, &|done, total| {
                self.with(|i| i.phase = Phase::Downloading { done, total });
                self.wake();
            }),
        };
        self.with(|i| {
            i.busy = false;
            match result {
                Ok(path) => {
                    i.staged = Some(path);
                    i.phase = Phase::Ready;
                }
                Err(e) => i.phase = Phase::Failed(format!("Couldn't download ShapeCraft {}: {e}", release.version_text())),
            }
        });
        self.wake();
    }

    /// An update is downloaded and waiting.
    pub fn ready(&self) -> bool {
        self.with(|i| matches!(i.phase, Phase::Ready) && i.staged.is_some()).unwrap_or(false)
    }

    /// Starts the helper that installs the ready update once ShapeCraft closes (and starts it again with `args` when
    /// `restart`). The caller closes ShapeCraft straight after.
    pub fn apply(&self, restart: bool, args: &[OsString]) -> Result<(), String> {
        let (staged, release) = self.with(|i| (i.staged.clone(), i.offered.clone())).ok_or("the updater is unavailable")?;
        let (Some(staged), Some(release)) = (staged, release) else { return Err("no update is ready".into()) };
        let dir = stage::updates_dir().ok_or("there's no cache folder")?;
        crate::apply::launch(self.install(), &staged, &dir.join(release.version_text()), &dir.join("update.log"), restart, args)
    }
}

/// The version of the offered release, if any.
pub fn offered_version(s: &Snapshot) -> Option<Version> {
    s.offered.as_ref().and_then(|(v, _, _)| release::parse_version(v))
}
