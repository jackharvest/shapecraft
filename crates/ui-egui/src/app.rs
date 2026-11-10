//! The application: projects screen ⇄ workspace, autosave, files, and the control channel.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use shapecraft_doc::project;
use shapecraft_engine::Session;
use shapecraft_kernel::Kernel;
use shapecraft_render::{SceneColors, SceneInput, StandardView};

use crate::control::{Request, Server};
use crate::landing::{Landing, LandingAction};
use crate::menubar::{AppAction, MenuAction, MenuState, Menus, WindowMenu};
use crate::theme;
use crate::workspace::export::{ExportDialog, Format};
use crate::workspace::{Tool, Workspace};

#[derive(Clone)]
pub struct Options {
    pub projects_root: PathBuf,
    pub control_port: Option<u16>,
    /// Open this project folder straight away.
    pub open: Option<PathBuf>,
    /// Start in a new, empty project (handy for scripted snapshots).
    pub new_project: bool,
    /// Install the native menu bar (the desktop app; not offscreen snapshots).
    pub native_menu: bool,
    /// Where preferences are read and saved (none: defaults, never saved).
    pub settings_file: Option<PathBuf>,
    /// The tablet's copy of the app (see `tablet`): menus in the window, files through the page, no update checks.
    pub hosted: bool,
}

impl Options {
    /// Where projects live: $SHAPECRAFT_PROJECTS, else ~/Documents/ShapeCraft. No file is touched here: the folder
    /// is made, moved over from its old name and first read off the UI thread (see `folder_access`).
    pub fn default_root() -> PathBuf {
        // SHAYPER3D_PROJECTS is the name from before the rename.
        if let Some(p) = std::env::var_os("SHAPECRAFT_PROJECTS").or_else(|| std::env::var_os("SHAYPER3D_PROJECTS")) {
            return PathBuf::from(p);
        }
        Self::documents().join("ShapeCraft")
    }

    /// The projects folder from before the rename (~/Documents/Shayper3D), moved over once.
    pub fn legacy_root() -> Option<PathBuf> {
        let named = std::env::var_os("SHAPECRAFT_PROJECTS").or_else(|| std::env::var_os("SHAYPER3D_PROJECTS")).is_some();
        (!named).then(|| Self::documents().join("Shayper3D"))
    }

    fn documents() -> PathBuf {
        if let Some(dir) = DOCUMENTS.get() {
            return dir.clone();
        }
        dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."))
    }

    /// Where "Documents" is when the system has none to find (Android: the app's own storage). Set once, before the
    /// app starts; later calls are ignored.
    pub fn set_documents(dir: PathBuf) {
        let _ = DOCUMENTS.set(dir);
    }
}

static DOCUMENTS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// A project's name for messages: its folder name (the path is long and says little).
fn project_name(dir: &Path) -> String {
    dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Set when the graphics driver reports the device lost, from whichever thread noticed.
static GPU_LOST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Tells the app started by `restart_without_gpu` why it was.
const RESTARTED_ENV: &str = "SHAPECRAFT_GPU_RESTARTED";

/// What the tablet's copy of the app asks of the page (see `tablet`).
pub(crate) enum TabletRequest {
    /// Show a file picker; the file comes back through [`ShapeCraftApp::tablet_file_arrived`] with this id.
    Pick { id: u64, label: String, accept: Vec<String> },
    /// Files written into this folder go to the page to download.
    Saved(PathBuf),
}

/// What a picked file is for, once the page sends it.
#[derive(Clone, Copy, Debug)]
enum Wanted {
    Import,
    Image,
    Decal,
    Material,
    Environment,
}

/// On a tablet, files come and go through the page instead of the system's dialogs.
struct TabletFiles {
    requests: Vec<TabletRequest>,
    picks: Vec<(u64, Wanted)>,
    next: u64,
    /// Where files saved for the page are written.
    dir: PathBuf,
}

/// A file to open: the system's dialog, or on a tablet the page's picker (the file arrives later, see `Wanted`).
fn open_path(tablet: &mut Option<TabletFiles>, wanted: Wanted, label: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    if let Some(t) = tablet {
        t.next += 1;
        let mut accept: Vec<String> = Vec::new();
        for e in filters.iter().flat_map(|(_, exts)| exts.iter()) {
            if !accept.iter().any(|a| a == e) {
                accept.push((*e).to_string());
            }
        }
        t.requests.push(TabletRequest::Pick { id: t.next, label: label.to_string(), accept });
        t.picks.push((t.next, wanted));
        return None;
    }
    filters.iter().fold(crate::dialogs::FileDialog::new(), |d, (name, exts)| d.add_filter(*name, exts)).pick_file()
}

/// A new folder whose files go to the tablet's page once this frame has written them.
fn tablet_folder(t: &mut TabletFiles) -> Option<PathBuf> {
    t.next += 1;
    let dir = t.dir.join(format!("save-{}", t.next));
    std::fs::create_dir_all(&dir).ok()?;
    t.requests.push(TabletRequest::Saved(dir.clone()));
    Some(dir)
}

/// Where to save a file: the system's dialog, or on a tablet a scratch file the page then downloads.
fn save_path(tablet: &mut Option<TabletFiles>, name: &str, filter: (&str, &[&str])) -> Option<PathBuf> {
    match tablet {
        Some(t) => Some(tablet_folder(t)?.join(crate::tablet::safe_file_name(name))),
        None => crate::dialogs::FileDialog::new().set_file_name(name).add_filter(filter.0, filter.1).save_file(),
    }
}

/// A folder to export several files into (the page downloads each, on a tablet).
fn save_folder(tablet: &mut Option<TabletFiles>) -> Option<PathBuf> {
    match tablet {
        Some(t) => tablet_folder(t),
        None => crate::dialogs::FileDialog::new().set_title("Choose a folder to export to").pick_folder(),
    }
}

/// A control request's answer, for whichever side gets to it first.
type WaitingReply = Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Result<Value, String>>>>>;

pub struct ShapeCraftApp {
    /// A scrap of GPU memory written every frame to learn that the device is gone before painting does (see `gpu_gone`).
    gpu_probe: Option<wgpu::Buffer>,
    /// The window's own title (hidden behind ours, but macOS lists it in the Window menu, Mission Control and the Dock).
    window_title: String,
    kernel: Arc<dyn Kernel>,
    /// The first touch of the projects folder (made, moved over, read once) and how it went.
    folder: crate::folder_access::FolderAccess,
    landing: Landing,
    /// Where a new project's folder goes when it's first saved.
    new_parent: Option<PathBuf>,
    ws: Option<Box<Workspace>>,
    control: Option<Server>,
    /// When the first unsaved change was made.
    last_change: Option<std::time::Instant>,
    saved_generation: u64,
    /// The window had the keyboard as of the last look.
    was_focused: bool,
    /// A screenshot was asked for and isn't back yet: who to answer, and since when.
    capture: Option<(WaitingReply, std::time::Instant)>,
    styled: bool,
    /// Time between frames in ms since `ui.frames` last read them, and when the last frame began.
    frame_gaps: Vec<f32>,
    /// Time spent in `frame`, the app's own work, and the 3D view's share of it, in ms. The rest of a gap is painting and waiting.
    frame_work: Vec<(f32, f32)>,
    last_frame: Option<std::time::Instant>,
    /// The window in points, and pixels per point, as of the last frame.
    view_size: (egui::Vec2, f32),
    /// The window has been checked against the screen.
    fitted: bool,
    menubar: Option<Menus>,
    settings: crate::settings::Settings,
    settings_file: Option<PathBuf>,
    /// The first-run device question, asked again (control `ui.onboarding`).
    ask_device: bool,
    /// The tablet's first-run question: the device picked, its controls showing until Start Modeling.
    tablet_choice: Option<usize>,
    /// Preferences opened on the projects screen: its tab, and the Navigation Presets modal over it.
    home_prefs: Option<usize>,
    home_nav: Option<&'static str>,
    /// The first run's second question: the navigation style, preselected from the device's answer.
    welcome_nav: Option<&'static str>,
    /// For textures made outside a frame (a tutorial step's picture).
    egui: egui::Context,
    /// The desktop app checks for updates and shows them (snapshots and tests don't).
    updating: bool,
    updates: crate::updates::UpdateUi,
    /// The tablet's copy of the app (no window, no system dialogs).
    hosted: bool,
    tablet_files: Option<TabletFiles>,
    /// The desktop app's options, for starting the tablet's copy (Preferences ▸ Tablet); None in the copy itself.
    tablet_opts: Option<Options>,
    /// The tablet host as last set up: port, open to the network, and the host (None if it couldn't start).
    tablet: Option<(u16, bool, String, Option<crate::tablet::Host>)>,
    /// What the backup thread was last told: the Work Folder and each backup with its folder now.
    backup_sent: Option<(PathBuf, crate::backup::Targets, crate::backup::Plan)>,
    /// This run was given its own projects folder (`--projects`, $SHAPECRAFT_PROJECTS): it isn't the Work Folder,
    /// so it doesn't back up into the Work Folder's backups, where same-named projects would replace each other.
    own_folder: bool,
    /// The open project's "open on this computer" mark (for other computers sharing the folder), and when it was renewed.
    marked: Option<(PathBuf, std::time::Instant)>,
    /// Asking before opening a project another computer has open: it, that computer, and how long ago it was seen.
    ask_open: Option<(PathBuf, String, std::time::Duration)>,
    /// The first-run setup (look, Work Folder, backups), while it's showing.
    setup: Option<crate::setup::Setup>,
}

impl ShapeCraftApp {
    pub fn new(kernel: Arc<dyn Kernel>, opts: Options, ctx: &egui::Context) -> Self {
        let tablet_opts = (opts.native_menu && !opts.hosted).then(|| opts.clone());
        let tablet_files = opts.hosted.then(|| TabletFiles {
            requests: Vec::new(),
            picks: Vec::new(),
            next: 0,
            dir: std::env::temp_dir().join(format!("shapecraft-tablet-files-{}", std::process::id())),
        });
        let settings = opts.settings_file.as_deref().map(crate::settings::Settings::load).unwrap_or_default();
        // Preferences ▸ Sync ▸ Work Folder, unless this run was given its own ($SHAPECRAFT_PROJECTS, tests).
        let named = opts.projects_root != Options::default_root() || Options::legacy_root().is_none();
        let root = if named || settings.work_folder.is_empty() { opts.projects_root.clone() } else { PathBuf::from(&settings.work_folder) };
        // The desktop app opens its window before touching the projects folder (macOS may ask first); snapshots and
        // tests settle it right here.
        let folder = if opts.native_menu {
            let ctx = ctx.clone();
            let legacy = if root == Options::default_root() { Options::legacy_root() } else { None };
            crate::folder_access::FolderAccess::start(root.clone(), legacy, move || ctx.request_repaint())
        } else {
            let _ = std::fs::create_dir_all(&root);
            crate::folder_access::FolderAccess::ready(root.clone())
        };
        let control = opts.control_port.and_then(|port| {
            let ctx = ctx.clone();
            match Server::start(port, move || ctx.request_repaint()) {
                Ok(s) => {
                    eprintln!("control channel on 127.0.0.1:{}", s.port);
                    Some(s)
                }
                Err(e) => {
                    eprintln!("control channel: {e}");
                    None
                }
            }
        });
        let menubar = if opts.hosted {
            Some(Menus::Window(WindowMenu::default()))
        } else if opts.native_menu {
            let ctx = ctx.clone();
            Menus::install(move || ctx.request_repaint())
        } else {
            None
        };
        // Backing up to a cloud app: find its folder now (off this thread). Otherwise nothing looks until Preferences ▸
        // Sync opens, since the first look may make macOS ask about access to them.
        if opts.settings_file.is_some() && settings.backups.iter().any(|b| crate::backup::Target::parse(b).is_cloud()) {
            let ctx = ctx.clone();
            crate::cloud::detector().refresh(move || ctx.request_repaint());
        }
        let updating = opts.native_menu;
        if updating {
            let ctx = ctx.clone();
            shapecraft_update::Updater::global().set_waker(move || ctx.request_repaint());
        }
        // Tessellation Quality takes effect from the start (Preferences says to restart for it).
        shapecraft_doc::project::set_deflection(match settings.tessellation_quality.as_str() {
            "Medium" => 0.05,
            "Low" => 0.12,
            _ => shapecraft_doc::project::DEFLECTION,
        });
        let mut app = Self {
            gpu_probe: None,
            window_title: "ShapeCraft".to_string(),
            menubar,
            settings,
            settings_file: opts.settings_file,
            ask_device: false,
            tablet_choice: None,
            home_prefs: None,
            home_nav: None,
            welcome_nav: None,
            egui: ctx.clone(),
            updating,
            updates: Default::default(),
            kernel,
            landing: Landing::new(root),
            backup_sent: None,
            own_folder: named,
            marked: None,
            ask_open: None,
            setup: None,
            folder,
            new_parent: None,
            ws: None,
            control,
            last_change: None,
            saved_generation: 0,
            was_focused: true,
            capture: None,
            styled: false,
            frame_gaps: Vec::new(),
            frame_work: Vec::new(),
            last_frame: None,
            view_size: (egui::Vec2::ZERO, 1.0),
            fitted: false,
            hosted: opts.hosted,
            tablet_files,
            tablet_opts,
            tablet: None,
        };
        if let Some(dir) = opts.open {
            app.open(dir);
        } else if opts.new_project {
            app.new_project();
        }
        app
    }

    /// What the tablet's copy asks of its page since the last call (pickers, downloads).
    pub(crate) fn take_tablet_requests(&mut self) -> Vec<TabletRequest> {
        self.tablet_files.as_mut().map(|t| std::mem::take(&mut t.requests)).unwrap_or_default()
    }

    /// A file the tablet's page sent for picker `id`: carries on with what asked for it.
    pub(crate) fn tablet_file_arrived(&mut self, id: u64, path: PathBuf) {
        let Some(t) = self.tablet_files.as_mut() else { return };
        let Some(i) = t.picks.iter().position(|(p, _)| *p == id) else { return };
        let (_, wanted) = t.picks.remove(i);
        match wanted {
            Wanted::Import => self.import_picked(path),
            Wanted::Image => self.image_picked(path),
            Wanted::Decal => {
                if let Some(ws) = self.ws.as_deref_mut() {
                    let _ = ws.run("import_decal", json!({ "path": path }));
                }
            }
            Wanted::Material => {
                if let Some(ws) = self.ws.as_deref_mut() {
                    let _ = ws.run("import_material", json!({ "path": path }));
                }
            }
            Wanted::Environment => {
                if let Some(ws) = self.ws.as_deref_mut() {
                    let _ = ws.run("import_environment", json!({ "path": path }));
                }
            }
        }
    }

    /// Starts, restarts or stops the tablet host as Preferences ▸ Tablet says (the desktop app only).
    fn sync_tablet(&mut self) {
        let Some(base) = self.tablet_opts.clone() else { return };
        let s = self.settings_now();
        if s.tablet_serve && s.tablet_key.is_empty() {
            s.tablet_key = crate::tablet::new_token();
        }
        let want = s.tablet_serve.then(|| (s.tablet_port, s.tablet_network, s.tablet_key.clone()));
        if want == self.tablet.as_ref().map(|(p, n, k, _)| (*p, *n, k.clone())) {
            return;
        }
        // Stopping first frees the port for a restart on the same one.
        self.tablet = None;
        let Some((port, network, key)) = want else { return };
        // The copy reads the same preferences file, key and all.
        let s = self.settings_now().clone();
        self.settings = s;
        self.save_settings();
        let ip = if network { std::net::Ipv4Addr::UNSPECIFIED } else { std::net::Ipv4Addr::LOCALHOST };
        let host = match crate::tablet::Host::start((ip, port).into(), key.clone(), self.kernel.clone(), base) {
            Ok(h) => Some(h),
            Err(e) => {
                self.toast(format!("Couldn't serve to tablets on port {port}: {e}"));
                None
            }
        };
        self.tablet = Some((port, network, key, host));
    }

    pub fn workspace(&mut self) -> Option<&mut Workspace> {
        self.ws.as_deref_mut()
    }

    fn new_project_in(&mut self, dir: PathBuf) {
        self.new_project();
        self.new_parent = Some(dir);
    }

    fn new_project(&mut self) {
        self.close_project();
        self.new_parent = None;
        let session = Session::new(self.kernel.clone(), "Untitled Project");
        self.ws = Some(Box::new(Workspace::new(session, self.settings.clone())));
        self.saved_generation = 0;
    }

    fn open(&mut self, dir: PathBuf) {
        self.close_project();
        match project::load(self.kernel.as_ref(), &dir) {
            Ok(doc) => {
                let session = Session::with_document(self.kernel.clone(), doc, Some(dir));
                self.ws = Some(Box::new(Workspace::new(session, self.settings.clone())));
                self.saved_generation = 0;
            }
            // The bodies file is unreadable: open the rest and make the bodies again from the History.
            Err(e @ (project::ProjectError::Kernel(_) | project::ProjectError::Bodies(_))) => match self.recover(&dir) {
                Some(note) => {
                    if let Some(ws) = self.ws.as_deref_mut() {
                        ws.toast(note);
                    }
                }
                None => {
                    self.landing.toast =
                        Some((format!("Couldn't open “{}”: its bodies file can't be read ({e})", project_name(&dir)), f64::MAX))
                }
            },
            Err(e) => self.landing.toast = Some((format!("Couldn't open “{}”: {e}", project_name(&dir)), f64::MAX)),
        }
    }

    /// Opens a project whose bodies file can't be read, rebuilding its bodies from the History. `None` when that
    /// can't be done either (nothing opened).
    fn recover(&mut self, dir: &Path) -> Option<String> {
        let doc = project::load_without_bodies(self.kernel.as_ref(), dir).ok()?;
        let mut session = Session::with_document(self.kernel.clone(), doc, Some(dir.to_path_buf()));
        let failed = session.rebuild_bodies().ok()?;
        let made = session.doc.bodies.len();
        session.dirty = true;
        self.ws = Some(Box::new(Workspace::new(session, self.settings.clone())));
        self.saved_generation = 0;
        Some(match failed {
            0 => format!("This project's saved bodies couldn't be read, so its {made} bodies were rebuilt from the History."),
            n => format!(
                "This project's saved bodies couldn't be read. They were rebuilt from the History, but {n} steps failed: check them."
            ),
        })
    }

    /// Projects save themselves, like the reference app; ours go to a folder on disk.
    fn save(&mut self) {
        let root = self.new_parent.clone().unwrap_or_else(|| self.landing.default_dir());
        let folder_ready = self.folder.is_ready();
        let Some(ws) = self.ws.as_deref_mut() else { return };
        // A new project waits for the projects folder (macOS may still be asking); it stays unsaved till then.
        if ws.session.path.is_none() && !folder_ready {
            return;
        }
        // A tutorial's scenes are scratch: they never become projects.
        if ws.session.doc.is_empty() && ws.session.path.is_none() || ws.tutorial.is_some() {
            return;
        }
        let dir = ws.session.path.clone().unwrap_or_else(|| project::unique_dir(&root, &ws.session.doc.name));
        if let Err(e) = project::save(&ws.session.doc, ws.session.kernel.as_ref(), &dir) {
            ws.toast(format!("Couldn't save: {e}"));
            return;
        }
        ws.session.path = Some(dir.clone());
        ws.session.dirty = false;
        write_thumbnail(ws, &dir);
        self.saved_generation = ws.session.revision;
        self.landing.rescan();
        crate::backup::backups().changed();
    }

    /// Opens a project, asking first when another computer sharing the folder has it open (both saving it makes two
    /// versions the sync app keeps side by side).
    fn open_checked(&mut self, dir: PathBuf) {
        let me = crate::backup::computer_name();
        match project::open_elsewhere(&dir, &me, std::time::Duration::from_secs(180)) {
            Some((who, ago)) => self.ask_open = Some((dir, who, ago)),
            None => self.open(dir),
        }
    }

    /// The open-elsewhere question, over the projects screen.
    fn ask_open_dialog(&mut self, ui: &mut egui::Ui) {
        use crate::workspace::drawings::{button, dialog};
        let Some((dir, who, ago)) = self.ask_open.clone() else { return };
        let full = ui.ctx().content_rect();
        let mut answer = None;
        egui::Area::new(ui.scope_id().with("ask-open")).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
            let _ = ui.allocate_rect(full, egui::Sense::click());
            let r = dialog(ui, full, egui::vec2(430.0, 150.0), &format!("“{}” is open on {who}", project_name(&dir)));
            let mins = ago.as_secs() / 60;
            let when = if mins == 0 { "less than a minute ago".to_string() } else { format!("{mins} min ago") };
            let p = ui.painter();
            for (k, line) in
                [format!("{who} was working on it {when}."), "If both computers change it, you'll end up with two versions.".to_string()]
                    .iter()
                    .enumerate()
            {
                p.text(
                    egui::pos2(r.left() + 16.0, r.top() + 52.0 + 18.0 * k as f32),
                    egui::Align2::LEFT_CENTER,
                    line,
                    crate::theme::body(12.0),
                    crate::theme::Tokens::text(),
                );
            }
            let b = |x: f32| egui::Rect::from_min_size(egui::pos2(r.right() - x, r.bottom() - 42.0), egui::vec2(110.0, 28.0));
            if button(ui, b(244.0), "Cancel", false, "ask-open-cancel") {
                answer = Some(false);
            }
            if button(ui, b(124.0), "Open Anyway", true, "ask-open-go") {
                answer = Some(true);
            }
        });
        if let Some(go) = answer {
            self.ask_open = None;
            if go {
                self.open(dir);
            }
        }
    }

    /// Keeps the open project's mark fresh, and takes it away when the project closes.
    fn keep_mark(&mut self) {
        let me = crate::backup::computer_name();
        let open = self.ws.as_ref().and_then(|w| w.session.path.clone());
        match (&self.marked, open) {
            (Some((dir, at)), Some(now)) if *dir == now => {
                if at.elapsed() > std::time::Duration::from_secs(60) {
                    project::mark_open(&now, &me);
                    self.marked = Some((now, std::time::Instant::now()));
                }
            }
            (old, now) => {
                if let Some((dir, _)) = old {
                    project::mark_closed(dir, &me);
                }
                if let Some(dir) = &now {
                    project::mark_open(dir, &me);
                }
                self.marked = now.map(|d| (d, std::time::Instant::now()));
            }
        }
    }

    /// Tells the backup thread what to back up where, when that changed, and shows the backups on the projects screen.
    fn keep_backups(&mut self, ctx: &egui::Context) {
        use crate::backup::{BACKUPS_DIR, Target, backups};
        if let Some(sw) = crate::backup::take_switch() {
            self.switch_work_folder(sw.to, sw.bring);
        }
        crate::backup::set_work(&self.landing.root);
        if self.settings_file.is_some() {
            self.keep_mark();
        }
        let detector = crate::cloud::detector();
        let found = detector.found();
        if found.is_none() && self.settings.backups.iter().any(|b| Target::parse(b).is_cloud()) {
            if !detector.busy() {
                let ctx = ctx.clone();
                detector.refresh(move || ctx.request_repaint());
            }
            return;
        }
        let found = found.unwrap_or_default();
        let targets: crate::backup::Targets = if self.own_folder {
            crate::backup::Targets::default()
        } else {
            self.settings.backups.iter().map(|b| (b.clone(), Target::parse(b).root(&found))).collect()
        };
        let plan = crate::backup::Plan {
            per_computer: self.settings.backup_per_computer,
            versions: self.settings.backup_versions,
            after_change: self.settings.backup_after_change,
        };
        let setup = (self.landing.root.clone(), targets, plan);
        if self.backup_sent.as_ref() != Some(&setup) {
            if !setup.1.is_empty() || self.backup_sent.is_some() {
                let state = self.settings_file.as_ref().map(|f| f.with_file_name("backups.json"));
                let ctx = ctx.clone();
                backups().setup(setup.0.clone(), setup.1.clone(), state, setup.2.clone(), move || ctx.request_repaint());
            }
            self.backup_sent = Some(setup);
        }
        let places = self
            .settings
            .backups
            .iter()
            .filter_map(|b| {
                let t = Target::parse(b);
                let root = t.root(&found)?;
                Some(crate::landing::Place { label: t.label(&found), dir: root.join(BACKUPS_DIR), cloud: t.is_cloud() })
            })
            .collect();
        self.landing.set_places(places);
    }

    /// Makes `to` the Work Folder (Preferences ▸ Sync), bringing the projects from the old one when asked. The open
    /// project is saved first and keeps working from where it moved to.
    fn switch_work_folder(&mut self, to: PathBuf, bring: bool) {
        let from = self.landing.root.clone();
        if to == from {
            return;
        }
        if to.starts_with(&from) || from.starts_with(&to) {
            crate::backup::set_note("Pick a folder that isn't inside the Work Folder (or holding it).".into());
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&to) {
            crate::backup::set_note(format!("Couldn't use {}: {e}", to.display()));
            return;
        }
        let mut note = format!("Projects now save to {}.", crate::landing::shorten_home(&to));
        if bring {
            if self.ws.as_ref().is_some_and(|w| w.session.dirty) {
                self.save();
            }
            let open = self.ws.as_ref().and_then(|w| w.session.path.clone());
            let items: Vec<PathBuf> = std::fs::read_dir(&from)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| !e.file_name().to_string_lossy().starts_with('.') && e.path().is_dir())
                        .map(|e| e.path())
                        .collect()
                })
                .unwrap_or_default();
            let (mut moved, mut failed) = (0, Vec::new());
            for item in items {
                match project::move_to(&item, &to) {
                    Ok(new) => {
                        moved += 1;
                        if let Some(ws) = self.ws.as_deref_mut()
                            && let Some(o) = open.as_ref().and_then(|o| o.strip_prefix(&item).ok())
                        {
                            ws.session.path = Some(new.join(o));
                        }
                    }
                    Err(e) => failed.push(format!("{}: {e}", project_name(&item))),
                }
            }
            note = format!("Moved {moved} item{} to {}.", if moved == 1 { "" } else { "s" }, crate::landing::shorten_home(&to));
            if !failed.is_empty() {
                note += &format!(" {} stayed behind ({}).", failed.len(), failed.join("; "));
            }
        }
        self.settings.work_folder = if to == Options::default_root() { String::new() } else { to.display().to_string() };
        self.sync_settings();
        self.landing.set_root(to.clone());
        self.folder = crate::folder_access::FolderAccess::ready(to);
        crate::backup::set_note(note);
    }

    /// Saves a moment after the last change, or as soon as the window loses the keyboard: leaving for a slicer or a file
    /// manager shouldn't find the project a change behind.
    fn autosave(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        let left = std::mem::replace(&mut self.was_focused, focused) && !focused;
        let Some(ws) = self.ws.as_deref() else { return };
        if ws.session.revision == self.saved_generation || !ws.session.dirty {
            return;
        }
        let first = *self.last_change.get_or_insert_with(std::time::Instant::now);
        if left || first.elapsed().as_secs_f32() > 1.5 {
            self.last_change = None;
            self.save();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    /// Whether the graphics device is gone (driver reset, GPU unplugged or disabled, some wake-ups). The toolkit's paint
    /// panics on a lost device, so this asks the same question first, with the same kind of allocation.
    fn gpu_gone(&mut self, state: &egui_wgpu::RenderState) -> bool {
        use std::sync::atomic::Ordering;
        let probe = self.gpu_probe.get_or_insert_with(|| {
            state.device.set_device_lost_callback(|reason, _| {
                if reason != wgpu::DeviceLostReason::Destroyed {
                    GPU_LOST.store(true, Ordering::Relaxed);
                }
            });
            state.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lost device probe"),
                size: 4,
                usage: wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        GPU_LOST.load(Ordering::Relaxed)
            || wgpu::BufferSize::new(4).is_some_and(|size| state.queue.write_buffer_with(probe, 0, size).is_none())
    }

    /// Saves the work and starts over on whatever graphics are left. A device can't come back in this process.
    fn restart_without_gpu(&mut self) {
        // Not `save`: its thumbnail needs the device that's gone.
        let root = self.new_parent.clone().unwrap_or_else(|| self.landing.default_dir());
        let mut open = None;
        if let Some(ws) = self.ws.as_deref_mut()
            && !(ws.session.doc.is_empty() && ws.session.path.is_none())
        {
            let dir = ws.session.path.clone().unwrap_or_else(|| project::unique_dir(&root, &ws.session.doc.name));
            match project::save(&ws.session.doc, ws.session.kernel.as_ref(), &dir) {
                Ok(()) => open = Some(dir),
                Err(e) => eprintln!("couldn't save before restarting: {e}"),
            }
        }
        let mut args = Vec::new();
        let mut old = std::env::args_os().skip(1);
        while let Some(arg) = old.next() {
            if arg == "--open" {
                old.next();
            } else if arg != "--new" {
                args.push(arg);
            }
        }
        if let Some(dir) = open {
            args.push("--open".into());
            args.push(dir.into_os_string());
        }
        let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).args(args).env(RESTARTED_ENV, "1").spawn());
        if let Err(e) = started {
            eprintln!("couldn't restart: {e}");
        }
        std::process::exit(0);
    }

    fn close_project(&mut self) {
        if self.ws.as_ref().is_some_and(|w| w.session.dirty) {
            self.save();
        }
        crate::backup::backups().now();
        self.ws = None;
        self.landing.rescan();
    }

    fn import_dialog(&mut self) {
        let filters: [(&str, &[&str]); 7] = [
            ("All Supported Files", &["shapr", "step", "stp", "x_t", "x_b", "iges", "igs", "stl", "obj", "dxf", "svg"]),
            (".shapr project", &["shapr"]),
            ("STEP", &["step", "stp"]),
            ("Parasolid", &["x_t", "x_b"]),
            ("IGES", &["iges", "igs"]),
            ("Meshes (STL, OBJ)", &["stl", "obj"]),
            ("2D (DXF, SVG)", &["dxf", "svg"]),
        ];
        let Some(path) = open_path(&mut self.tablet_files, Wanted::Import, "Import", &filters) else { return };
        self.import_picked(path);
    }

    fn import_picked(&mut self, path: PathBuf) {
        if self.ws.is_none() {
            self.new_project();
        }
        let Some(ws) = self.ws.as_deref_mut() else { return };
        // STEP, IGES and Parasolid first ask how to clean the geometry up, unless told not to.
        let mode = crate::workspace::import_prefs::mode_index(&ws.settings.import_mode);
        if crate::workspace::import_prefs::wants_sheet(&path) && !ws.settings.import_skip_prefs {
            ws.import_prefs =
                Some(crate::workspace::import_prefs::ImportPrefs { path, mode, custom: ws.settings.import_custom, dont_show: false });
            return;
        }
        let heal = crate::workspace::import_prefs::heal_param(mode, ws.settings.import_custom);
        self.import_file(path, heal);
    }

    /// Brings a file into the open project (after Import Preferences, when it asked).
    fn import_file(&mut self, path: std::path::PathBuf, heal: serde_json::Value) {
        if let Some(ws) = self.ws.as_deref_mut() {
            // A project made for the import takes the file's name; one with work in it keeps its own.
            let empty = ws.session.doc.bodies.is_empty() && ws.session.doc.sketches.is_empty();
            if empty
                && ws.session.doc.name == "Untitled Project"
                && let Some(stem) = path.file_stem()
            {
                ws.session.doc.name = stem.to_string_lossy().into_owned();
            }
            let mut params = json!({ "path": path });
            if crate::workspace::import_prefs::wants_sheet(&path) {
                params["heal"] = heal;
            }
            if let Some(done) = ws.run("import_file", params) {
                ws.standard_view(StandardView::Default);
                // Solids with geometry we can't read yet are left out; say so rather than lose them quietly.
                let skipped = done["unreadable"].as_array().map_or(0, Vec::len);
                if skipped > 0 {
                    ws.toast(format!(
                        "{skipped} {} in this file couldn't be read yet and {} left out",
                        if skipped == 1 { "solid" } else { "solids" },
                        if skipped == 1 { "was" } else { "were" }
                    ));
                }
            }
        }
    }

    /// Import Project (⇧⌘P): the local projects in a sheet to pick from.
    fn import_project_sheet(&mut self) {
        if !self.folder.is_ready() {
            if let Some(ws) = self.ws.as_deref_mut() {
                ws.toast("The projects folder isn't open yet".to_string());
            }
            return;
        }
        let all = self.landing.all_projects();
        let Some(ws) = self.ws.as_deref_mut() else { return };
        let projects = all.into_iter().map(|p| (p.name, p.dir, p.thumbnail, p.modified)).collect();
        ws.import_sheet = Some(crate::workspace::import_project::ImportSheet::new(projects));
    }

    /// Insert ▸ Image: picks a PNG or JPEG and places it on the selected face or plane (the top plane otherwise).
    fn image_dialog(&mut self) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        ws.wants_image = false;
        let Some(path) = open_path(
            &mut self.tablet_files,
            Wanted::Image,
            "Insert Image",
            &[("Image", &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "gif", "ico"])],
        ) else {
            return;
        };
        self.image_picked(path);
    }

    fn image_picked(&mut self, path: PathBuf) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        // In Drawings the picture goes on the open sheet.
        if let Some(id) = ws.drawings.as_ref().map(|d| d.id) {
            let _ = ws.run("insert_sheet_image", json!({ "drawing": id.0, "path": path }));
            return;
        }
        let _ = ws.insert_image(&path);
    }

    /// Visualization ▸ Decal ⊕: adds a PNG or JPEG picture to the project's decals.
    fn decal_dialog(&mut self) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        ws.wants_decal = false;
        let Some(path) = open_path(
            &mut self.tablet_files,
            Wanted::Decal,
            "Add a Decal",
            &[("Image", &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "gif", "ico"])],
        ) else {
            return;
        };
        let _ = ws.run("import_decal", json!({ "path": path }));
    }

    /// Visualization ▸ Custom ⊕: makes a custom material from a PNG or JPEG picture.
    fn material_dialog(&mut self) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        ws.wants_material = false;
        let Some(path) = open_path(
            &mut self.tablet_files,
            Wanted::Material,
            "Custom Material",
            &[("Image", &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "gif", "ico"])],
        ) else {
            return;
        };
        let _ = ws.run("import_material", json!({ "path": path }));
    }

    /// Visualization ▸ Environment ▸ Import…: adds a panorama as a custom environment.
    fn environment_dialog(&mut self) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        ws.wants_environment = false;
        let filters: [(&str, &[&str]); 1] = [("Panorama", &["hdr", "exr", "png", "jpg", "jpeg"])];
        let Some(path) = open_path(&mut self.tablet_files, Wanted::Environment, "Import Environment", &filters) else { return };
        let _ = ws.run("import_environment", json!({ "path": path }));
    }

    /// Saves the open drawing in each chosen format: one save dialog, the others written next to it.
    fn capture_dialog(&mut self) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        if !std::mem::take(&mut ws.wants_capture) {
            return;
        }
        let name = format!("{} {}.png", ws.session.doc.name, if ws.visualizing { "Capture" } else { "Screenshot" });
        let Some(path) = save_path(&mut self.tablet_files, &name, ("PNG", &["png"])) else { return };
        match ws.capture_png(&path) {
            Ok(()) => ws.toast(format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            Err(e) => ws.toast(format!("Couldn't save the image: {e}")),
        }
    }

    fn export_drawing_dialog(&mut self) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        let Some(formats) = ws.wants_drawing_export.take() else { return };
        let Some(first) = formats.first().copied() else { return };
        let drawing = ws.drawings.as_ref().map(|d| d.id.0);
        let stem = drawing
            .and_then(|id| ws.session.doc.drawing(shapecraft_doc::ItemId(id)))
            .map_or_else(|| ws.session.doc.name.clone(), |d| d.name.clone());
        let upper = first.to_uppercase();
        let Some(path) = save_path(&mut self.tablet_files, &format!("{stem}.{first}"), (&upper, &[first])) else { return };
        let mut written = Vec::new();
        for f in &formats {
            let out = path.with_extension(f);
            let mut params = json!({ "path": out });
            if let Some(id) = drawing {
                params["drawing"] = json!(id);
            }
            if ws.run("export_drawing", params).is_some() {
                written.push(out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            }
        }
        if !written.is_empty() {
            ws.toast(format!("Exported {}", written.join(", ")));
        }
    }

    /// Opens the Export dialog, optionally with one format picked.
    fn export_dialog(&mut self, preset: Option<Format>) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        let mut d = ExportDialog::new(&ws.session.doc.name, preset);
        d.slicer = crate::workspace::slicers::pick(&ws.settings.slicer);
        ws.export = Some(d);
    }

    /// Quick Export to Favorite Formats: the last export's formats, written again into its folder without asking;
    /// before any export it opens the Export dialog.
    fn quick_export(&mut self) {
        let Some(ws) = self.ws.as_deref_mut() else { return };
        let formats: Vec<Format> =
            ws.settings.favorite_formats.iter().filter_map(|n| Format::from_label(n)).filter(|f| *f != Format::Slicer).collect();
        let dir = ws.settings.last_export_dir.clone().map(std::path::PathBuf::from).filter(|d| d.is_dir());
        match dir {
            Some(dir) if !formats.is_empty() => {
                let mut d = ExportDialog::new(&ws.session.doc.name, None);
                d.chosen = formats;
                d.quick = Some(dir);
                ws.wants_export = Some(d);
                self.run_export();
            }
            _ => self.export_dialog(None),
        }
    }

    /// Export pressed: asks where (a file for one format, a folder for several or one file per item), then writes
    /// each format through its command.
    fn run_export(&mut self) {
        use crate::workspace::export::Format;
        let Some(ws) = self.ws.as_deref_mut() else { return };
        let Some(mut d) = ws.wants_export.take() else { return };
        let name = d.name();
        // Slicer: a 3MF in a scratch folder, opened in the chosen slicer; no save dialog of its own.
        if d.chosen.contains(&Format::Slicer) {
            d.chosen.retain(|f| *f != Format::Slicer);
            let tablet_dir = self.tablet_files.as_mut().and_then(tablet_folder);
            let on_tablet = tablet_dir.is_some();
            let dir = tablet_dir.unwrap_or_else(|| std::env::temp_dir().join("ShapeCraft Slicer"));
            let path = dir.join(format!("{name}.3mf"));
            let mut params = d.params(Format::Slicer);
            params["path"] = json!(path);
            let made = std::fs::create_dir_all(&dir).is_ok() && ws.run("export_3mf", params).is_some();
            let shown = d.slicer.clone();
            ws.settings.slicer = shown.clone();
            let opened = made && !on_tablet && crate::workspace::slicers::open(&shown, &path);
            ws.toast(if made && on_tablet {
                format!("{name}.3mf is ready to download for {shown}")
            } else if opened {
                format!("Opened {name} in {shown}")
            } else if made {
                format!("Couldn't open {shown}: is it installed?")
            } else {
                format!("Couldn't export {name} for {shown}")
            });
            if d.chosen.is_empty() {
                return;
            }
        }
        let single = d.chosen.len() == 1 && !(d.separately && d.chosen[0].per_body());
        let folder = if let Some(dir) = d.quick.clone() {
            (dir, name.clone())
        } else if single {
            let (ext, _) = d.chosen[0].ext_and_command();
            let upper = ext.to_uppercase();
            let Some(path) = save_path(&mut self.tablet_files, &format!("{name}.{ext}"), (&upper, &[ext])) else { return };
            match path.parent() {
                Some(dir) => (dir.to_path_buf(), path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(name.clone())),
                None => return,
            }
        } else {
            let Some(dir) = save_folder(&mut self.tablet_files) else { return };
            (dir, name.clone())
        };
        let (dir, stem) = folder;
        let mut written = Vec::new();
        let mut failed = Vec::new();
        for f in &d.chosen {
            let (ext, cmd) = f.ext_and_command();
            // One file per visible (or every) body, named after it, when asked; else one file of them all.
            let jobs: Vec<(String, Option<u64>)> = if d.separately && f.per_body() {
                ws.session
                    .doc
                    .bodies
                    .iter()
                    .filter(|b| b.visible || d.hidden)
                    .map(|b| (b.name.replace(['/', '\\', ':'], "-"), Some(b.id.0)))
                    .collect()
            } else {
                vec![(stem.clone(), None)]
            };
            for (file, body) in jobs {
                let path = dir.join(format!("{file}.{ext}"));
                let ok = match cmd {
                    Some(cmd) => {
                        let mut params = d.params(*f);
                        params["path"] = json!(path);
                        if let Some(id) = body {
                            params["items"] = json!([id]);
                        }
                        if cmd == "export_drawing"
                            && let Some(id) = ws.drawings.as_ref().map(|dr| dr.id.0)
                        {
                            params["drawing"] = json!(id);
                        }
                        ws.run(cmd, params).is_some()
                    }
                    None => ws.capture_png(&path).is_ok(),
                };
                let shown = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if ok { written.push(shown) } else { failed.push(shown) }
            }
        }
        // Remembered for Quick Export to Favorite Formats.
        if !written.is_empty() {
            ws.settings.favorite_formats = d.chosen.iter().map(|f| f.label().to_string()).collect();
            ws.settings.last_export_dir = Some(dir.to_string_lossy().into_owned());
        }
        if !failed.is_empty() {
            ws.toast(format!("Couldn't export {}", failed.join(", ")));
        } else if !written.is_empty() {
            ws.toast(format!("Exported {}", written.join(", ")));
        }
    }

    fn global_keys(&mut self, ctx: &egui::Context) {
        if self.ws.as_ref().is_some_and(|w| w.wants_drawing_export.is_some()) {
            self.export_drawing_dialog();
        }
        if self.ws.as_ref().is_some_and(|w| w.wants_export.is_some()) {
            self.run_export();
        }
        if self.ws.as_ref().is_some_and(|w| w.wants_capture) {
            self.capture_dialog();
        }
        if self.ws.as_ref().is_some_and(|w| w.wants_image) {
            self.image_dialog();
        }
        if self.ws.as_ref().is_some_and(|w| w.wants_decal) {
            self.decal_dialog();
        }
        if self.ws.as_ref().is_some_and(|w| w.wants_material) {
            self.material_dialog();
        }
        if self.ws.as_ref().is_some_and(|w| w.wants_environment) {
            self.environment_dialog();
        }
        let (n, w, s, e, i) = ctx.input_mut(|inp| {
            let cmd = egui::Modifiers::COMMAND;
            let cmd_shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
            (
                inp.consume_key(cmd, egui::Key::N),
                inp.consume_key(cmd, egui::Key::W),
                inp.consume_key(cmd, egui::Key::S),
                inp.consume_key(cmd_shift, egui::Key::E),
                inp.consume_key(cmd_shift, egui::Key::I),
            )
        });
        if n {
            self.new_project();
        }
        if w {
            self.close_project();
        }
        if s {
            // Saving happens by itself; ⌘S (muscle memory from other apps) also backs up now.
            self.save();
            crate::backup::backups().now();
        }
        if e {
            self.export_dialog(None);
        }
        if i {
            self.import_dialog();
        }
    }

    fn handle_control(&mut self, ctx: &egui::Context) {
        let mut reqs = Vec::new();
        if let Some(server) = &self.control {
            while let Ok(r) = server.requests.try_recv() {
                reqs.push(r);
            }
        }
        for Request { method, params, reply } in reqs {
            if method == "ui.screenshot" {
                let path = params.get("path").and_then(Value::as_str).map(PathBuf::from);
                match path {
                    Some(p) => {
                        let waiting = Arc::new(std::sync::Mutex::new(Some(reply)));
                        self.capture = Some((waiting.clone(), std::time::Instant::now()));
                        let cb = egui::ScreenshotCallback::new(move |image| {
                            if let Some(reply) = waiting.lock().ok().and_then(|mut w| w.take()) {
                                let _ = reply.send(write_color_image(&image, &p).map(|_| json!({ "path": p })));
                            }
                        });
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(cb));
                    }
                    None => {
                        let _ = reply.send(Err("'path' is required".into()));
                    }
                }
                continue;
            }
            let _ = reply.send(self.control_call(&method, &params));
        }
    }

    pub fn control_call(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "engine.execute" => {
                if self.ws.is_none() {
                    self.new_project();
                }
                let ws = self.ws.as_deref_mut().ok_or("no project")?;
                let command = params.get("command").and_then(Value::as_str).ok_or("'command' is required")?;
                let p = params.get("params").cloned().unwrap_or(json!({}));
                // A modeling tool left open (Offset Face, Fillet …) ends first, keeping what it already applied, as
                // a menu command would: its handles, pills and hint would otherwise stay on screen over the new model.
                ws.end_model_tool();
                let out = ws.session.execute(command, &p).map_err(|e| e.to_string())?;
                // Entering a sketch over the control channel looks at its plane, unless the call says "look": false
                // (scripts that place the camera themselves, recordings). ui.camera also stops a move under way.
                if command == "sketch"
                    && params.get("look").or_else(|| p.get("look")).and_then(Value::as_bool) != Some(false)
                    && let Some(sk) = ws.session.active_sketch()
                {
                    let plane = sk.plane;
                    ws.animate_to(shapecraft_render::Camera::facing(plane.normal(), plane.y, plane.origin, ws.camera.distance));
                }
                Ok(out.value)
            }
            "engine.commands" => Ok(Value::Array(
                shapecraft_engine::registry()
                    .iter()
                    .map(|c| json!({ "id": c.id, "label": c.label, "menu": c.menu, "shortcut": c.shortcut, "help": c.help, "params": c.params }))
                    .collect(),
            )),
            "project.new" => {
                self.new_project();
                Ok(Value::Null)
            }
            "project.open" => {
                let p = params.get("path").and_then(Value::as_str).ok_or("'path' is required")?;
                self.open(PathBuf::from(p));
                Ok(json!({ "open": self.ws.is_some() }))
            }
            "project.save" => {
                self.save();
                Ok(json!({ "path": self.ws.as_ref().and_then(|w| w.session.path.clone()) }))
            }
            "project.home" => {
                self.close_project();
                Ok(Value::Null)
            }
            "ui.tutorial" => {
                let id = params.get("lesson").and_then(Value::as_str).ok_or("'lesson' is required")?;
                let step = params.get("step").and_then(Value::as_u64).unwrap_or(0) as usize;
                let lesson = crate::tutorials::lesson(id).ok_or_else(|| format!("no tutorial '{id}'"))?;
                if step >= lesson.steps.len() {
                    return Err(format!("'{id}' has {} steps", lesson.steps.len()));
                }
                self.start_tutorial(id, step);
                Ok(json!({ "steps": lesson.steps.len() }))
            }
            "ui.tutorial_state" => Ok(self.ws.as_deref().and_then(Workspace::tutorial_state).unwrap_or(Value::Null)),
            "ui.tutorial_action" => {
                use crate::workspace::tutorial::Request;
                let req = match params.get("action").and_then(Value::as_str) {
                    Some("retry") => Request::Retry,
                    Some("continue") => Request::Next,
                    Some("quit") => Request::Quit,
                    _ => return Err("'action' is retry, continue or quit".into()),
                };
                let ctx = self.egui.clone();
                self.tutorial_request(req, &ctx);
                Ok(Value::Null)
            }
            "ui.landing" => {
                if self.ws.is_some() {
                    self.close_project();
                }
                let section = params.get("section").and_then(Value::as_str).unwrap_or("recents");
                self.landing.show_section(section).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "project.list" if !self.folder.is_ready() => Err("the projects folder isn't open yet".into()),
            "project.list" => {
                let root = self.landing.root.clone();
                let dir = params.get("folder").and_then(Value::as_str).map(PathBuf::from).unwrap_or_else(|| root.clone());
                if !dir.starts_with(&root) {
                    return Err("folder must be inside the projects folder".into());
                }
                let projects: Vec<Value> = project::list(&dir).iter().map(|p| json!({ "name": p.name, "path": p.dir })).collect();
                Ok(json!({ "folders": project::folders(&dir), "projects": projects }))
            }
            "project.new_folder" | "project.rename" | "project.duplicate" | "project.delete" if !self.folder.is_ready() => {
                Err("the projects folder isn't open yet".into())
            }
            "project.new_folder" | "project.rename" | "project.duplicate" | "project.delete" => {
                let root = self.landing.root.clone();
                let path = params.get("path").and_then(Value::as_str).map(PathBuf::from).unwrap_or_else(|| root.clone());
                if !path.starts_with(&root) {
                    return Err("path must be inside the projects folder".into());
                }
                let done = match method {
                    "project.new_folder" => project::new_folder(&path),
                    "project.rename" => project::rename(&path, params.get("name").and_then(Value::as_str).ok_or("'name' is required")?),
                    "project.duplicate" => project::duplicate(&path),
                    _ => project::trash(&path, &root),
                }
                .map_err(|e| e.to_string())?;
                self.landing.rescan();
                Ok(json!({ "path": done }))
            }
            "ui.view" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let v = match params.get("view").and_then(Value::as_str).unwrap_or("default") {
                    "front" => StandardView::Front,
                    "back" => StandardView::Back,
                    "top" => StandardView::Top,
                    "bottom" => StandardView::Bottom,
                    "right" => StandardView::Right,
                    "left" => StandardView::Left,
                    _ => StandardView::Default,
                };
                ws.standard_view(v);
                Ok(Value::Null)
            }
            "ui.camera" => {
                // Puts the camera somewhere at once: {"target": [x,y,z]?, "yaw": rad?, "pitch": rad?, "distance": d?,
                // "ortho": 0–1?, "fit": bool? (frame the whole model first)}. Recordings use it to orbit.
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                ws.anim = None;
                let mut c = ws.camera;
                if params.get("fit").and_then(Value::as_bool) == Some(true)
                    && let Some((min, max)) = ws.session.doc.bounds()
                {
                    c.fit(min, max);
                }
                let num = |k: &str| params.get(k).and_then(Value::as_f64);
                if let Some(t) = params.get("target").and_then(Value::as_array)
                    && t.len() == 3
                {
                    c.target = glam::DVec3::new(t[0].as_f64().unwrap_or(0.0), t[1].as_f64().unwrap_or(0.0), t[2].as_f64().unwrap_or(0.0));
                }
                c.yaw = num("yaw").unwrap_or(c.yaw);
                c.pitch = num("pitch").unwrap_or(c.pitch);
                c.distance = num("distance").unwrap_or(c.distance);
                c.ortho = num("ortho").unwrap_or(c.ortho);
                ws.camera = c;
                Ok(json!({ "target": c.target.to_array(), "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance }))
            }
            "ui.tool" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                use crate::workspace::toolbar::ToolKind as K;
                let kind = match params.get("tool").and_then(Value::as_str).unwrap_or("") {
                    "sketch" => K::Sketch,
                    "line" => K::Line,
                    "rectangle" => K::Rectangle,
                    "circle" => K::Circle,
                    "extrude" => K::Extrude,
                    "fillet" => K::Fillet,
                    "shell" => K::Shell,
                    "revolve" => K::Revolve,
                    "polygon" => K::Polygon,
                    "arc" => K::Arc,
                    "move" => K::Move,
                    "mirror" => K::Mirror,
                    "pattern" => K::Pattern,
                    "ellipse" => K::Ellipse,
                    "spline" => K::Spline,
                    "offset_face" => K::OffsetFace,
                    "scale" => K::Scale,
                    "split_body" => K::SplitBody,
                    "loft" => K::Loft,
                    "trim" => K::Trim,
                    "offset_plane" => K::OffsetPlane,
                    "pattern_circular" => K::CircularPattern,
                    "project" => K::Project,
                    "text" => K::Text,
                    "sketch_fillet" => K::SketchFillet,
                    "axis_2points" => K::Axis2,
                    "align" => K::Align,
                    "replace_face" => K::ReplaceFace,
                    "wrap_emboss" => K::WrapEmboss,
                    "project_edge" => K::ProjectEdge,
                    "scale_non_uniform" => K::ScaleNonUniform,
                    "plane_3points" => K::Plane3,
                    "rotate_axis" => K::RotateAxis,
                    "offset_edge" => K::OffsetEdge,
                    "sweep" => K::Sweep,
                    "translate" => K::Translate,
                    "union" => K::Union,
                    "subtract" => K::Subtract,
                    "intersect" => K::Intersect,
                    "plane_through_edge" => K::PlaneEdge,
                    "plane_on_curve" => K::PlaneCurve,
                    "none" => {
                        ws.tool = Tool::Idle;
                        return Ok(Value::Null);
                    }
                    other => return Err(format!("unknown tool '{other}'")),
                };
                ws.start_tool(kind);
                Ok(Value::Null)
            }
            "ui.drawing" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let id = params.get("id").and_then(Value::as_u64).ok_or("'id' is required")?;
                if ws.session.doc.drawing(shapecraft_doc::ItemId(id)).is_none() {
                    return Err(format!("no drawing {id}"));
                }
                ws.open_drawing_public(shapecraft_doc::ItemId(id));
                Ok(Value::Null)
            }
            "ui.export" => {
                // Opens the Export dialog with the given formats picked (file extensions, e.g. ["3mf", "stl"]).
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let mut d = ExportDialog::new(&ws.session.doc.name, None);
                if let Some(list) = params.get("formats").and_then(Value::as_array) {
                    d.chosen = list.iter().filter_map(Value::as_str).filter_map(Format::from_extension).collect();
                }
                ws.export = Some(d);
                Ok(Value::Null)
            }
            "ui.onboarding" => {
                // {"setup": true}: the setup that follows the first-run questions (look, Work Folder, backups).
                if params.get("setup").and_then(Value::as_bool) == Some(true) {
                    self.setup = Some(crate::setup::Setup::new(&self.settings));
                } else {
                    self.ask_device = true;
                }
                Ok(Value::Null)
            }
            "ui.window" => {
                // Size and place the window (points), e.g. for checks at a fixed window size.
                let num = |k: &str| params.get(k).and_then(Value::as_f64).filter(|v| v.is_finite() && (0.0..=20_000.0).contains(v));
                if let (Some(w), Some(h)) = (num("w"), num("h")) {
                    self.egui.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w.max(400.0) as f32, h.max(300.0) as f32)));
                }
                if let (Some(x), Some(y)) = (num("x"), num("y")) {
                    self.egui.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x as f32, y as f32)));
                }
                self.egui.request_repaint();
                Ok(Value::Null)
            }
            "ui.theme" => {
                // Preferences ▸ Theme: System, Light or Dark.
                let name = params.get("theme").and_then(Value::as_str).ok_or("theme: System, Light or Dark")?;
                if !["System", "Light", "Dark"].contains(&name) {
                    return Err(format!("unknown theme '{name}'"));
                }
                self.settings.theme = name.to_string();
                if let Some(ws) = self.ws.as_deref_mut() {
                    ws.settings.theme = name.to_string();
                }
                Ok(Value::Null)
            }
            "ui.setting" => {
                // Any Preferences value by its settings key, e.g. {"key": "confirm_buttons", "value": "Toolbar"}.
                let key = params.get("key").and_then(Value::as_str).ok_or("'key' is required")?;
                let value = params.get("value").cloned().ok_or("'value' is required")?;
                let current = self.ws.as_deref().map_or(&self.settings, |ws| &ws.settings);
                let mut all = serde_json::to_value(current).map_err(|e| e.to_string())?;
                let slot = all.get_mut(key).ok_or_else(|| format!("no setting '{key}'"))?;
                *slot = value;
                self.settings = serde_json::from_value(all).map_err(|e| format!("bad value for '{key}': {e}"))?;
                if let Some(ws) = self.ws.as_deref_mut() {
                    ws.settings = self.settings.clone();
                }
                // Both copies now agree, so the per-frame check that saves Preferences changes won't see this one.
                if let Some(path) = &self.settings_file {
                    self.settings.save(path).map_err(|e| e.to_string())?;
                }
                Ok(Value::Null)
            }
            "ui.work_folder" => {
                // Preferences ▸ Sync ▸ Work Folder ▸ Change…: {"path": "/x", "bring": true} moves the projects too.
                let path = params.get("path").and_then(Value::as_str).ok_or("'path' is required")?;
                let bring = params.get("bring").and_then(Value::as_bool).unwrap_or(false);
                self.switch_work_folder(PathBuf::from(path), bring);
                Ok(json!({ "root": self.landing.root, "note": crate::backup::note() }))
            }
            "ui.import_project" => {
                // Opens Import Project's sheet over the local projects.
                self.ws.as_deref().ok_or("no project open")?;
                self.import_project_sheet();
                Ok(Value::Null)
            }
            "ui.draw_tool" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let i = params.get("index").and_then(Value::as_u64).ok_or("'index' is required")?;
                ws.ui_action(crate::workspace::toolbar::UiAction::DrawTool(i.min(255) as u8));
                Ok(Value::Null)
            }
            "ui.press" => {
                // A left-column button by its label ("Scale", "Fillet"…), pressed on the next frame like a click.
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let label = params.get("label").and_then(Value::as_str).ok_or("'label' is required")?;
                ws.press_label = Some(label.chars().take(80).collect());
                Ok(Value::Null)
            }
            "ui.view_tool" => {
                // A Views tool on the open drawing: 0 Base View, 1 Projection, 2 Section View, 3 Detail View.
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let i = params.get("index").and_then(Value::as_u64).ok_or("'index' is required")?;
                ws.ui_action(crate::workspace::toolbar::UiAction::ViewTool(i.min(255) as u8));
                Ok(Value::Null)
            }
            "ui.capture" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let path = params.get("path").and_then(Value::as_str).ok_or("'path' is required")?;
                ws.capture_png(std::path::Path::new(path))?;
                Ok(json!({ "path": path }))
            }
            "ui.selection_filter" => {
                // Selection filter: {"only": "all" | "bodies" | "faces" | "edges"}.
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                ws.pick_filter = match params.get("only").and_then(Value::as_str).unwrap_or("all") {
                    "all" => "all",
                    "bodies" => "bodies",
                    "faces" => "faces",
                    "edges" => "edges",
                    other => return Err(format!("unknown selection filter '{other}'")),
                };
                Ok(Value::Null)
            }
            "ui.history_step" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                let step = params.get("step").and_then(Value::as_u64).ok_or("'step' is required")?;
                ws.open_history_step(step as usize);
                Ok(Value::Null)
            }
            "ui.visual" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                if let Some(d) = params.get("decal") {
                    ws.decal_sel = d.as_u64().map(shapecraft_doc::ItemId);
                }
                if let Some(t) = params.get("tab").and_then(Value::as_u64) {
                    ws.vis_tab = (t as usize).min(2);
                }
                if let Some(e) = params.get("env_settings").and_then(Value::as_bool) {
                    ws.env_settings = e;
                }
                Ok(Value::Null)
            }
            "ui.toggle" if self.ws.is_none() && params.get("panel").and_then(Value::as_str) == Some("preferences") => {
                self.home_prefs = Some(params.get("tab").and_then(Value::as_u64).unwrap_or(0).min(4) as usize);
                Ok(Value::Null)
            }
            "ui.edit" => {
                // Edit ▸ Copy / Paste as the menu runs them.
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                match params.get("action").and_then(Value::as_str).unwrap_or("") {
                    "copy" => ws.ui_action(crate::workspace::toolbar::UiAction::Copy),
                    "paste" => ws.ui_action(crate::workspace::toolbar::UiAction::Paste),
                    other => return Err(format!("action is copy or paste, not '{other}'")),
                }
                Ok(json!({ "selection": ws.session.selection }))
            }
            "ui.toggle" => {
                let ws = self.ws.as_deref_mut().ok_or("no project open")?;
                match params.get("panel").and_then(Value::as_str).unwrap_or("") {
                    "items" => ws.show_items = !ws.show_items,
                    "history" => ws.show_history = !ws.show_history,
                    "grid" => ws.view_opts.grid = !ws.view_opts.grid,
                    "drawings" => ws.ui_action(crate::workspace::toolbar::UiAction::Drawings),
                    "visualization" => ws.ui_action(crate::workspace::toolbar::UiAction::Visualization),
                    "help" => ws.ui_action(crate::workspace::toolbar::UiAction::Help),
                    "preferences" => {
                        ws.prefs_tab = params.get("tab").and_then(Value::as_u64).unwrap_or(0).min(4) as usize;
                        ws.ui_action(crate::workspace::toolbar::UiAction::Preferences);
                    }
                    "nav_presets" => ws.ui_action(crate::workspace::toolbar::UiAction::NavPresets),
                    "variables" => ws.ui_action(crate::workspace::toolbar::UiAction::Variables),
                    "shortcuts" => ws.ui_action(crate::workspace::toolbar::UiAction::Shortcuts),
                    other => return Err(format!("unknown panel '{other}'")),
                }
                Ok(Value::Null)
            }
            "ui.state" => Ok(match self.ws.as_deref() {
                None => json!({ "screen": "projects", "root": self.landing.root, "preferences": self.home_prefs.is_some() }),
                Some(ws) => json!({
                    "screen": "workspace",
                    "graphics": crate::graphics::status().active.map(|a| format!("{}: {}", a.path, a.name)),
                    "project": ws.session.doc.name,
                    "path": ws.session.path,
                    "tool": format!("{:?}", std::mem::discriminant(&ws.tool)),
                    "active_sketch": ws.session.active_sketch.map(|i| i.0),
                    "renaming": ws.renaming.is_some(),
                    "selection": ws.session.selection,
                    "camera": { "target": ws.camera.target.to_array(), "yaw": ws.camera.yaw, "pitch": ws.camera.pitch, "distance": ws.camera.distance },
                    "banner": ws.seen_banner,
                    "column": ws.seen_column,
                    "bottom": ws.seen_bottom,
                }),
            }),
            // Frame pacing since the last call. Keep the app busy while measuring: time it spends idle counts as slow
            // frames, up to the 5 s past which a gap can only be idling.
            "ui.frames" => {
                let mut gaps: Vec<f32> = self.frame_gaps.drain(..).filter(|g| *g < 5000.0).collect();
                gaps.sort_by(f32::total_cmp);
                let at = |q: f32| gaps.get((gaps.len().saturating_sub(1) as f32 * q) as usize).copied();
                let total: f32 = gaps.iter().sum();
                let mut work: Vec<f32> = self.frame_work.iter().map(|w| w.0).collect();
                let mut view: Vec<f32> = self.frame_work.drain(..).map(|w| w.1).collect();
                work.sort_by(f32::total_cmp);
                view.sort_by(f32::total_cmp);
                Ok(json!({
                    "work_p50_ms": work.get(work.len() / 2), "view_p50_ms": view.get(view.len() / 2),
                    "frames": gaps.len(),
                    "fps": if total > 0.0 { gaps.len() as f32 * 1000.0 / total } else { 0.0 },
                    "p50_ms": at(0.5), "p95_ms": at(0.95), "p99_ms": at(0.99), "worst_ms": gaps.last(),
                    "over_33ms": gaps.iter().filter(|g| **g > 33.4).count(),
                    "points": [self.view_size.0.x, self.view_size.0.y], "pixels_per_point": self.view_size.1,
                }))
            }
            // Updates: what the updater knows, a check as Help ▸ Check for Updates… makes, closing the window as its
            // close button does (with an update ready, that asks first), and an answer to that question.
            "update.state" => {
                let u = shapecraft_update::Updater::global();
                let s = u.snapshot();
                Ok(json!({
                    "current": s.current,
                    "install": format!("{:?}", s.install),
                    "phase": format!("{:?}", s.phase),
                    "offered": s.offered.as_ref().map(|o| o.0.clone()),
                    "manual": s.manual,
                    "ready": u.ready(),
                    "asking": self.updates.asking,
                    "release_build": shapecraft_update::release_build(),
                }))
            }
            "update.check" => {
                shapecraft_update::Updater::global().check_now();
                Ok(Value::Null)
            }
            "update.download" => {
                shapecraft_update::Updater::global().download();
                Ok(Value::Null)
            }
            "app.close" => {
                self.egui.send_viewport_cmd(egui::ViewportCommand::Close);
                Ok(Value::Null)
            }
            "update.answer" => {
                use crate::updates::Action;
                let action = match params.get("choice").and_then(Value::as_str) {
                    Some("update") => Some(Action::Install { restart: false }),
                    Some("reopen") => Some(Action::Install { restart: true }),
                    Some("not_now") => Some(Action::CloseAnyway),
                    Some("cancel") => None,
                    _ => return Err("'choice' is update, reopen, not_now or cancel".into()),
                };
                self.updates.asking = false;
                if let Some(action) = action {
                    let ctx = self.egui.clone();
                    self.update_action(&ctx, action);
                }
                Ok(Value::Null)
            }
            other => Err(format!("unknown method '{other}'")),
        }
    }

    /// One frame. `render_state` is the window's wgpu state when there is one.
    pub fn frame(&mut self, ui: &mut egui::Ui, render_state: Option<&egui_wgpu::RenderState>) {
        if let Some(state) = render_state
            && !self.hosted
            && self.gpu_gone(state)
        {
            self.restart_without_gpu();
        }
        let began = std::time::Instant::now();
        // Preferences ▸ Language takes effect at once (the workspace keeps its own copy of the settings).
        crate::i18n::set(self.ws.as_deref().map_or(&self.settings.language, |ws| &ws.settings.language));
        if let Some(last) = self.last_frame.replace(began)
            && self.frame_gaps.len() < 100_000
        {
            self.frame_gaps.push((began - last).as_secs_f32() * 1000.0);
        }
        let ctx = ui.ctx().clone();
        self.view_size = (ctx.content_rect().size(), ctx.pixels_per_point());
        self.keep_backups(&ctx);
        let settings = self.ws.as_deref().map_or(&self.settings, |ws| &ws.settings);
        let light = match settings.theme.as_str() {
            "Light" => true,
            "System" => crate::system_theme::light().unwrap_or(ctx.system_theme() == Some(egui::Theme::Light)),
            _ => false,
        };
        if !self.styled || light != theme::is_light() {
            theme::set_light(light);
            if !self.styled {
                theme::install_fonts(&ctx);
            }
            theme::apply_style(&ctx);
            self.styled = true;
        }
        if !self.fitted {
            self.fitted = fit_window(&ctx);
            if std::env::var_os(RESTARTED_ENV).is_some()
                && let Some(ws) = self.ws.as_deref_mut()
            {
                ws.toast("The graphics driver reset, so ShapeCraft restarted. Your work was saved.");
            }
        }
        if let Some(ws) = self.ws.as_deref_mut()
            && crate::graphics::take_slow_warning()
        {
            ws.toast("Drawing on the processor, so things may feel slow. See Preferences ▸ ShapeCraft.");
        }
        // Without a native menu bar nothing else would catch the menus' shortcuts, and they go before the workspace's keys.
        if let Some(bar) = self.menubar.as_mut() {
            bar.take_shortcuts(&ctx);
        }
        self.handle_menu(&ctx);
        self.global_keys(&ctx);

        let first_run_open = self.first_run_open();
        match self.ws.as_deref_mut() {
            Some(ws) => {
                // The first-run windows sit over the workspace: its keys wait until they're answered.
                ws.keys_blocked = first_run_open;
                ws.ui(ui, render_state);
                // Tutorials run on the default tool types and leave the remembered ones alone.
                if ws.tutorial.is_none() {
                    ws.keep_variants();
                }
                if let Some((path, heal)) = ws.import_file_go.take() {
                    self.import_file(path, heal);
                }
                let Some(ws) = self.ws.as_deref_mut() else { return };
                if ws.settings != self.settings {
                    self.settings = ws.settings.clone();
                    if let Some(path) = &self.settings_file
                        && let Err(e) = self.settings.save(path)
                    {
                        ws.toast(format!("Couldn't save preferences: {e}"));
                    }
                }
                if ws.go_home {
                    self.close_project();
                } else if let Some(req) = ws.tutorial.as_deref_mut().and_then(|t| t.request.take()) {
                    self.tutorial_request(req, ui.ctx());
                }
            }
            None => {
                self.landing.set_access(self.folder.state());
                if self.landing.tutorials_done != self.settings.tutorials_done {
                    self.landing.tutorials_done = self.settings.tutorials_done.clone();
                }
                match self.landing.ui(ui) {
                    Some(LandingAction::Open(dir)) => self.open_checked(dir),
                    Some(LandingAction::New(dir)) => self.new_project_in(dir),
                    Some(LandingAction::Import) => self.import_dialog(),
                    Some(LandingAction::Tutorial(id)) => self.start_tutorial(id, 0),
                    Some(LandingAction::NavigationPresets) => {
                        self.home_prefs = Some(0);
                        self.home_nav = Some(self.settings.preset().name);
                    }
                    Some(LandingAction::Shortcuts) => {
                        self.new_project();
                        if let Some(ws) = self.ws.as_deref_mut() {
                            ws.ui_action(crate::workspace::toolbar::UiAction::Shortcuts);
                        }
                    }
                    None => {}
                }
                self.home_preferences(ui);
                self.ask_open_dialog(ui);
            }
        }
        // On top of the bar the screen just drew; the workspace and the home screen both fill the window.
        let menus_at = self.ws.as_deref().map_or(crate::landing::MENUS_AT, |ws| ws.menus_at);
        if let Some(bar) = self.menubar.as_mut() {
            bar.show(ui, menus_at);
        }
        crate::window_chrome::show(ui);
        if self.updating {
            let full = ctx.content_rect();
            for action in crate::updates::show(ui, full, &mut self.updates) {
                self.update_action(&ctx, action);
            }
        }
        if self.frame_work.len() < 100_000 {
            let view = self.ws.as_deref().map_or(0.0, |ws| ws.view_ms);
            self.frame_work.push((began.elapsed().as_secs_f32() * 1000.0, view));
        }
        // A tablet asks its own question once: pencil, fingers or keyboard, then shows that one's controls.
        if crate::tablet::on_tablet() {
            if (self.settings.tablet_device.is_empty() || self.ask_device)
                && let Some(i) = crate::onboarding::tablet(ui, &mut self.tablet_choice)
            {
                self.settings.tablet_device = crate::onboarding::TABLET_DEVICES[i].1.to_string();
                self.ask_device = false;
                self.tablet_choice = None;
                self.sync_settings();
            }
        // First run: which device the model is worked with (picks the navigation preset).
        } else if ((self.settings_file.is_some() && self.settings.device.is_empty()) || self.ask_device)
            && let Some(i) = crate::onboarding::show(ui)
        {
            let (_, value, preset) = crate::onboarding::DEVICES[i];
            self.settings.device = value.to_string();
            self.settings.nav_preset = preset.to_string();
            self.ask_device = false;
            self.welcome_nav = Some(self.settings.preset().name);
            self.sync_settings();
        } else if self.welcome_nav.is_none()
            && !self.ask_device
            && self.settings_file.is_some()
            && !self.settings.device.is_empty()
            && !self.settings.nav_asked
        {
            // Installs from before the second question get it once.
            self.welcome_nav = Some(self.settings.preset().name);
        }
        // …then which app's navigation feels like home.
        if self.welcome_nav.is_some() {
            let full = ctx.content_rect();
            let mut nav = self.welcome_nav;
            let mut s = self.settings.clone();
            egui::Area::new(ui.scope_id().with("welcome-nav")).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
                let _ = ui.allocate_rect(full, egui::Sense::click());
                crate::workspace::prefs::presets_window(ui, full, &mut nav, &mut s, true);
            });
            self.welcome_nav = nav;
            if nav.is_none() {
                s.nav_asked = true;
            }
            if s != self.settings {
                self.settings = s;
                self.sync_settings();
            }
        }
        // …then how it looks, where projects live and where they're backed up (asked once, installs from before too).
        if self.setup.is_none()
            && self.welcome_nav.is_none()
            && !self.ask_device
            && self.settings_file.is_some()
            && !self.settings.device.is_empty()
            && self.settings.nav_asked
            && !self.settings.setup_done
        {
            self.setup = Some(crate::setup::Setup::new(&self.settings));
        }
        if let Some(mut st) = self.setup.take() {
            let detector = crate::cloud::detector();
            if detector.found().is_none() && !detector.busy() {
                let ctx = ctx.clone();
                detector.refresh(move || ctx.request_repaint());
            }
            let before = st.theme.clone();
            let found = detector.found();
            if crate::setup::show(ui, &mut st, found.as_ref()) {
                self.settings.theme = st.theme.clone();
                self.settings.backups = st.backups.clone();
                self.settings.backup_after_change = st.after_change;
                self.settings.setup_done = true;
                if let Some(to) = st.work.clone() {
                    crate::backup::request_switch(to, true);
                }
                self.sync_settings();
            } else {
                // The look changes as it's picked.
                if st.theme != before {
                    self.settings.theme = st.theme.clone();
                    self.sync_settings();
                }
                self.setup = Some(st);
            }
        }
    }

    /// The first-run questions (device, then navigation style) are showing or about to.
    fn first_run_open(&self) -> bool {
        if crate::tablet::on_tablet() {
            return self.ask_device || self.settings.tablet_device.is_empty();
        }
        let has_file = self.settings_file.is_some();
        self.ask_device
            || self.setup.is_some()
            || self.welcome_nav.is_some()
            || (has_file && self.settings.device.is_empty())
            || (has_file && !self.settings.nav_asked)
    }

    /// Hands the app's settings to the open workspace and saves them.
    fn sync_settings(&mut self) {
        if let Some(ws) = self.ws.as_deref_mut() {
            ws.settings = self.settings.clone();
        }
        if let Some(path) = &self.settings_file {
            let _ = self.settings.save(path);
        }
    }

    /// Preferences over the projects screen (⌘, works there too, like the reference). The workspace has its own.
    fn home_preferences(&mut self, ui: &mut egui::Ui) {
        if self.ws.is_some() {
            (self.home_prefs, self.home_nav) = (None, None);
            return;
        }
        let Some(mut tab) = self.home_prefs else { return };
        let full = ui.max_rect();
        let mut s = self.settings.clone();
        let mut nav = self.home_nav;
        egui::Area::new(ui.scope_id().with("home-prefs")).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
            let _ = ui.allocate_rect(full, egui::Sense::click());
            let out = crate::workspace::prefs::window(ui, full, &mut tab, &mut s, false);
            if out.presets {
                nav = Some(s.preset().name);
            }
            let open = nav.is_some();
            crate::workspace::prefs::presets_window(ui, full, &mut nav, &mut s, false);
            self.home_prefs = if out.done && !open { None } else { Some(tab) };
        });
        self.home_nav = nav;
        if s != self.settings {
            self.settings = s;
            if let Some(path) = &self.settings_file
                && let Err(e) = self.settings.save(path)
            {
                self.landing.toast = Some((format!("Couldn't save preferences: {e}"), f64::MAX));
            }
        }
    }

    fn handle_menu(&mut self, ctx: &egui::Context) {
        let Some(bar) = self.menubar.as_mut() else { return };
        let state = match self.ws.as_deref() {
            None => MenuState::default(),
            Some(ws) => MenuState {
                in_project: true,
                bodies: ws.session.doc.bodies.len(),
                selection: !ws.session.selection.is_empty(),
                sketching: ws.session.active_sketch.is_some(),
                undo: ws.session.can_undo(),
                redo: ws.session.can_redo(),
                grid_um: (ws.grid_size() * 1000.0).round().max(0.0) as u64,
                grid_locked: ws.grid_locked.is_some(),
                visualizing: ws.visualizing,
                drawing: ws.drawings.is_some(),
                checks: ws.menu_checks(),
            },
        };
        bar.update(state);
        for (action, key) in bar.poll() {
            // A tool's single key picked the item (macOS key equivalent): hand the key to the workspace as typed, so a
            // text field gets its letter and a running tool its own meaning of the key (E flips Sides…).
            if let Some(key) = key {
                let ch = key.name().to_lowercase();
                ctx.input_mut(|i| {
                    for pressed in [true, false] {
                        i.events.push(egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                    i.events.push(egui::Event::Text(ch));
                });
                continue;
            }
            match action {
                MenuAction::App(a) => match a {
                    AppAction::NewProject => self.new_project(),
                    AppAction::CloseProject | AppAction::GoHome => self.close_project(),
                    AppAction::Save => {
                        self.save();
                        crate::backup::backups().now();
                    }
                    AppAction::Import => self.import_dialog(),
                    AppAction::ImportProject => self.import_project_sheet(),
                    AppAction::Export => self.export_dialog(None),
                    AppAction::ExportAs(f) => self.export_dialog(Some(f)),
                    AppAction::QuickExport => self.quick_export(),
                    AppAction::Quit => {
                        if self.ws.as_ref().is_some_and(|w| w.session.dirty) {
                            self.save();
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    AppAction::CheckUpdates => shapecraft_update::Updater::global().check_now(),
                    AppAction::GetStarted | AppAction::Tutorials => {
                        self.close_project();
                        self.landing.section = if a == AppAction::Tutorials {
                            crate::landing::Section::Tutorials
                        } else {
                            crate::landing::Section::GetStarted
                        };
                    }
                    AppAction::OpenPage(url) if self.hosted => self.egui.open_url(egui::OpenUrl::new_tab(url)),
                    AppAction::OpenPage(url) => open_page(url),
                },
                MenuAction::Home(cmd) => {
                    if self.ws.is_none() {
                        let now = ctx.input(|i| i.time);
                        self.landing.command(cmd, now);
                    }
                }
                MenuAction::Ui(crate::workspace::toolbar::UiAction::Preferences) if self.ws.is_none() => {
                    self.home_prefs.get_or_insert(0);
                }
                other => {
                    let Some(ws) = self.ws.as_deref_mut() else { continue };
                    match other {
                        MenuAction::Command(id) => {
                            let _ = ws.run(id, json!({}));
                        }
                        MenuAction::Tool(kind) => ws.start_tool(kind),
                        MenuAction::Ui(u) => ws.ui_action(u),
                        MenuAction::View(v) => ws.standard_view(v),
                        MenuAction::Display(d) => ws.display = d,
                        MenuAction::Soon(label) => ws.toast(format!("{label} is coming soon")),
                        MenuAction::Variant(kind, key, i) => ws.start_variant(kind, key, i),
                        MenuAction::Home(_) => {}
                        MenuAction::App(_) => {}
                    }
                }
            }
        }
    }

    /// Opens a tutorial at a step: a fresh scratch workspace built from the step's scene, so nothing done in an
    /// earlier step (or a slip) carries on, as in the reference.
    pub(crate) fn start_tutorial(&mut self, id: &str, step: usize) {
        use crate::tutorials;
        let Some(lesson) = tutorials::lesson(id) else { return };
        let Some(def) = lesson.steps.get(step) else { return };
        if self.ws.as_ref().is_some_and(|w| w.tutorial.is_none()) {
            self.close_project();
        }
        let mut session = Session::new(self.kernel.clone(), lesson.title);
        let mut vars = std::collections::HashMap::new();
        let setup = tutorials::run(&mut session, def.setup, &mut vars);
        // Each scene starts with nothing picked (an extrude would leave its face selected).
        session.selection.clear();
        let base = tutorials::snapshot(&session);
        let mut ws = Workspace::new(session, self.settings.clone());
        ws.variants.clear();
        ws.show_items = false;
        ws.show_history = false;
        if let Err(e) = setup {
            ws.toast(format!("This step's scene didn't build: {e}"));
        }
        match def.view {
            tutorials::View::Default => {
                // An empty first step frames the size of part the lesson makes (about 10 cm).
                let (min, max) =
                    ws.session.doc.bounds().unwrap_or((glam::DVec3::new(-60.0, -45.0, 0.0), glam::DVec3::new(60.0, 45.0, 20.0)));
                let mut c = shapecraft_render::Camera::default();
                c.fit(min, max);
                ws.camera = shapecraft_render::Camera::standard(StandardView::Default, c.target, c.distance);
            }
            tutorials::View::Sketch => {
                if let Some(plane) = ws.session.active_sketch().map(|s| s.plane) {
                    ws.focus_plane_public(plane);
                }
            }
        }
        let mut run = crate::workspace::tutorial::TutorialRun::new(lesson, step, base);
        // Where the step is headed: its scene with the solution done, rendered for the corner card.
        let mut goal = Session::new(self.kernel.clone(), lesson.title);
        let mut gvars = std::collections::HashMap::new();
        if tutorials::run(&mut goal, def.setup, &mut gvars).and_then(|_| tutorials::run(&mut goal, def.solution, &mut gvars)).is_ok() {
            goal.selection.clear();
            run.picture = picture(&mut ws, &goal.doc, (480, 330)).map(|(w, h, px)| {
                let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
                self.egui.load_texture(format!("tutorial-{id}-{step}"), image, egui::TextureOptions::LINEAR)
            });
        }
        ws.tutorial = Some(Box::new(run));
        self.ws = Some(Box::new(ws));
        self.saved_generation = 0;
    }

    fn tutorial_request(&mut self, req: crate::workspace::tutorial::Request, _ctx: &egui::Context) {
        use crate::workspace::tutorial::Request;
        let Some((id, step, steps)) =
            self.ws.as_deref().and_then(|w| w.tutorial.as_deref()).map(|t| (t.lesson.id, t.step, t.lesson.steps.len()))
        else {
            return;
        };
        match req {
            Request::Retry => self.start_tutorial(id, step),
            Request::Next if step + 1 < steps => self.start_tutorial(id, step + 1),
            Request::Next | Request::Quit => {
                if req == Request::Next && !self.settings.tutorials_done.iter().any(|d| d == id) {
                    self.settings.tutorials_done.push(id.to_string());
                    self.save_settings();
                }
                self.ws = None;
                self.landing.show_tutorials();
                if req == Request::Next {
                    self.landing.toast = Some(("Tutorial complete. Nicely done!".into(), f64::MAX));
                }
            }
        }
    }

    fn save_settings(&mut self) {
        if let Some(path) = &self.settings_file {
            let _ = self.settings.save(path);
        }
    }

    /// The settings in force: the workspace keeps its own copy while a project is open (the frame copies it back).
    fn settings_now(&mut self) -> &mut crate::settings::Settings {
        match self.ws.as_deref_mut() {
            Some(ws) => &mut ws.settings,
            None => &mut self.settings,
        }
    }

    fn toast(&mut self, text: String) {
        match self.ws.as_deref_mut() {
            Some(ws) => ws.toast(text),
            None => self.landing.toast = Some((text, f64::MAX)),
        }
    }

    /// Installs the ready update as ShapeCraft closes (the helper waits for it to exit), saving first.
    fn install_update(&mut self, ctx: &egui::Context, restart: bool) -> bool {
        if self.ws.as_ref().is_some_and(|w| w.session.dirty) {
            self.save();
        }
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        match shapecraft_update::Updater::global().apply(restart, &args) {
            Ok(()) => {
                self.updates.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                true
            }
            Err(e) => {
                self.toast(format!("Couldn't install the update: {e}"));
                false
            }
        }
    }

    fn update_action(&mut self, ctx: &egui::Context, action: crate::updates::Action) {
        use crate::updates::Action;
        match action {
            Action::Install { restart } => {
                self.install_update(ctx, restart);
            }
            Action::CloseAnyway => {
                self.updates.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Skip(version) => {
                self.settings_now().update_skip = version;
                shapecraft_update::Updater::global().skip();
                if self.ws.is_none() {
                    self.save_settings();
                }
            }
            Action::AlwaysOnClose => {
                self.settings_now().update_on_close = crate::updates::ON_CLOSE[1].to_string();
                if self.ws.is_none() {
                    self.save_settings();
                }
            }
            Action::OpenPage(url) => open_page(&url),
        }
    }

    /// Every frame, shown or not: automatic checks, and the question when closing with an update downloaded.
    fn update_logic(&mut self, ctx: &egui::Context) {
        if !self.updating {
            return;
        }
        let updater = shapecraft_update::Updater::global();
        if !std::mem::replace(&mut self.updates.told_failure, true)
            && let Some(why) = updater.take_failure()
        {
            self.toast(format!("The last update didn't install ({why}). ShapeCraft will try again."));
        }
        let prefs = crate::updates::prefs(self.ws.as_deref().map_or(&self.settings, |w| &w.settings));
        updater.tick(&prefs);
        // An idle window draws no frames, so ask for one when the next automatic check could be due.
        ctx.request_repaint_after(std::time::Duration::from_secs(if updater.snapshot().checked_ago.is_some() { 300 } else { 5 }));
        if !ctx.input(|i| i.viewport().close_requested()) || self.updates.closing || !updater.ready() {
            return;
        }
        let always = self.ws.as_deref().map_or(&self.settings, |w| &w.settings).update_on_close == crate::updates::ON_CLOSE[1];
        if always {
            // Installs on the way out; the window closes as asked.
            if self.ws.as_ref().is_some_and(|w| w.session.dirty) {
                self.save();
            }
            let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
            let _ = updater.apply(false, &args);
            self.updates.closing = true;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.updates.asking = true;
            ctx.request_repaint();
        }
    }
}

/// A document rendered on its own, see-through around the model (unmixed from black and white backgrounds), at the
/// default view fitted to it: project thumbnails and the tutorial's pictures.
fn picture(ws: &mut Workspace, doc: &shapecraft_doc::Document, px: (u32, u32)) -> Option<(u32, u32, Vec<u8>)> {
    let (min, max) = doc.bounds()?;
    let mut c = shapecraft_render::Camera::default();
    c.fit(min, max);
    let camera = shapecraft_render::Camera::standard(StandardView::Default, c.target, c.distance * 0.92);
    let mut over = |background: u32| {
        let scene = SceneInput {
            doc,
            selection: &[],
            hover: None,
            active_sketch: None,
            plane_tiles: &[],
            overlays: &[],
            colors: SceneColors { background: shapecraft_render::Rgba::hex(background), ..SceneColors::default() },
            show_axes: false,
            show_edges: true,
            show_hidden_edges: false,
            show_decals: true,
            grid: None,
            display: Default::default(),
            section: None,
            presentation: false,
            backdrop: shapecraft_render::Rgba::hex(0x101010),
            shadow: 0.0,
            ghosts: &[],
            tints: &[],
            drawn: &[],
            defined: &[],
        };
        ws.gpu.thumbnail(&scene, &camera, px)
    };
    let (w, h, black) = over(0x000000)?;
    let (_, _, white) = over(0xffffff)?;
    Some((w, h, crate::workspace::unmix(&black, &white)))
}

/// The project card's picture: see-through around the model (rendered over black and over white, then unmixed),
/// so the card shows it on either theme's background.
fn write_thumbnail(ws: &mut Workspace, dir: &std::path::Path) {
    let camera = ws.thumbnail_camera();
    let mut over = |background: u32| {
        let scene = SceneInput {
            doc: &ws.session.doc,
            selection: &[],
            hover: None,
            active_sketch: None,
            plane_tiles: &[],
            overlays: &[],
            colors: SceneColors { background: shapecraft_render::Rgba::hex(background), ..SceneColors::default() },
            show_axes: false,
            show_edges: true,
            show_hidden_edges: false,
            show_decals: true,
            grid: None,
            display: Default::default(),
            section: None,
            presentation: false,
            backdrop: shapecraft_render::Rgba::hex(0x101010),
            shadow: 0.0,
            ghosts: &[],
            tints: &[],
            drawn: &[],
            defined: &[],
        };
        ws.gpu.thumbnail(&scene, &camera, (400, 296))
    };
    if let (Some((w, h, black)), Some((_, _, white))) = (over(0x000000), over(0xffffff)) {
        let mut px = crate::workspace::unmix(&black, &white);
        // A project of only sketches is hairlines, which unmix to near see-through and vanish on a light card: make
        // their coverage solid (bodies keep their measured look).
        if ws.session.doc.bodies.is_empty() {
            for a in px.iter_mut().skip(3).step_by(4) {
                let t = 1.0 - *a as f32 / 255.0;
                *a = ((1.0 - t * t * t) * 255.0).round() as u8;
            }
        }
        let _ = write_rgba(&dir.join(project::THUMBNAIL_FILE), w, h, &px);
    }
}

pub fn write_rgba(path: &std::path::Path, w: u32, h: u32, px: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(px).map_err(|e| e.to_string())
}

pub fn write_color_image(image: &egui::ColorImage, path: &std::path::Path) -> Result<(), String> {
    let px: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    write_rgba(path, image.size[0] as u32, image.size[1] as u32, &px)
}

impl eframe::App for ShapeCraftApp {
    /// What must go on with the window hidden too, where `ui` doesn't run: a minimized window, or one the compositor has
    /// stopped giving frames to, still answers the control channel and still saves.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.sync_tablet();
        self.handle_control(ctx);
        self.autosave(ctx);
        self.update_logic(ctx);
        // The project's name, as the reference titles its window ("Untitled Project" in Window ▸).
        let title = self.ws.as_ref().map_or("ShapeCraft", |ws| ws.session.doc.name.as_str());
        let title = if title.trim().is_empty() { "ShapeCraft" } else { title };
        if title != self.window_title {
            self.window_title = title.to_string();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title.clone()));
        }
        // A screenshot comes back on a later poll of the device, which an idle app would never make.
        if let Some((waiting, since)) = &self.capture {
            let late = since.elapsed().as_secs() >= 5;
            let answered = waiting.lock().map_or(true, |mut w| {
                if late && let Some(reply) = w.take() {
                    let _ = reply.send(Err("the window isn't being drawn (minimized, or the display is off)".to_string()));
                }
                w.is_none()
            });
            if answered {
                self.capture = None;
            } else {
                if let Some(state) = frame.wgpu_render_state() {
                    let _ = state.device.poll(wgpu::PollType::Poll);
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let rs = frame.wgpu_render_state().cloned();
        self.frame(ui, rs.as_ref());
        if let Some(url) = crate::landing::take_tablet_link() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }

    fn on_exit(&mut self) {
        if self.ws.as_ref().is_some_and(|w| w.session.dirty) {
            self.save();
        }
        // The last backup of the session: what was saved since the last one (a backup out of reach doesn't hold
        // the app open; the next start catches up).
        crate::backup::backups().flush(std::time::Duration::from_secs(15));
        if let Some((dir, _)) = self.marked.take() {
            project::mark_closed(&dir, &crate::backup::computer_name());
        }
    }
}

/// Brings a window that came up too big for its screen, or hanging off it, fully into view (a high-DPI laptop has far
/// fewer points than pixels), leaving room for the title bar and taskbar. False until the system has said how big things
/// are. New windows open on the main screen, the one with the origin the position is measured from.
fn fit_window(ctx: &egui::Context) -> bool {
    let (screen, inner, outer) = ctx.input(|i| (i.viewport().monitor_size, i.viewport().inner_rect, i.viewport().outer_rect));
    let (Some(screen), Some(inner), Some(outer)) = (screen, inner, outer) else { return false };
    let room = egui::vec2(screen.x * 0.9, screen.y * 0.85);
    let size = inner.size().min(room);
    if size != inner.size() || outer.right() > screen.x || outer.bottom() > screen.y {
        let chrome = outer.size() - inner.size();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(((screen - size - chrome) / 2.0).max(egui::Vec2::ZERO).to_pos2()));
    }
    true
}

/// Opens a file in an app: by name on macOS (`open -a`), else its command on the PATH.
/// Opens a web page in the default browser.
fn open_page(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(url).spawn();
}
