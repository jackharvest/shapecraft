//! The projects screen. Same shape as the reference app's, but it browses a folder on your disk.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, pos2, vec2};
use shapecraft_doc::project::{self, ProjectSummary};

use crate::glide::Glide;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, hex, radius};
use crate::window_chrome;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    GetStarted,
    Tutorials,
    Recents,
    All,
    /// A phone's page for one of the computer's menus (File, Help): what the menu bar holds elsewhere, as rows.
    Menu(&'static str),
}

/// What the home screen's menu bar items ask for (File, Go and View on the home screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeCmd {
    NewFolder,
    /// The selected project or folder, moved into a new folder in its place (⌥⌘N).
    NewFolderWithSelection,
    Duplicate,
    Back,
    Forward,
    EnclosingFolder,
    Recents,
    Projects,
    /// Sort By: 0 date newest first, 1 date oldest first, 2 name A–Z, 3 name Z–A.
    Sort(u8),
    Search,
    /// Edit ▸ Rename (⌘R): the selected tile's name, edited in place.
    Rename,
}

pub enum LandingAction {
    Open(PathBuf),
    /// A new project in this folder.
    New(PathBuf),
    Import,
    /// Start an in-app tutorial (its id).
    Tutorial(&'static str),
    NavigationPresets,
    Shortcuts,
    /// A row of a phone's menu page (Help ▸ Report a Problem or Idea…, File ▸ New Folder …), done as the menu
    /// bar's item would be; also Settings from the sections menu.
    Menu(crate::menubar::MenuAction),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Date,
    Name,
}

/// A backup, browsable from the sidebar (Preferences ▸ Sync): read-only here, projects come back with Restore.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub label: String,
    /// Its ShapeCraft Backups folder (every computer's copies).
    pub dir: PathBuf,
    pub cloud: bool,
}

/// A tile's right-click menu target.
#[derive(Clone, PartialEq)]
enum Tile {
    Project(PathBuf),
    Folder(PathBuf),
}

pub struct Landing {
    pub root: PathBuf,
    pub section: Section,
    pub search: String,
    /// The folder All Projects is showing, and the Back / Forward history.
    folder: PathBuf,
    /// Where Back and Forward go: the page (section) and folder, as a browser keeps them.
    back: Vec<(Section, PathBuf)>,
    forward: Vec<(Section, PathBuf)>,
    projects: Vec<ProjectSummary>,
    folders: Vec<PathBuf>,
    scanned: Option<f64>,
    thumbs: HashMap<PathBuf, (SystemTime, TextureHandle)>,
    pub toast: Option<(String, f64)>,
    sort: Sort,
    /// Sort By's reversed orders (date oldest first, name Z to A).
    sort_reversed: bool,
    /// View ▸ Search: the search field takes the keyboard next frame.
    focus_search: bool,
    sort_menu: Option<egui::Pos2>,
    /// When the Sort menu opened: the tap that opened it isn't a tap outside it (its first frame has no place yet).
    sort_opened: f64,
    menu: Option<(Tile, egui::Pos2, f64)>,
    /// A tile being renamed in place: its path, the text, and whether to grab focus.
    renaming: Option<(PathBuf, String, bool)>,
    /// The tile a single click picked (a double-click opens it), as observed (L13).
    selected: Option<PathBuf>,
    /// The last click was in the sidebar (its selected row shows blue).
    sidebar_focus: bool,
    /// The tutorials finished (ids), from the settings.
    pub tutorials_done: Vec<String>,
    art: HashMap<String, TextureHandle>,
    /// The What's new row, scrolled sideways (a phone's glides; see `glide`).
    row: Glide,
    whats_new_open: Option<usize>,
    /// A phone's page, scrolled down (its cards don't fit the short screen), and how far it can go (as of the last
    /// frame's layout).
    page: Glide,
    page_max: f32,
    /// Which way the finger on a phone's page is scrolling: sideways (the What's new row) or not, settled by its
    /// first move and kept until it lifts (egui forgets where a press began on the frame it ends).
    drag_sideways: Option<bool>,
    /// The folders as a tree down the left (Sort menu ▸ Folder Tree; the app keeps it in the settings).
    pub tree: bool,
    /// The tree's open folders, and each folder's folders as last read.
    expanded: HashSet<PathBuf>,
    tree_kids: HashMap<PathBuf, Vec<PathBuf>>,
    /// The folder the tree last opened its way down to.
    tree_seen: PathBuf,
    /// A phone's sections menu (it has no sidebar), where it opens and when.
    nav_menu: Option<(egui::Pos2, f64)>,
    /// Whether the projects folder can be read yet (the app checks it off the UI thread; see `folder_access`).
    access: crate::folder_access::Access,
    /// The backups, in the sidebar under the projects.
    places: Vec<Place>,
}

/// Where the in-window menus start on this screen (Linux, Windows): the top of the sidebar, free without window buttons.
pub const MENUS_AT: egui::Pos2 = egui::pos2(10.0, 25.0);

impl Landing {
    pub fn new(root: PathBuf) -> Self {
        Self {
            folder: root.clone(),
            root,
            selected: None,
            section: Section::Recents,
            search: String::new(),
            back: Vec::new(),
            forward: Vec::new(),
            projects: Vec::new(),
            folders: Vec::new(),
            scanned: None,
            thumbs: HashMap::new(),
            toast: None,
            sort: Sort::Date,
            sort_reversed: false,
            focus_search: false,
            sort_menu: None,
            sort_opened: 0.0,
            menu: None,
            renaming: None,
            sidebar_focus: false,
            tutorials_done: Vec::new(),
            art: HashMap::new(),
            row: Glide::default(),
            whats_new_open: None,
            page: Glide::default(),
            page_max: 0.0,
            drag_sideways: None,
            nav_menu: None,
            tree: false,
            expanded: HashSet::new(),
            tree_kids: HashMap::new(),
            tree_seen: PathBuf::new(),
            access: crate::folder_access::Access::Ready(PathBuf::new()),
            places: Vec::new(),
        }
    }

    /// The projects folder's state, every frame. Once it's ready the list is read (from the folder actually in use:
    /// the old Shayper3D one when it couldn't be moved).
    pub fn set_access(&mut self, access: crate::folder_access::Access) {
        if access == self.access {
            return;
        }
        if let crate::folder_access::Access::Ready(root) = &access
            && !root.as_os_str().is_empty()
            && *root != self.root
        {
            self.root = root.clone();
            self.folder = root.clone();
        }
        self.access = access;
        self.rescan();
    }

    pub fn rescan(&mut self) {
        self.scanned = None;
        self.tree_kids.clear();
    }

    /// The backups (from Preferences ▸ Sync), every frame.
    pub fn set_places(&mut self, places: Vec<Place>) {
        if places != self.places {
            self.places = places;
            self.rescan();
        }
    }

    /// The Work Folder, then each backup's folder.
    fn roots(&self) -> Vec<PathBuf> {
        std::iter::once(self.root.clone()).chain(self.places.iter().map(|p| p.dir.clone())).collect()
    }

    /// The backup `path` is in, if it's in one (the innermost folder wins, should one hold another).
    /// Where a backup keeps a project's older versions (the first backup that has any).
    fn versions_of(&self, project: &Path) -> Option<PathBuf> {
        let rel = project.strip_prefix(&self.root).ok()?;
        self.places.iter().find_map(|p| {
            [p.dir.join(crate::backup::computer_name()), p.dir.clone()]
                .into_iter()
                .map(|base| base.join(crate::backup::OLDER).join(rel))
                .find(|d| std::fs::read_dir(d).is_ok_and(|mut r| r.next().is_some()))
        })
    }

    fn backup_of(&self, path: &Path) -> Option<&Place> {
        let inner = self.roots().into_iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.components().count())?;
        self.places.iter().find(|p| p.dir == inner)
    }

    /// Switches to another Work Folder (Preferences ▸ Sync).
    pub fn set_root(&mut self, root: PathBuf) {
        if root != self.root {
            self.root = root.clone();
            self.back.clear();
            self.forward.clear();
            self.folder = root;
            self.selected = None;
            self.rescan();
        }
    }

    /// Where new projects go when they aren't made in a folder: the Work Folder.
    pub fn default_dir(&self) -> PathBuf {
        self.root.clone()
    }

    /// Every project in the Work Folder, newest first (Import Project).
    pub fn all_projects(&self) -> Vec<ProjectSummary> {
        project::list_all(&self.root)
    }

    /// Copies a project or folder from a backup into the Work Folder (beside what's there, never over it).
    fn restore(&mut self, item: &Path, now: f64) -> Option<PathBuf> {
        let r = project::copy_to(item, &self.root);
        let to = self.report(now, r)?;
        let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.toast = Some((format!("Restored to your Work Folder as {name}"), now + 4.0));
        Some(to)
    }

    /// Opens a project; one in a backup is restored first (a backup's copies aren't edited in place).
    fn open_tile(&mut self, dir: PathBuf, now: f64) -> Option<LandingAction> {
        if self.backup_of(&dir).is_some() { self.restore(&dir, now).map(LandingAction::Open) } else { Some(LandingAction::Open(dir)) }
    }

    fn refresh(&mut self, now: f64) {
        if self.scanned.is_some_and(|t| now - t < 2.0) || !matches!(self.access, crate::folder_access::Access::Ready(_)) {
            return;
        }
        if !self.roots().iter().any(|r| self.folder.starts_with(r)) || !self.folder.is_dir() {
            self.folder = self.root.clone();
        }
        let browsing = self.section == Section::All && self.search.is_empty();
        self.projects = if browsing { project::list(&self.folder) } else { project::list_all(&self.root) };
        self.folders = if browsing { project::folders(&self.folder) } else { Vec::new() };
        if self.sort == Sort::Name {
            self.projects.sort_by_key(|p| p.name.to_lowercase());
        }
        if self.sort_reversed {
            self.projects.reverse();
            if self.sort == Sort::Name {
                self.folders.reverse();
            }
        }
        self.scanned = Some(now);
    }

    /// Where a new project or folder goes: the folder being browsed, else the root (never into a backup).
    pub fn target_dir(&self) -> PathBuf {
        if self.section == Section::All && self.backup_of(&self.folder).is_none() { self.folder.clone() } else { self.root.clone() }
    }

    /// A home-screen menu bar item.
    pub fn command(&mut self, cmd: HomeCmd, now: f64) {
        let in_backup = self.section == Section::All && self.backup_of(&self.folder).is_some();
        if in_backup && matches!(cmd, HomeCmd::NewFolderWithSelection | HomeCmd::Duplicate | HomeCmd::Rename) {
            self.toast = Some(("Backups aren't changed here: Restore it to your Work Folder first".into(), now + 4.0));
            return;
        }
        match cmd {
            HomeCmd::NewFolder => {
                if self.section != Section::All || in_backup {
                    let root = self.root.clone();
                    self.go_to(root);
                }
                let parent = self.folder.clone();
                if let Some(dir) = self.report(now, project::new_folder(&parent)) {
                    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.renaming = Some((dir, name, true));
                }
            }
            HomeCmd::NewFolderWithSelection => match self.selected.clone() {
                // As the reference: only where the project actually sits, not in Recents.
                Some(item) if self.section == Section::All => {
                    if let Some(dir) = self.report(now, project::new_folder_with(&item)) {
                        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        self.selected = Some(dir.clone());
                        self.renaming = Some((dir, name, true));
                    }
                }
                Some(_) => self.toast = Some(("Open Projects to put this in a new folder".into(), now + 3.0)),
                None => self.toast = Some(("Select a project or folder to put in a new folder".into(), now + 3.0)),
            },
            HomeCmd::Rename => match self.selected.clone() {
                Some(dir) => {
                    let name = match self.projects.iter().find(|p| p.dir == dir) {
                        Some(p) => Some(p.name.clone()),
                        None => dir.file_name().map(|n| n.to_string_lossy().into_owned()),
                    };
                    self.renaming = Some((dir, name.unwrap_or_default(), true));
                }
                None => self.toast = Some(("Select a project or folder to rename".into(), now + 3.0)),
            },
            HomeCmd::Duplicate => match self.selected.clone() {
                Some(dir) if self.projects.iter().any(|p| p.dir == dir) => {
                    let r = project::duplicate(&dir);
                    self.report(now, r);
                }
                _ => self.toast = Some(("Select a project to duplicate".into(), now + 3.0)),
            },
            HomeCmd::Back => self.go_back(),
            HomeCmd::Forward => self.go_forward(),
            HomeCmd::EnclosingFolder => {
                if self.section == Section::All
                    && !self.roots().contains(&self.folder)
                    && let Some(up) = self.folder.parent().map(Path::to_path_buf)
                {
                    self.go_to(up);
                }
            }
            HomeCmd::Recents => self.visit(Section::Recents, self.folder.clone()),
            HomeCmd::Projects => self.visit(Section::All, self.folder.clone()),
            HomeCmd::Sort(i) => {
                self.sort = if i < 2 { Sort::Date } else { Sort::Name };
                self.sort_reversed = i % 2 == 1;
                self.rescan();
            }
            HomeCmd::Search => self.focus_search = true,
        }
    }

    /// A listed project's name (what it was renamed to, not its folder's name).
    pub fn project_name(&self, dir: &std::path::Path) -> Option<String> {
        self.projects.iter().find(|p| p.dir == dir).map(|p| p.name.clone())
    }

    fn go_to(&mut self, folder: PathBuf) {
        self.visit(Section::All, folder);
    }

    /// Goes to a page (and folder), leaving where it was for Back: every page counts, not only folders.
    fn visit(&mut self, section: Section, folder: PathBuf) {
        if (section, &folder) != (self.section, &self.folder) {
            self.back.push((self.section, std::mem::replace(&mut self.folder, folder)));
            self.forward.clear();
        }
        self.section = section;
        self.arrive();
    }

    fn go_back(&mut self) {
        if let Some((section, folder)) = self.back.pop() {
            self.forward.push((std::mem::replace(&mut self.section, section), std::mem::replace(&mut self.folder, folder)));
            self.arrive();
        }
    }

    fn go_forward(&mut self) {
        if let Some((section, folder)) = self.forward.pop() {
            self.back.push((std::mem::replace(&mut self.section, section), std::mem::replace(&mut self.folder, folder)));
            self.arrive();
        }
    }

    /// A new page: from its top, read afresh.
    fn arrive(&mut self) {
        self.search.clear();
        self.page.reset(0.0);
        self.rescan();
    }

    fn report<T>(&mut self, now: f64, r: Result<T, project::ProjectError>) -> Option<T> {
        match r {
            Ok(v) => {
                self.rescan();
                Some(v)
            }
            Err(e) => {
                self.toast = Some((e.to_string(), now + 4.0));
                None
            }
        }
    }

    fn thumbnail(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureId> {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
        if let Some((t, tex)) = self.thumbs.get(path)
            && *t == modified
        {
            return Some(tex.id());
        }
        let image = load_png(path)?;
        let tex = ctx.load_texture(path.to_string_lossy(), image, egui::TextureOptions::LINEAR);
        let id = tex.id();
        self.thumbs.insert(path.to_path_buf(), (modified, tex));
        Some(id)
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Option<LandingAction> {
        let now = ui.input(|i| i.time);
        self.refresh(now);
        let full = ui.max_rect();
        let p = ui.painter().clone();
        // Measured (s23 landing): #1E1E1E (light #FFFFFF) under a #373636 bar. It reaches under a phone's or
        // tablet's status bar and home bar (outside the safe area `full` keeps to); on a computer they're the same.
        let screen = ui.ctx().viewport_rect();
        let under = egui::Painter::new(ui.ctx().clone(), ui.layer_id(), screen);
        under.rect_filled(screen, 0.0, theme::c(0x1e1e1e));
        let mut action = None;

        // Sidebar (measured, s30-011/012 Light, D-landing Dark). The macOS window it sits in greys its sidebar,
        // title strip and selection when it isn't the key window, and the selected row is blue only while the
        // sidebar has the keyboard (the last click was in it).
        let focused = ui.input(|i| i.focused);
        let light = theme::is_light();
        let local = "All Projects";
        // A phone on its side (the iPhone Duo) has no room for the sidebar: its sections are in a menu, the search
        // in the title strip.
        let phone = crate::tablet::small_screen();
        // Upright (the iPhone Duo closed): the search and buttons take a second row under the status corner.
        let narrow = phone && full.width() < full.height();
        // The title strip's height; a phone's has no fill, its page shows through.
        let head = if narrow { 132.0 } else { 51.0 };
        let side = Rect::from_min_size(full.min, vec2(if phone { 0.0 } else { 250.0 }, full.height()));
        if !phone {
            if ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| p.x > full.left() + 250.0)) {
                self.sidebar_focus = false;
            }
            let side_fill = match (light, focused) {
                (true, true) => hex(0xe1e0e0),
                (true, false) => hex(0xe8e7e7),
                (false, _) => Tokens::landing_sidebar(),
            };
            under.rect_filled(Rect::from_min_max(pos2(screen.left(), screen.top()), pos2(side.right(), screen.bottom())), 0.0, side_fill);
            p.line_segment([side.right_top(), side.right_bottom()], Stroke::new(1.0, if light { hex(0xc2c0c0) } else { hex(0x000000) }));
            // The team row: a grey square with a white letter, the name, and a short line under it.
            let badge = Rect::from_min_size(pos2(side.left() + 16.0, side.top() + 59.0), vec2(32.0, 32.0));
            p.rect_filled(badge, radius(6), hex(0x808080));
            p.text(badge.center(), Align2::CENTER_CENTER, "S", theme::strong(17.0), Color32::WHITE);
            p.text(
                pos2(badge.right() + 9.0, badge.top() + 8.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s("ShapeCraft"),
                theme::strong(13.5),
                Tokens::text_strong(),
            );
            let folder = self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| shorten_home(&self.root));
            let mut job = egui::text::LayoutJob::simple_singleline(
                format!("{} · {folder}", crate::i18n::tr("Local")),
                theme::body(10.0),
                Tokens::text_dim(),
            );
            job.wrap = egui::text::TextWrapping {
                max_width: side.right() - badge.right() - 21.0,
                max_rows: 1,
                break_anywhere: true,
                overflow_character: Some('…'),
            };
            let g = ui.fonts_mut(|f| f.layout_job(job));
            p.galley(pos2(badge.right() + 9.0, badge.top() + 24.0 - g.size().y / 2.0), g, Tokens::text_dim());
            // The projects folder sits in the last slot, so the spacing stays.
            let folder_button =
                Rect::from_min_max(pos2(side.left() + 10.0, side.top() + 101.0), pos2(side.left() + 240.0, side.top() + 121.0));
            let fb = ui.interact(folder_button, ui.scope_id().with("projects-folder"), Sense::click());
            let fb_fill = match (light, fb.hovered()) {
                (true, false) => hex(0xcac9c9),
                (true, true) => hex(0xbebdbd),
                (false, false) => hex(0x5c5c5c),
                (false, true) => hex(0x676767),
            };
            p.rect_filled(folder_button, radius(5), fb_fill);
            p.text(
                folder_button.center(),
                Align2::CENTER_CENTER,
                crate::i18n::tr("Show Projects Folder"),
                theme::strong(12.0),
                Tokens::text_strong(),
            );
            if fb.clicked() {
                if crate::tablet::on_tablet() {
                    // The folder is on the computer serving this page, not on the tablet.
                    self.toast = Some((format!("Your projects are kept on the computer, in {}", shorten_home(&self.root)), f64::MAX));
                } else {
                    reveal(&self.root);
                }
            }

            let search_rect =
                Rect::from_min_max(pos2(side.left() + 10.0, side.top() + 138.0), pos2(side.left() + 240.0, side.top() + 164.0));
            let search_fill = match (light, focused) {
                (true, true) => hex(0xd6d5d5),
                (true, false) => hex(0xdcdbdb),
                (false, _) => theme::c(0x313030),
            };
            p.rect_filled(search_rect, radius(6), search_fill);
            if light {
                p.rect_stroke(search_rect, radius(6), Stroke::new(1.0, hex(0xcecece)), StrokeKind::Inside);
            }
            icons::paint(
                &p,
                Rect::from_center_size(pos2(search_rect.left() + 13.0, search_rect.center().y), vec2(13.0, 13.0)),
                Icon::Search,
                Tokens::text_dim(),
            );
            let field = Rect::from_min_max(
                pos2(search_rect.left() + 26.0, search_rect.top() + 4.0),
                pos2(search_rect.right() - 6.0, search_rect.bottom() - 4.0),
            );
            let searched = ui.put(
                field,
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(crate::i18n::tr("Search"))
                    .frame(egui::Frame::NONE)
                    .font(theme::body(12.0)),
            );
            if std::mem::take(&mut self.focus_search) {
                searched.request_focus();
            }
            if searched.changed() && !self.search.is_empty() && !matches!(self.section, Section::Recents | Section::All) {
                self.section = Section::Recents;
            }

            let blue = self.sidebar_focus && focused;
            let row = |ui: &mut Ui, y: f32, icon: Icon, label: &str, selected: bool, id: &str| -> bool {
                let r = Rect::from_min_size(pos2(side.left() + 10.0, y - 14.0), vec2(230.0, 28.0));
                let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
                let fill = match (selected, blue, light, focused) {
                    (true, true, ..) => Some(hex(0x1d4fff)),
                    (true, false, true, true) => Some(hex(0xc9c8c8)),
                    (true, false, true, false) => Some(hex(0xd0cfcf)),
                    (true, false, false, _) => Some(theme::c(0x3f3e3e)),
                    (false, ..) if resp.hovered() => Some(if light { hex(0xd5d4d4) } else { theme::c(0x3a3537) }),
                    _ => None,
                };
                if let Some(f) = fill {
                    ui.painter().rect_filled(r, radius(6), f);
                }
                let on_blue = selected && blue;
                icons::paint(
                    ui.painter(),
                    Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(15.0, 15.0)),
                    icon,
                    if on_blue { Color32::WHITE } else { Tokens::icon_blue() },
                );
                ui.painter().text(
                    pos2(r.left() + 33.0, r.center().y),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(label),
                    theme::body(12.5),
                    if on_blue { Color32::WHITE } else { Tokens::text() },
                );
                resp.clicked()
            };
            let searching = !self.search.is_empty();
            let here = self.backup_of(&self.folder).map(|p| p.dir.clone());
            let rows = [
                (186.0, Icon::GetStarted, "Get Started", Section::GetStarted, "get-started"),
                (215.0, Icon::Tutorials, "Tutorials", Section::Tutorials, "tutorials"),
                (266.0, Icon::Recents, "Recents", Section::Recents, "recents"),
                (294.0, Icon::Projects, local, Section::All, "all"),
            ];
            // The backups, first first, each opening its ShapeCraft Backups folder (every computer's copies).
            let mut open_place = None;
            if !self.places.is_empty() {
                p.text(
                    pos2(side.left() + 16.0, side.top() + 337.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr("Backups"),
                    theme::strong(10.5),
                    Tokens::text_faint(),
                );
            }
            for (k, place) in self.places.iter().enumerate() {
                let on = self.section == Section::All && !searching && here.as_ref() == Some(&place.dir);
                let icon = if place.cloud { Icon::Cloud } else { Icon::Folder };
                if row(ui, side.top() + 360.0 + 28.0 * k as f32, icon, &place.label, on, &format!("place-{k}")) {
                    open_place = Some(place.dir.clone());
                }
            }
            if let Some(dir) = open_place {
                self.sidebar_focus = true;
                self.search.clear();
                if dir.is_dir() {
                    self.go_to(dir);
                } else {
                    self.toast = Some(("Nothing's backed up there yet (or it can't be reached now)".into(), now + 4.0));
                }
            }
            for (y, icon, label, section, id) in rows {
                let on = self.section == section && !searching && (section != Section::All || here.is_none());
                if row(ui, side.top() + y, icon, label, on, id) {
                    self.sidebar_focus = true;
                    self.search.clear();
                    if section == Section::All {
                        let root = self.root.clone();
                        self.go_to(root);
                    } else {
                        self.visit(section, self.folder.clone());
                    }
                }
            }
            // A tablet has no menu bar to find it in.
            if crate::tablet::touch_ui()
                && row(ui, crate::tablet::above_home_strip(ui.ctx(), side.bottom()) - 26.0, Icon::Gear, "Preferences", false, "home-prefs")
            {
                action = Some(LandingAction::Menu(crate::menubar::MenuAction::Ui(crate::workspace::toolbar::UiAction::Preferences)));
            }
            p.text(
                pos2(side.left() + 16.0, side.top() + 243.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr("Projects"),
                theme::strong(10.5),
                Tokens::text_faint(),
            );
        }

        // Content: the title strip (white with no line in a key window, grey with a line otherwise, as measured).
        let content = Rect::from_min_max(pos2(side.right(), full.top()), full.max);
        let strip = Rect::from_min_max(content.min, pos2(content.right(), content.top() + head));
        let (strip_fill, strip_line) = match (light, focused) {
            (true, true) => (hex(0xffffff), None),
            (true, false) => (hex(0xe8e7e7), Some(hex(0xd9d9d9))),
            (false, _) => (theme::c(0x373636), Some(hex(0x000000))),
        };
        if !phone {
            p.rect_filled(strip, 0.0, strip_fill);
        }
        let folder_name = self.folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let title = match (self.search.is_empty(), self.section) {
            (false, _) => crate::i18n::tr_s("Search"),
            (true, Section::GetStarted) => crate::i18n::tr_s("Get Started"),
            (true, Section::Tutorials) => crate::i18n::tr_s("Tutorials"),
            (true, Section::Recents) => crate::i18n::tr_s("Recents"),
            (true, Section::All) if self.folder == self.root => crate::i18n::tr_s(local),
            (true, Section::All) if let Some(p) = self.places.iter().find(|p| p.dir == self.folder) => {
                format!("{} {}", p.label, crate::i18n::tr("Backup"))
            }
            (true, Section::All) => folder_name,
            (true, Section::Menu(title)) => title.to_string(),
        };
        // (A phone's title, sections button and back/forward are glass buttons, drawn over its page: phone_chrome.)
        if !phone {
            let chevron = |ui: &mut Ui, x: f32, icon: Icon, enabled: bool, id: &str| -> bool {
                let r = Rect::from_center_size(pos2(content.left() + x, content.top() + 25.0), vec2(24.0, 24.0));
                let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
                if enabled && resp.hovered() {
                    ui.painter().rect_filled(r, radius(6), Tokens::button_hover());
                }
                icons::paint(ui.painter(), r.shrink(5.0), icon, if enabled { Tokens::text_dim() } else { Tokens::text_faint() });
                enabled && resp.clicked()
            };
            if chevron(ui, 23.0, Icon::ChevronLeft, !self.back.is_empty(), "back") {
                self.go_back();
            }
            if chevron(ui, 54.0, Icon::ChevronRight, !self.forward.is_empty(), "forward") {
                self.go_forward();
            }
            let x = content.left() + 75.0;
            let room = crate::tablet::top_right(content.top(), content.right()) - 8.0 - x;
            let mut job = egui::text::LayoutJob::simple_singleline(title.clone(), theme::strong(13.0), Tokens::text());
            job.wrap =
                egui::text::TextWrapping { max_width: room.max(40.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
            let g = ui.fonts_mut(|f| f.layout_job(job));
            p.galley(pos2(x, content.top() + 25.0 - g.size().y / 2.0), g, Tokens::text());
        }
        if let Some(line) = strip_line.filter(|_| !phone) {
            p.line_segment(
                [pos2(content.left(), content.top() + 51.0), pos2(content.right(), content.top() + 51.0)],
                Stroke::new(1.0, line),
            );
        }
        // Clear of the window's own buttons, where it has them.
        // (And of a phone's status corner: the iPhone Duo keeps its clock in the top right.)
        let bar_right = crate::tablet::top_right(content.top(), content.right() - window_chrome::buttons_w());
        if window_chrome::own_frame() {
            let title_end = pos2(content.left() + 320.0, content.top());
            window_chrome::drag_area(ui, Rect::from_min_max(title_end, pos2(bar_right - 130.0, content.top() + 51.0)), "home-drag");
        }
        // New, Import, New Folder and Sort. A computer has them in the title strip. A phone on its side stacks them
        // down the right edge under the status corner on the iPhone Duo; upright, they sit in
        // the second row beside the search.
        let tools = [(Icon::Plus, "new"), (Icon::Import, "import"), (Icon::NewFolder, "folder"), (Icon::Sort, "sort")];
        let mut hit = [false; 4];
        let sort_at = pos2(bar_right - 190.0, content.top() + 42.0);
        // A phone's glass buttons, laid out now (the page keeps clear of them) and drawn over it at the end.
        let chrome = phone.then(|| PhoneChrome::new(ui, full, narrow, &title));
        let grid_right = chrome.as_ref().map_or(content.right(), |c| c.rail.left() - 12.0);
        if !phone {
            let tb = |ui: &mut Ui, i: usize, icon: Icon, id: &str| -> bool {
                let r = Rect::from_center_size(pos2(bar_right - 32.0 - i as f32 * 38.0, content.top() + 25.0), vec2(26.0, 26.0));
                let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, radius(6), Tokens::button_hover());
                }
                icons::paint(ui.painter(), r.shrink(5.0), icon, Tokens::text_dim());
                resp.clicked()
            };
            for (k, (icon, id)) in tools.iter().enumerate() {
                hit[k] = tb(ui, 3 - k, *icon, id);
            }
            if let Some(a) = self.tool_hits(hit, sort_at, now) {
                action = Some(a);
            }
        }

        // Folder Tree: the folders down the left of a folder's page (drawn after the cards, so it takes its drags).
        let tree_on = self.tree && self.section == Section::All && self.search.is_empty();
        let pane = tree_on.then(|| {
            Rect::from_min_max(
                pos2(content.left(), content.top() + if phone { head + 1.0 } else { 52.0 }),
                pos2(content.left() + if narrow { 160.0 } else { 220.0 }, full.bottom()),
            )
        });
        let base = pane.map_or(content.left(), |r| r.right());
        let left = base + if phone { 20.0 } else { 33.0 };
        let mut y = content.top() + 94.0;
        // A phone's page scrolls under the title strip: drag anywhere that isn't a card (a card takes taps, the page
        // underneath the drags), or the What's new row sideways.
        let learn = self.search.is_empty() && matches!(self.section, Section::GetStarted | Section::Tutorials);
        let p = if phone {
            // The page runs under the glass buttons (they're drawn over it, last), up to the top of the screen.
            let view = Rect::from_min_max(pos2(content.left(), screen.top()), screen.max);
            let chead = chrome.as_ref().map_or(head, |c| c.head);
            let y_now = content.top() + chead + 21.0 - self.page.offset;
            // The What's new row, where it's drawn below (its heading at y, the cards 26 under that).
            let row = Rect::from_min_max(pos2(content.left(), y_now + 26.0), pos2(screen.right(), y_now + 253.0));
            let row_on = self.search.is_empty() && self.section == Section::Recents;
            let width = crate::whats_new::ENTRIES.len() as f32 * 190.0 - 20.0;
            // The last card can come all the way out from under the buttons.
            let row_max = (width - (grid_right - left)).max(0.0);
            let drag = ui.interact(view, ui.scope_id().with("phone-scroll"), Sense::drag());
            if drag.dragged() && self.drag_sideways.is_none() {
                // Which way this drag goes is settled by its first move, so a sideways swipe stays sideways. (Not on
                // the press itself: egui starts a drag-only widget's drag there, before the finger has gone anywhere.)
                let o = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
                let at = ui.input(|i| i.pointer.interact_pos()).unwrap_or(o);
                if (at - o).length() >= 3.0 {
                    self.drag_sideways = Some(row_on && row.contains(o) && (at.x - o.x).abs() > (at.y - o.y).abs());
                }
            }
            if !drag.dragged() && !drag.drag_stopped() {
                self.drag_sideways = None;
            }
            let d = drag.drag_delta();
            if drag.dragged() && self.drag_sideways.is_some() {
                if self.drag_sideways == Some(true) {
                    self.row.drag(-d.x, row_max, row.width(), now);
                } else {
                    self.page.drag(-d.y, self.page_max, view.height(), now);
                }
            }
            if drag.drag_stopped() {
                // Let go mid-flick: the page (or the What's new row) glides on and slows, as a phone's lists do.
                if self.drag_sideways == Some(true) {
                    self.row.release(now);
                } else {
                    self.page.release(now);
                }
                self.drag_sideways = None;
            }
            if view.contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or_default()) {
                let wheel = ui.input(|i| i.smooth_scroll_delta.y);
                if wheel != 0.0 {
                    self.page.nudge(-wheel, self.page_max);
                }
            }
            let dt = ui.input(|i| i.stable_dt);
            let gliding = self.page.step(dt, self.page_max) | self.row.step(dt, row_max);
            if gliding {
                ui.ctx().request_repaint();
            }
            y = content.top() + chead + 21.0 - self.page.offset;
            ui.set_clip_rect(view);
            ui.painter().clone()
        } else {
            p
        };
        if let Section::Menu(title) = self.section
            && self.search.is_empty()
        {
            let (a, end) = self.menu_page(ui, title, y, left, grid_right, now);
            if a.is_some() {
                action = a;
            }
            if phone {
                self.clamp_scroll(end, full.bottom());
            }
            if let Some(c) = &chrome
                && let Some(a) = self.phone_chrome(ui, c, screen, now)
            {
                action = Some(a);
            }
            self.toast_and_detail(ui, content, full, now);
            return action;
        }
        if learn {
            // On a computer the page scrolls with the wheel where its cards don't all fit.
            if !phone {
                let (wheel, over, dt) =
                    ui.input(|i| (i.smooth_scroll_delta().y, i.pointer.hover_pos().is_some_and(|q| content.contains(q)), i.stable_dt));
                if over && wheel != 0.0 {
                    self.page.nudge(-wheel, self.page_max);
                }
                if self.page.step(dt, self.page_max) {
                    ui.ctx().request_repaint();
                }
                // Under the title strip: the page scrolls behind it, never over it.
                let page = Rect::from_min_max(pos2(content.left(), content.top() + head), content.max);
                ui.set_clip_rect(page.intersect(ui.clip_rect()));
            }
            let top = if phone { y - 94.0 } else { content.top() - self.page.offset };
            let (a, end) = self.learn_page(ui, top, left, if phone { grid_right } else { content.right() }, phone);
            if a.is_some() {
                action = a;
            }
            self.clamp_scroll(end, full.bottom());
            if let Some(c) = &chrome
                && let Some(a) = self.phone_chrome(ui, c, screen, now)
            {
                action = Some(a);
            }
            self.toast_and_detail(ui, content, full, now);
            return action;
        }
        if self.search.is_empty() && self.section == Section::Recents {
            p.text(pos2(left, y), Align2::LEFT_CENTER, crate::i18n::tr("What's new"), theme::strong(16.0), Tokens::text_strong());
            // (On a phone the row runs on under the buttons, and its last card can come out from under them.)
            let right = if phone { screen.right() } else { grid_right };
            let width = crate::whats_new::ENTRIES.len() as f32 * 190.0 - 20.0;
            let max = (width - if phone { grid_right - left } else { (grid_right - 33.0 - left).max(170.0) }).max(0.0);
            self.whats_new_row(ui, Rect::from_min_max(pos2(content.left(), y + 26.0), pos2(right, y + 34.0 + 219.0)), left, max);
            y += 252.0;
            p.text(
                pos2(left, y + 20.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr("Recent Projects"),
                theme::strong(16.0),
                Tokens::text_strong(),
            );
            y += 52.0;
        }

        let q = self.search.to_lowercase();
        let mut list: Vec<ProjectSummary> =
            self.projects.iter().filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q)).cloned().collect();
        if self.section == Section::Recents && q.is_empty() {
            list.truncate(15);
        }
        // Folders come first when browsing, as tiles of their own.
        // (With the tree showing, the folders are there rather than among the cards.)
        let mut tiles: Vec<Tile> = self.folders.iter().filter(|_| !tree_on).cloned().map(Tile::Folder).collect();
        tiles.extend(list.iter().map(|p| Tile::Project(p.dir.clone())));
        // Waiting for the projects folder, or not allowed in: say so rather than look empty or stuck.
        let waiting = match &self.access {
            crate::folder_access::Access::Ready(_) => None,
            crate::folder_access::Access::Pending(since) => Some((
                "Opening your projects folder…".to_string(),
                (since.elapsed() >= crate::folder_access::HINT_AFTER)
                    .then(|| "If macOS asks to let ShapeCraft use your Documents folder, choose Allow.".to_string()),
            )),
            crate::folder_access::Access::Denied(why) => Some((
                why.clone(),
                Some(if cfg!(target_os = "macos") {
                    "Turn ShapeCraft on in System Settings ▸ Privacy & Security ▸ Files & Folders, then reopen it.".to_string()
                } else {
                    "Check the folder's permissions, then reopen ShapeCraft.".to_string()
                }),
            )),
        };
        if let Some((line, hint)) = waiting {
            if matches!(self.access, crate::folder_access::Access::Pending(_)) {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            }
            let c = pos2(content.center().x, y + 120.0);
            p.text(c, Align2::CENTER_CENTER, crate::i18n::tr_s(line), theme::strong(13.0), Tokens::text());
            if let Some(hint) = hint {
                p.text(c + vec2(0.0, 20.0), Align2::CENTER_CENTER, crate::i18n::tr_s(hint), theme::body(11.0), Tokens::text_dim());
            }
        } else if tiles.is_empty() && !self.search.is_empty() {
            // As the reference words and places it, in the middle of the page (26.202, s39-refsearch).
            let c = content.center() - vec2(0.0, 10.0);
            p.text(
                c,
                Align2::CENTER_CENTER,
                crate::i18n::tr_s(format!("No results for \"{}\"", self.search)),
                theme::strong(13.0),
                Tokens::text(),
            );
            p.text(
                c + vec2(0.0, 20.0),
                Align2::CENTER_CENTER,
                crate::i18n::tr("Check the spelling or try a new search."),
                theme::body(11.0),
                Tokens::text_dim(),
            );
        } else if tiles.is_empty() {
            let c = pos2(content.center().x, y + 120.0);
            p.text(c, Align2::CENTER_CENTER, crate::i18n::tr("No projects yet"), theme::strong(13.0), Tokens::text());
            p.text(
                c + vec2(0.0, 20.0),
                Align2::CENTER_CENTER,
                crate::i18n::tr_s(if crate::tablet::shows_shortcuts() {
                    format!("Press + or {} to start one", theme::key_text("⌘N"))
                } else {
                    "Tap + to start one".to_string()
                }),
                theme::body(11.0),
                Tokens::text_dim(),
            );
        }
        // As observed (L12): as many columns as fit at 195 px or more, the cards stretched to fill the row (200 px
        // at 1840 wide, 215 at 1000), 21 px apart, pictures 0.74 as tall as wide.
        // A touch screen gets bigger cards, easier to tap: three across an iPad in landscape (measured there: cards
        // 245.5 wide, 20 apart).
        // A phone has the cards at the computer's size, three or four across the iPhone Duo on its side, two across
        // its front screen.
        let pitch = if crate::tablet::touch_ui() && !phone {
            265.0
        } else if narrow {
            170.0
        } else {
            216.0
        };
        let avail =
            grid_right - content.left() - if phone { 40.0 } else { 65.0 } - (left - content.left() - if phone { 20.0 } else { 33.0 });
        let cols = ((avail + 21.0) / pitch).floor().max(1.0) as usize;
        let card_w = ((avail - 21.0 * (cols as f32 - 1.0)) / cols as f32).max(120.0);
        let card_h = (card_w * 0.74).round();
        for (i, t) in tiles.iter().enumerate() {
            let (cx, cy) = (i % cols, i / cols);
            let tile = Rect::from_min_size(pos2(left + cx as f32 * (card_w + 21.0), y + cy as f32 * (card_h + 76.0)), vec2(card_w, card_h));
            if tile.top() > full.bottom() {
                if phone {
                    continue;
                }
                break;
            }
            let resp = ui.interact(tile, ui.scope_id().with(("tile", i)), Sense::click());
            let tile_dir = match t {
                Tile::Folder(d) | Tile::Project(d) => d.clone(),
            };
            let picked = self.selected.as_ref() == Some(&tile_dir);
            p.rect_filled(tile, radius(5), Tokens::card());
            p.rect_stroke(
                tile,
                radius(5),
                Stroke::new(1.0, if resp.hovered() { Tokens::accent() } else { Tokens::card_border() }),
                StrokeKind::Inside,
            );
            if picked {
                // A thick blue ring round the card.
                p.rect_stroke(tile.expand(1.0), radius(7), Stroke::new(3.0, theme::c(0x2d6bf5)), StrokeKind::Outside);
            }
            let (dir, name, meta) = match t {
                Tile::Folder(dir) => {
                    icons::paint(&p, Rect::from_center_size(tile.center(), vec2(64.0, 64.0)), Icon::Folder, Tokens::text_dim());
                    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    // Projects and folders inside, as the reference counts them ("1 item", "0 items"; s40-034).
                    let n = project::list(dir).len() + project::folders(dir).len();
                    (dir.clone(), name, format!("{n} item{}", if n == 1 { "" } else { "s" }))
                }
                Tile::Project(dir) => {
                    let Some(proj) = list.iter().find(|p| &p.dir == dir) else { continue };
                    if let Some(thumb) = proj.thumbnail.as_ref().and_then(|t| self.thumbnail(ui.ctx(), t)) {
                        p.image(thumb, tile.shrink(6.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    let place = match (self.backup_of(dir), proj.versions) {
                        (Some(p), _) => format!("{} {}", p.label, crate::i18n::tr("Backup")),
                        // Saved on two computers at once: the sync app kept both (right-click splits them apart).
                        (None, n) if n > 0 => format!("{} Versions", n + 1),
                        (None, _) => "Local".to_string(),
                    };
                    (dir.clone(), proj.name.clone(), format!("{} · {place}", short_date(proj.modified)))
                }
            };
            let editing = self.renaming.as_ref().is_some_and(|(d, ..)| *d == dir);
            let lines = if editing {
                self.rename_field(ui, &dir, tile, now);
                1.0
            } else {
                let name_color = if picked {
                    Color32::WHITE
                } else if name == "Untitled Project" {
                    Tokens::text_dim()
                } else {
                    Tokens::text_strong()
                };
                // At most two lines under the card, the second ending in "…" when the name runs longer.
                let fits = |t: &str| {
                    let g = p.layout(t.to_string(), theme::strong(12.5), name_color, card_w);
                    g.rows.len() <= 2 && g.size().x <= card_w + 0.5
                };
                let name = if fits(&name) {
                    name
                } else {
                    let chars: Vec<char> = name.chars().collect();
                    let cut = |n: usize| chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
                    let (mut lo, mut hi) = (0, chars.len());
                    while lo < hi {
                        let mid = (lo + hi).div_ceil(2);
                        if fits(&cut(mid)) { lo = mid } else { hi = mid - 1 }
                    }
                    cut(lo)
                };
                let g = p.layout(name.clone(), theme::strong(12.5), name_color, card_w);
                let lines = g.rows.len().min(2) as f32;
                let at = pos2(tile.center().x - g.size().x / 2.0, tile.bottom() + 10.0);
                if picked {
                    // The picked card's name sits on a blue label.
                    p.rect_filled(Rect::from_min_size(at, g.size()).expand2(vec2(5.0, 1.0)), radius(4), theme::c(0x2d6bf5));
                }
                p.galley(at, g, name_color);
                lines
            };
            p.text(
                pos2(tile.center().x, tile.bottom() + 18.0 + 16.0 * lines),
                Align2::CENTER_CENTER,
                meta,
                theme::body(10.5),
                Tokens::text_dim(),
            );
            if resp.secondary_clicked() {
                self.menu = resp.interact_pointer_pos().map(|pos| (t.clone(), pos, now));
                self.sort_menu = None;
            } else if (resp.double_clicked() || resp.clicked() && crate::tablet::touch_ui()) && !editing {
                // On a touch screen one tap opens it, as on iPad (Select picks cards there).
                match t {
                    Tile::Folder(d) => self.go_to(d.clone()),
                    Tile::Project(d) => action = self.open_tile(d.clone(), now),
                }
            } else if resp.clicked() && !editing {
                self.selected = Some(tile_dir);
            }
        }
        if phone {
            // How far it scrolls: until the last row of cards is up off the bottom.
            let rows = tiles.len().div_ceil(cols) as f32;
            let end = y + rows * (card_h + 76.0) + 16.0;
            self.clamp_scroll(end, full.bottom());
        }
        if let Some(pane) = pane {
            self.folder_tree(ui, pane);
        }
        if let Some(c) = &chrome
            && let Some(a) = self.phone_chrome(ui, c, screen, now)
        {
            action = Some(a);
        }
        if let Some(a) = self.menus(ui, now) {
            action = Some(a);
        }
        // Return opens the picked card, Escape lets it go.
        if self.renaming.is_none() && action.is_none() && !ui.ctx().egui_wants_keyboard_input() {
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if ui.input(|i| i.key_pressed(egui::Key::F2)) {
                self.command(HomeCmd::Rename, now);
            } else if esc {
                self.selected = None;
            } else if enter && let Some(d) = self.selected.clone() {
                if self.folders.contains(&d) {
                    self.go_to(d);
                } else {
                    action = self.open_tile(d, now);
                }
            }
        }

        self.toast_and_detail(ui, content, full, now);
        action
    }

    /// The toast at the bottom, and a What's new card opened over the screen.
    fn toast_and_detail(&mut self, ui: &mut Ui, content: Rect, full: Rect, now: f64) {
        let p = ui.painter().clone();
        if let Some((text, until)) = &self.toast {
            if *until < now {
                self.toast = None;
            } else {
                let g = p.layout_no_wrap(crate::i18n::tr_s(text.clone()), theme::body(12.0), Tokens::text());
                let r = Rect::from_center_size(pos2(content.center().x, full.bottom() - 40.0), vec2(g.size().x + 24.0, 30.0));
                p.rect_filled(r, radius(8), Tokens::banner());
                p.text(r.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(text), theme::body(12.0), Tokens::text());
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
        let Some(i) = self.whats_new_open else { return };
        let Some(e) = crate::whats_new::ENTRIES.get(i) else {
            self.whats_new_open = None;
            return;
        };
        let art = self.art(ui.ctx(), e.art);
        let mut close = false;
        egui::Area::new(ui.scope_id().with("whats-new-card")).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
            let back = ui.allocate_rect(full, Sense::click());
            let p = ui.painter();
            p.rect_filled(full, 0.0, Color32::from_black_alpha(110));
            let card = Rect::from_center_size(full.center(), vec2(440.0, 470.0));
            p.rect_filled(card.translate(vec2(0.0, 6.0)), radius(16), Color32::from_black_alpha(70));
            p.rect_filled(card, radius(14), hex(0x333333));
            let pic = Rect::from_min_size(card.min + vec2(0.0, 70.0), vec2(card.width(), 250.0));
            if let Some(tex) = art {
                let size = vec2(340.0, 438.0) * (pic.height() / 438.0);
                p.image(
                    tex,
                    Rect::from_center_size(pic.center(), size),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            p.text(card.min + vec2(24.0, 26.0), Align2::LEFT_CENTER, crate::i18n::tr_s(e.title), theme::strong(18.0), Color32::WHITE);
            p.text(
                card.min + vec2(24.0, 50.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(format!("New in {}", e.version)),
                theme::body(11.5),
                hex(0xa9a9ab),
            );
            let g = p.layout(e.text.to_string(), theme::body(12.5), hex(0xdfdfdf), card.width() - 48.0);
            p.galley(pos2(card.left() + 24.0, pic.bottom() + 16.0), g, hex(0xdfdfdf));
            let b = Rect::from_min_size(pos2(card.right() - 104.0, card.bottom() - 46.0), vec2(80.0, 28.0));
            let r = ui.interact(b, ui.scope_id().with("whats-new-close"), Sense::click());
            p.rect_filled(b, radius(14), hex(if r.hovered() { 0x2a5cff } else { 0x1d4fff }));
            p.text(b.center(), Align2::CENTER_CENTER, crate::i18n::tr("Close"), theme::strong(12.5), Color32::WHITE);
            // Every version's notes, in the changelog.
            let notes = Rect::from_min_size(pos2(card.left() + 24.0, card.bottom() - 46.0), vec2(150.0, 28.0));
            let n = ui.interact(notes, ui.scope_id().with("whats-new-notes"), Sense::click());
            let nc = if n.hovered() { hex(0xffffff) } else { hex(0xa9a9ab) };
            p.text(notes.left_center(), Align2::LEFT_CENTER, crate::i18n::tr("All release notes"), theme::body(12.0), nc);
            if n.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if n.clicked() {
                open_link(crate::whats_new::NOTES_URL);
            }
            let outside = back.clicked() && ui.input(|i| i.pointer.interact_pos().is_some_and(|q| !card.contains(q)));
            close = r.clicked() || outside || ui.input(|i| i.key_pressed(egui::Key::Escape));
        });
        if close {
            self.whats_new_open = None;
        }
    }

    /// What's new: the reference's cards (170 × 219 every 190 px, a white bold title on #333333, the feature's
    /// picture below it), scrolled sideways when they don't all fit. A click opens the card.
    /// `max`: how far it scrolls sideways.
    fn whats_new_row(&mut self, ui: &mut Ui, row: Rect, left: f32, max: f32) {
        let entries = crate::whats_new::ENTRIES;
        // (A phone's page takes the drags itself, up and down or along the row.)
        let touch = crate::tablet::touch_ui() && !crate::tablet::small_screen();
        let resp = ui.interact(row, ui.scope_id().with("whats-new-row"), if touch { Sense::drag() } else { Sense::hover() });
        if touch {
            let now = ui.input(|i| i.time);
            if resp.dragged() {
                self.row.drag(-resp.drag_delta().x, max, row.width(), now);
            } else if resp.drag_stopped() {
                self.row.release(now);
            }
            if self.row.step(ui.input(|i| i.stable_dt), max) {
                ui.ctx().request_repaint();
            }
        }
        if resp.hovered() {
            let d = ui.input(|i| i.smooth_scroll_delta);
            let d = if d.x.abs() > 0.0 { d.x } else { d.y };
            // (Only with wheel input: a nudge stops a glide.)
            if d != 0.0 {
                self.row.nudge(-d, max);
            }
        }
        let clip = Rect::from_min_max(pos2(row.left() + 1.0, row.top()), row.max);
        let p = ui.painter().with_clip_rect(clip);
        for (i, e) in entries.iter().enumerate() {
            let r = Rect::from_min_size(pos2(left + i as f32 * 190.0 - self.row.offset, row.top() + 8.0), vec2(170.0, 219.0));
            if r.right() < clip.left() || r.left() > clip.right() {
                continue;
            }
            let hit = ui.interact(r.intersect(clip), ui.scope_id().with(("whats-new", i)), Sense::click());
            p.rect_filled(r, radius(5), hex(0x333333));
            if let Some(tex) = self.art(ui.ctx(), e.art) {
                // The picture bleeds off the bottom, as the reference's do.
                let pic = Rect::from_center_size(pos2(r.center().x + 4.0, r.top() + 140.0), vec2(184.0, 237.0));
                p.with_clip_rect(r.intersect(clip)).image(tex, pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            p.galley(
                pos2(r.left() + 16.0, r.top() + 15.0),
                p.layout(crate::i18n::tr_s(e.title), theme::strong(14.0), Color32::WHITE, 140.0),
                Color32::WHITE,
            );
            if hit.clicked() {
                self.whats_new_open = Some(i);
            }
            if hit.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
    }

    /// Get Started and Tutorials, laid out like the reference's (s30-013/014): a bold heading, then 170 × 219 cards
    /// every 190 px; blue cards on Get Started, dark ones on Tutorials.
    /// The cards wrap in rows, on a phone filling the width; returns where the page ends.
    fn learn_page(&mut self, ui: &mut Ui, top: f32, left: f32, right: f32, phone: bool) -> (Option<LandingAction>, f32) {
        let p = ui.painter().clone();
        let mut action = None;
        let tutorials_page = self.section == Section::Tutorials;
        let mut y = top + 94.0;
        // Columns and card width: 170 every 190 on a computer; on a phone as many as fit, widened
        // to fill the room.
        let room = right - 20.0 - left;
        let cols = ((room + 20.0) / 190.0).floor().max(1.0);
        let card_w = if phone { (room - (cols - 1.0) * 20.0) / cols } else { 170.0 };
        let cols = cols as usize;
        let place = |i: usize, y: f32| -> Rect {
            let (row, col) = (i / cols, i % cols);
            Rect::from_min_size(pos2(left + col as f32 * (card_w + 20.0), y + 34.0 + row as f32 * 239.0), vec2(card_w, 219.0))
        };
        let rows = |n: usize| n.div_ceil(cols.min(n.max(1))) as f32;
        let heading = |y: f32, text: &str| {
            p.text(pos2(left, y), Align2::LEFT_CENTER, crate::i18n::tr_s(text), theme::strong(16.0), Tokens::text_strong());
        };
        use crate::tutorials::{Course, LESSONS};
        // The beginner courses in order, then (on Tutorials) one for each tool.
        let parts: &[(Course, &str, u32)] = if tutorials_page {
            &[(Course::Beginner, "Start Here", 0x1d4fff), (Course::Tool, "Learn a Tool", 0x333333)]
        } else {
            &[(Course::Beginner, "New to CAD", 0x1d4fff)]
        };
        for (k, &(course, title, fill)) in parts.iter().enumerate() {
            if k > 0 {
                y += 53.0;
            }
            heading(y, title);
            let lessons: Vec<_> = LESSONS.iter().filter(|l| l.course == course).collect();
            for (i, lesson) in lessons.iter().enumerate() {
                if self.lesson_card(ui, place(i, y), lesson, hex(fill), (course == Course::Beginner).then_some(i + 1)) {
                    action = Some(LandingAction::Tutorial(lesson.id));
                }
            }
            y += 34.0 + rows(lessons.len()) * 239.0 - 20.0;
        }
        if tutorials_page {
            return (action, y + 16.0);
        }
        y += 53.0;
        heading(y, "Coming from another CAD");
        let cards = [("Navigation Presets", LandingAction::NavigationPresets), ("Keyboard Shortcuts", LandingAction::Shortcuts)];
        for (i, (title, act)) in cards.into_iter().enumerate() {
            let r = place(i, y);
            let hit = ui.interact(r, ui.scope_id().with(("from-cad", i)), Sense::click());
            p.rect_filled(r, radius(5), hex(0x505050));
            p.galley(
                pos2(r.left() + 16.0, r.top() + 15.0),
                p.layout(crate::i18n::tr_s(title), theme::strong(14.0), Color32::WHITE, 140.0),
                Color32::WHITE,
            );
            if i == 0 {
                // The presets to choose from, in a 2 × 2 grid as the reference draws its own.
                let grid = Rect::from_min_size(pos2(r.center().x - 69.0, r.top() + 62.0), vec2(138.0, 138.0));
                let line = Stroke::new(1.0, hex(0xd9d9d9));
                p.rect_stroke(grid, 0.0, line, StrokeKind::Inside);
                p.line_segment([pos2(grid.center().x, grid.top()), pos2(grid.center().x, grid.bottom())], line);
                p.line_segment([pos2(grid.left(), grid.center().y), pos2(grid.right(), grid.center().y)], line);
                let names = ["Blender", "Fusion", "SolidWorks", "Onshape"];
                for (k, name) in names.iter().enumerate() {
                    let cell = Rect::from_min_size(grid.min + vec2((k % 2) as f32 * 69.0, (k / 2) as f32 * 69.0), vec2(69.0, 69.0));
                    let tile = Rect::from_center_size(cell.center() - vec2(0.0, 6.0), vec2(34.0, 34.0));
                    p.rect_filled(tile, radius(8), if k == 3 { hex(0x111111) } else { Color32::WHITE });
                    if k == 3 {
                        p.text(tile.center(), Align2::CENTER_CENTER, "+", theme::body(20.0), Color32::WHITE);
                    } else {
                        p.text(tile.center(), Align2::CENTER_CENTER, &name[..1], theme::strong(16.0), hex(0x1d4fff));
                    }
                    p.text(pos2(cell.center().x, cell.bottom() - 9.0), Align2::CENTER_CENTER, *name, theme::body(9.0), hex(0xe6e6e6));
                }
            } else {
                // Key caps in a checkerboard (four across, whatever the card's width).
                let k_w = r.width() / 4.0;
                let keys = ["⌘F", "", "E", "", "⇧⌘A", "F", "", "⌥⌘P", "", "R", "⌘1", ""];
                for (k, key) in keys.iter().enumerate() {
                    let cell =
                        Rect::from_min_size(pos2(r.left() + (k % 4) as f32 * k_w, r.top() + 88.0 + (k / 4) as f32 * 42.5), vec2(k_w, 42.5));
                    if key.is_empty() {
                        continue;
                    }
                    let dark = (k + k / 4) % 2 == 0;
                    p.rect_filled(cell.intersect(r), 0.0, if dark { hex(0x111111) } else { hex(0xf2f2f2) });
                    p.text(
                        cell.center(),
                        Align2::CENTER_CENTER,
                        theme::key_text(key),
                        theme::strong(10.0),
                        if dark { Color32::WHITE } else { hex(0x222222) },
                    );
                }
            }
            if hit.hovered() {
                p.rect_stroke(r, radius(5), Stroke::new(2.0, Color32::from_white_alpha(70)), StrokeKind::Inside);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hit.clicked() {
                action = Some(act);
            }
        }
        (action, y + 34.0 + rows(2) * 239.0 - 20.0 + 16.0)
    }

    /// A tutorial's card: its title and what it teaches over a picture of what it makes. Returns whether it was pressed.
    fn lesson_card(&mut self, ui: &mut Ui, r: Rect, lesson: &crate::tutorials::Lesson, fill: Color32, number: Option<usize>) -> bool {
        let p = ui.painter().clone();
        let hit = ui.interact(r.intersect(ui.clip_rect()), ui.scope_id().with(("lesson", lesson.id)), Sense::click());
        p.rect_filled(r, radius(8), fill);
        let w = r.width() - 32.0;
        let title = match number {
            Some(n) => format!("{n}. {}", crate::i18n::tr(lesson.title)),
            None => crate::i18n::tr_s(lesson.title),
        };
        let title = p.layout(title, theme::strong(14.0), Color32::WHITE, w);
        let about = p.layout(crate::i18n::tr_s(lesson.about), theme::body(11.0), Color32::from_white_alpha(210), w);
        let words = title.size().y + 4.0 + about.size().y;
        if let Some(tex) = self.art(ui.ctx(), &format!("tut-{}", lesson.id)) {
            // Under the words, as big as fits, off the bottom edge a little.
            let room = Rect::from_min_max(pos2(r.left() + 12.0, r.top() + 15.0 + words + 4.0), pos2(r.right() - 12.0, r.bottom() - 4.0));
            let k = (room.width() / 340.0).min(room.height() / 360.0);
            let pic = Rect::from_center_size(room.center() + vec2(0.0, -4.0), vec2(340.0, 438.0) * k);
            p.with_clip_rect(r.intersect(ui.clip_rect())).image(
                tex,
                pic,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let title_h = title.size().y;
        p.galley(pos2(r.left() + 16.0, r.top() + 14.0), title, Color32::WHITE);
        p.galley(pos2(r.left() + 16.0, r.top() + 18.0 + title_h), about, Color32::WHITE);
        p.text(
            pos2(r.left() + 16.0, r.bottom() - 18.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr("{n} min").replace("{n}", &lesson.minutes.to_string()),
            theme::strong(10.5),
            Color32::WHITE,
        );
        // ▶ in a ring, bottom right; a tick instead once it's been done.
        let c = pos2(r.right() - 22.0, r.bottom() - 18.0);
        let done = self.tutorials_done.iter().any(|d| d == lesson.id);
        if done {
            p.circle_filled(c, 9.0, hex(0x34c759));
            p.line_segment([c + vec2(-3.4, 0.2), c + vec2(-0.9, 2.8)], Stroke::new(1.6, Color32::WHITE));
            p.line_segment([c + vec2(-0.9, 2.8), c + vec2(3.6, -2.6)], Stroke::new(1.6, Color32::WHITE));
        } else {
            p.circle_filled(c, 9.0, Color32::from_black_alpha(70));
            p.circle_stroke(c, 8.0, Stroke::new(1.4, Color32::WHITE));
            p.add(egui::Shape::convex_polygon(
                vec![c + vec2(-2.2, -3.6), c + vec2(3.8, 0.0), c + vec2(-2.2, 3.6)],
                Color32::WHITE,
                Stroke::NONE,
            ));
        }
        if hit.hovered() {
            p.rect_stroke(r, radius(8), Stroke::new(2.0, Color32::from_white_alpha(70)), StrokeKind::Inside);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        hit.clicked()
    }

    /// How far a phone's page scrolls: until `end` (where its content stops now) comes up to `bottom`.
    fn clamp_scroll(&mut self, end: f32, bottom: f32) {
        self.page_max = (self.page.offset + end - bottom).max(0.0);
    }

    /// A picture from `assets/home/`, decoded once.
    fn art(&mut self, ctx: &egui::Context, name: &str) -> Option<egui::TextureId> {
        if let Some(t) = self.art.get(name) {
            return Some(t.id());
        }
        let bytes = ART.iter().find(|(n, _)| *n == name)?.1;
        let image = decode_png(std::io::Cursor::new(bytes))?;
        let tex = ctx.load_texture(format!("home-art-{name}"), image, egui::TextureOptions::LINEAR);
        let id = tex.id();
        self.art.insert(name.to_string(), tex);
        Some(id)
    }

    pub fn show_tutorials(&mut self) {
        self.visit(Section::Tutorials, self.folder.clone());
    }

    pub fn show_section(&mut self, name: &str) -> Result<(), String> {
        self.search.clear();
        match name {
            "get_started" => self.visit(Section::GetStarted, self.folder.clone()),
            "tutorials" => self.visit(Section::Tutorials, self.folder.clone()),
            "recents" => self.visit(Section::Recents, self.folder.clone()),
            "all" => {
                let root = self.root.clone();
                self.go_to(root);
            }
            "whats_new" => {
                self.section = Section::Recents;
                self.whats_new_open = Some(0);
            }
            _ => return Err("'section' is get_started, tutorials, recents, all or whats_new".into()),
        }
        self.rescan();
        Ok(())
    }
}

/// The home screen's pictures (our own renders, `assets/home/`, made with `shapecraft-cli run … --png --clear`).
const ART: &[(&str, &[u8])] = &[
    ("tut-coaster", include_bytes!("../../../assets/home/tut-coaster.png")),
    ("tut-pencil-cup", include_bytes!("../../../assets/home/tut-pencil-cup.png")),
    ("tut-phone-stand", include_bytes!("../../../assets/home/tut-phone-stand.png")),
    ("tut-drawer-knob", include_bytes!("../../../assets/home/tut-drawer-knob.png")),
    ("tut-flower-pot", include_bytes!("../../../assets/home/tut-flower-pot.png")),
    ("tut-coat-hook", include_bytes!("../../../assets/home/tut-coat-hook.png")),
    ("tut-birthday-cake", include_bytes!("../../../assets/home/tut-birthday-cake.png")),
    ("tut-serving-tray", include_bytes!("../../../assets/home/tut-serving-tray.png")),
    ("tut-bottle-neck", include_bytes!("../../../assets/home/tut-bottle-neck.png")),
    ("new-navigation", include_bytes!("../../../assets/home/new-navigation.png")),
    ("new-tutorials", include_bytes!("../../../assets/home/new-tutorials.png")),
    ("new-view-cube", include_bytes!("../../../assets/home/new-view-cube.png")),
    ("new-cards", include_bytes!("../../../assets/home/new-cards.png")),
    ("new-copies", include_bytes!("../../../assets/home/new-copies.png")),
    ("new-languages", include_bytes!("../../../assets/home/new-languages.png")),
    ("new-shapr", include_bytes!("../../../assets/home/new-shapr.png")),
    ("new-meshes", include_bytes!("../../../assets/home/new-meshes.png")),
    ("new-threads", include_bytes!("../../../assets/home/new-threads.png")),
    ("new-iphone", include_bytes!("../../../assets/home/new-iphone.png")),
];

thread_local! {
    /// A link to hand to the tablet's page at the end of the frame.
    static TABLET_LINK: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The link the tablet's copy asked to open this frame, for egui to send to the page.
pub(crate) fn take_tablet_link() -> Option<String> {
    TABLET_LINK.with(|l| l.take())
}

/// The file manager's menu item.
const SHOW_IN: &str = if cfg!(target_os = "macos") {
    "Show in Finder"
} else if cfg!(windows) {
    "Show in Explorer"
} else {
    "Show in Files"
};

/// Opens a web page in the default browser.
pub(crate) fn open_link(url: &str) {
    // On a tablet the page opens it (the app's own platform output carries it there).
    if crate::tablet::on_tablet() {
        TABLET_LINK.with(|l| l.replace(Some(url.to_string())));
        return;
    }
    crate::app::open_page(url);
}

/// Opens a folder in the system's file manager.
pub(crate) fn reveal(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}

pub(crate) fn shorten_home(p: &Path) -> String {
    match dirs::home_dir() {
        Some(h) if p.starts_with(&h) => format!("~/{}", p.strip_prefix(&h).map(|r| r.display().to_string()).unwrap_or_default()),
        _ => p.display().to_string(),
    }
}

/// "9/22/26", the way the reference app dates projects.
/// The computer's menus a phone shows as pages, from its sections menu.
const PHONE_MENUS: [&str; 2] = ["File", "Help"];

/// One row of a phone's menu page; no action = a line of text.
struct MenuRow {
    label: String,
    action: Option<crate::menubar::MenuAction>,
}

/// The home screen's menu `title` as a phone's rows: the items that work here, grouped as the menu's separators
/// group them. Items for a picked card (Duplicate, Rename, New Folder with Selection) stay on its long-press menu.
fn phone_menu_rows(title: &str) -> Vec<Vec<MenuRow>> {
    use crate::menubar::{Entry, MenuState, home_tree};
    let home = MenuState::default();
    let mut groups: Vec<Vec<MenuRow>> = vec![Vec::new()];
    for (t, entries) in home_tree() {
        if t != title {
            continue;
        }
        for e in entries {
            match e {
                Entry::Separator => groups.push(Vec::new()),
                Entry::Item { label, action, when, .. }
                    if when.holds(&home) && !matches!(label, "Duplicate" | "Rename" | "New Folder with Selection") =>
                {
                    let label = match label {
                        "Import as new project..." => "Import…",
                        l => l,
                    };
                    if let Some(g) = groups.last_mut() {
                        g.push(MenuRow { label: label.to_string(), action: Some(action) });
                    }
                }
                _ => {}
            }
        }
    }
    groups.retain(|g| !g.is_empty());
    groups
}

/// How the last update check went, for the line under Check for Updates (None before any check).
fn update_note(u: &shapecraft_update::Snapshot) -> Option<String> {
    use shapecraft_update::Phase;
    let offered = u.offered.as_ref().map(|o| o.0.clone());
    Some(match (&u.phase, offered) {
        (Phase::Checking, _) => "Checking GitHub…".to_string(),
        (Phase::UpToDate, _) => format!("You're up to date ({})", u.current),
        (Phase::Failed(why), _) => format!("Couldn't check: {why}"),
        (_, Some(v)) => format!("ShapeCraft {v} is out (you have {})", u.current),
        (Phase::Idle, None) if u.checked_ago.is_none() => return None,
        _ => format!("You're up to date ({})", u.current),
    })
}

/// A phone's glass buttons, placed (see `Landing::phone_chrome`).
struct PhoneChrome {
    sections: Rect,
    title: Rect,
    title_text: std::sync::Arc<egui::Galley>,
    search: Rect,
    /// Back and forward, and New, Import, New Folder and Sort: two capsules down the right.
    nav: Rect,
    tools: Rect,
    /// Both capsules: the page's cards keep left of it.
    rail: Rect,
    /// Where the page starts under the top row.
    head: f32,
}

/// The rail's buttons: a capsule this wide, a button every PITCH down it, PAD at its ends.
const RAIL_W: f32 = 56.0;
const PITCH: f32 = 46.0;
const PAD: f32 = 5.0;
/// The top row's buttons and fields (Apple's smallest comfortable tap).
const ROW_H: f32 = 44.0;

impl PhoneChrome {
    /// `narrow`: upright (the iPhone Duo folded shut), where the search takes a second row.
    fn new(ui: &Ui, full: Rect, narrow: bool, title: &str) -> Self {
        // Level with the clock in the status corner (its middle about 38 down on the iPhone Duo, opened or shut).
        // An ordinary iPhone keeps its status bar across the whole top (a plain inset, no corner): the row goes under it.
        let inset = ui.ctx().input(|i| i.safe_area_insets().0.top);
        let top = if inset > 0.0 && inset.is_finite() { full.top() + inset + 4.0 } else { full.top() + 16.0 };
        let lead = crate::tablet::top_left(full.top(), full.left()) - full.left();
        let sections = Rect::from_min_size(pos2(full.left() + 16.0 + lead, top), vec2(ROW_H, ROW_H));
        let bar = crate::tablet::side_bar();
        let corner = crate::tablet::reserved_corner();
        // In the vertical bar, under the clock; without one, down the right edge under the status corner.
        let (cx, rail_top) = match bar {
            Some(b) => (b.center().x, corner.filter(|c| c.intersects(b)).map_or(b.top() + 110.0, |c| c.bottom()) + 12.0),
            // Upright, the rail starts level with the search row, under the title's row.
            None if narrow => (full.right() - 12.0 - RAIL_W / 2.0, crate::tablet::right_top(top, full.right()).max(top + ROW_H + 8.0)),
            None => (full.right() - 12.0 - RAIL_W / 2.0, crate::tablet::right_top(top, full.right())),
        };
        let nav = Rect::from_min_size(pos2(cx - RAIL_W / 2.0, rail_top), vec2(RAIL_W, 2.0 * PITCH + 2.0 * PAD));
        let tools = Rect::from_min_size(pos2(nav.left(), nav.bottom() + 12.0), vec2(RAIL_W, 4.0 * PITCH + 2.0 * PAD));
        let rail = nav.union(tools);
        let search = if narrow {
            // Under the status block when it reaches that far down, level with the top of the rail.
            let row = crate::tablet::right_top(sections.bottom() + 8.0, full.right());
            Rect::from_min_max(pos2(full.left() + 16.0, row), pos2(rail.left() - 10.0, row + ROW_H))
        } else {
            let right = match bar {
                Some(b) => b.left() - 12.0,
                None => (crate::tablet::top_right(full.top(), full.right()) - 12.0).min(if corner.is_none() {
                    rail.left() - 12.0
                } else {
                    f32::INFINITY
                }),
            };
            Rect::from_min_max(pos2(right - 250.0, top), pos2(right, top + ROW_H))
        };
        let title_right = if narrow { crate::tablet::top_right(full.top(), full.right()) - 8.0 } else { search.left() - 12.0 };
        let left = sections.right() + 8.0;
        let room = (title_right - left - 32.0).max(40.0);
        let mut job = egui::text::LayoutJob::simple_singleline(title.to_string(), theme::strong(17.0), Tokens::text());
        job.wrap = egui::text::TextWrapping { max_width: room, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
        let title_text = ui.ctx().fonts_mut(|f| f.layout_job(job));
        let title = Rect::from_min_size(pos2(left, top), vec2(title_text.size().x + 32.0, ROW_H));
        let head = if narrow { search.bottom() + 8.0 } else { sections.bottom() + 8.0 };
        PhoneChrome { sections, title, title_text, search, nav, tools, rail, head }
    }

    /// The k-th button of a capsule.
    fn cell(&self, capsule: Rect, k: usize) -> Rect {
        Rect::from_min_size(pos2(capsule.left(), capsule.top() + PAD + k as f32 * PITCH), vec2(RAIL_W, PITCH))
    }
}

/// A frosted-glass control: see-through, so the page shows behind it, with a soft shadow and a fine bright rim.
fn glass(p: &egui::Painter, r: Rect) {
    let round = radius((r.width().min(r.height()) / 2.0).min(255.0) as u8);
    let light = theme::is_light();
    p.add(
        egui::epaint::Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(if light { 36 } else { 80 }) }
            .as_shape(r, round),
    );
    let fill = if light { Color32::from_rgba_unmultiplied(250, 250, 252, 205) } else { Color32::from_rgba_unmultiplied(52, 52, 56, 200) };
    p.rect_filled(r, round, fill);
    let rim = if light { Color32::from_black_alpha(20) } else { Color32::from_white_alpha(26) };
    p.rect_stroke(r, round, Stroke::new(1.0, rim), StrokeKind::Inside);
}

fn short_date(t: SystemTime) -> String {
    let (y, m, d) = shapecraft_doc::date::local_date(t);
    format!("{m}/{d}/{:02}", y.rem_euclid(100))
}

/// A small floating menu in the workspace's style. Returns the chosen row and the menu's rect.
fn popup_menu(ui: &mut Ui, id: &str, at: egui::Pos2, entries: &[&str], danger: &str) -> (Option<usize>, Rect) {
    let mut chosen = None;
    let shown = egui::Area::new(ui.scope_id().with(id)).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(180.0, 28.0 * entries.len() as f32 + 8.0), Sense::hover());
        ui.painter().rect_filled(rect, radius(8), Tokens::banner());
        ui.painter().rect_stroke(rect, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
        for (i, label) in entries.iter().enumerate() {
            let row = Rect::from_min_size(rect.min + vec2(4.0, 4.0 + 28.0 * i as f32), vec2(172.0, 28.0));
            let r = ui.interact(row, ui.scope_id().with((id, i)), Sense::click());
            if r.hovered() {
                ui.painter().rect_filled(row, radius(6), Tokens::accent());
            }
            let col = if *label == danger && !r.hovered() { Tokens::danger() } else { Tokens::text() };
            ui.painter().text(
                pos2(row.left() + 10.0, row.center().y),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(*label),
                theme::body(12.5),
                col,
            );
            if r.clicked() {
                chosen = Some(i);
            }
        }
    });
    (chosen, shown.response.rect)
}

impl Landing {
    /// The folder tree: the Work Folder (or the backup being browsed) and the folders in it, each opening with its
    /// triangle; a row goes to that folder.
    fn folder_tree(&mut self, ui: &mut Ui, pane: Rect) {
        let light = theme::is_light();
        let p = ui.painter();
        p.rect_filled(pane, 0.0, if light { hex(0xefeeee) } else { theme::c(0x252525) });
        p.line_segment([pane.right_top(), pane.right_bottom()], Stroke::new(1.0, if light { hex(0xd6d5d5) } else { hex(0x000000) }));
        let top = self.roots().into_iter().filter(|r| self.folder.starts_with(r)).max_by_key(|r| r.components().count());
        let top = top.unwrap_or_else(|| self.root.clone());
        if self.tree_seen != self.folder {
            // Open the way down to the folder being shown.
            self.tree_seen = self.folder.clone();
            let mut at = self.folder.parent().map(Path::to_path_buf);
            while let Some(dir) = at.filter(|d| d.starts_with(&top)) {
                at = dir.parent().map(Path::to_path_buf);
                self.expanded.insert(dir);
            }
            self.expanded.insert(top.clone());
        }
        let mut rows = Vec::new();
        self.tree_rows(&top, 0, &mut rows);
        let touch = crate::tablet::touch_ui();
        let row_h = if touch { 36.0 } else { 26.0 };
        let root_name = match self.places.iter().find(|p| p.dir == top) {
            Some(place) => format!("{} Backup", place.label),
            None => "All Projects".to_string(),
        };
        let mut go = None;
        let mut toggle = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(pane.shrink2(vec2(6.0, 8.0))));
        egui::ScrollArea::vertical().id_salt("folder-tree").show(&mut child, |ui| {
            let w = ui.available_width();
            ui.spacing_mut().item_spacing.y = 2.0;
            for (depth, dir, kids) in &rows {
                let (r, resp) = ui.allocate_exact_size(vec2(w, row_h), Sense::click());
                let on = *dir == self.folder;
                let p = ui.painter();
                if on {
                    p.rect_filled(r, radius(6), if light { hex(0xc9c8c8) } else { theme::c(0x3f3e3e) });
                } else if resp.hovered() {
                    p.rect_filled(r, radius(6), if light { hex(0xd9d8d8) } else { theme::c(0x333232) });
                }
                let x = r.left() + 4.0 + *depth as f32 * 14.0;
                let open = self.expanded.contains(dir);
                if *kids {
                    let c = pos2(x + 7.0, r.center().y);
                    let tri = if open {
                        vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
                    } else {
                        vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
                    };
                    p.add(egui::Shape::convex_polygon(tri, Tokens::text_dim(), Stroke::NONE));
                }
                let icon = if *depth == 0 { Icon::Projects } else { Icon::Folder };
                icons::paint(p, Rect::from_center_size(pos2(x + 24.0, r.center().y), vec2(15.0, 15.0)), icon, Tokens::icon_blue());
                let name = if *depth == 0 {
                    root_name.clone()
                } else {
                    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                };
                let mut job = egui::text::LayoutJob::simple_singleline(name, theme::body(if touch { 13.5 } else { 12.5 }), Tokens::text());
                job.wrap = egui::text::TextWrapping {
                    max_width: (r.right() - x - 40.0).max(20.0),
                    max_rows: 1,
                    break_anywhere: true,
                    overflow_character: Some('…'),
                };
                let g = ui.fonts_mut(|f| f.layout_job(job));
                ui.painter().galley(pos2(x + 36.0, r.center().y - g.size().y / 2.0), g, Tokens::text());
                if resp.clicked() {
                    // The triangle opens and closes; the rest of the row goes there.
                    let on_triangle = *kids && resp.interact_pointer_pos().is_some_and(|q| q.x < x + 16.0);
                    if on_triangle {
                        toggle = Some(dir.clone());
                    } else {
                        go = Some(dir.clone());
                    }
                }
            }
        });
        if let Some(dir) = toggle
            && !self.expanded.remove(&dir)
        {
            self.expanded.insert(dir);
        }
        if let Some(dir) = go {
            self.selected = None;
            self.go_to(dir);
        }
    }

    /// The tree's rows from `dir` down, through the open folders: (depth, folder, has folders in it).
    fn tree_rows(&mut self, dir: &Path, depth: usize, out: &mut Vec<(usize, PathBuf, bool)>) {
        let kids = self.tree_kids.entry(dir.to_path_buf()).or_insert_with(|| project::folders(dir)).clone();
        out.push((depth, dir.to_path_buf(), !kids.is_empty()));
        if self.expanded.contains(dir) {
            for k in kids {
                self.tree_rows(&k, depth + 1, out);
            }
        }
    }

    /// New, Import, New Folder and Sort, tapped (`hit` in that order); the Sort menu opens at `sort_at`.
    fn tool_hits(&mut self, hit: [bool; 4], sort_at: egui::Pos2, now: f64) -> Option<LandingAction> {
        if hit[3] {
            self.sort_menu = if self.sort_menu.is_some() && crate::tablet::small_screen() { None } else { Some(sort_at) };
            self.sort_opened = now;
            self.menu = None;
            self.nav_menu = None;
        }
        if hit[2] {
            if self.section != Section::All || self.backup_of(&self.folder).is_some() {
                let root = self.root.clone();
                self.go_to(root);
            }
            let parent = self.folder.clone();
            if let Some(dir) = self.report(now, project::new_folder(&parent)) {
                let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.renaming = Some((dir, name, true));
            }
        }
        if hit[1] {
            return Some(LandingAction::Import);
        }
        if hit[0] {
            // A view-only phone opens projects and files to look at; a new one starts on a computer or the Duo.
            if crate::tablet::view_only() {
                self.toast = Some((crate::tablet::VIEW_ONLY_NOTE.into(), now + 4.0));
                return None;
            }
            return Some(LandingAction::New(self.target_dir()));
        }
        None
    }

    /// A phone's search field: a glass capsule.
    fn phone_search(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        glass(&p, r);
        icons::paint(&p, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(16.0, 16.0)), Icon::Search, Tokens::text_dim());
        let field = Rect::from_min_max(pos2(r.left() + 38.0, r.center().y - 10.0), pos2(r.right() - 16.0, r.center().y + 10.0));
        let searched = ui.put(
            field,
            egui::TextEdit::singleline(&mut self.search)
                .hint_text(crate::i18n::tr("Search"))
                .frame(egui::Frame::NONE)
                .font(theme::body(15.0)),
        );
        if std::mem::take(&mut self.focus_search) {
            searched.request_focus();
        }
        if searched.changed() {
            self.page.reset(0.0);
            if !self.search.is_empty() && !matches!(self.section, Section::Recents | Section::All) {
                self.section = Section::Recents;
            }
        }
    }

    /// A phone's glass buttons over its page on the iPhone Duo home screen: the sections
    /// button and the page's title top left, the search top right, and down the right (the Duo's vertical bar, under
    /// its clock) back/forward in one group and New, Import, New Folder and Sort in another.
    fn phone_chrome(&mut self, ui: &mut Ui, c: &PhoneChrome, screen: Rect, now: f64) -> Option<LandingAction> {
        ui.set_clip_rect(screen);
        let p = ui.painter().clone();
        // The page fades out under the top row (as a phone's scroll edge does), so its buttons stand clear.
        {
            let bg = theme::c(0x1e1e1e);
            let solid = Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 230);
            let mid = c.head * 0.45;
            let mut m = egui::Mesh::default();
            for (y, col) in [(screen.top(), solid), (mid, solid), (c.head + 8.0, Color32::TRANSPARENT)] {
                m.colored_vertex(pos2(screen.left(), y), col);
                m.colored_vertex(pos2(screen.right(), y), col);
            }
            m.add_triangle(0, 1, 3);
            m.add_triangle(0, 3, 2);
            m.add_triangle(2, 3, 5);
            m.add_triangle(2, 5, 4);
            p.add(m);
        }
        let press = |ui: &Ui, r: Rect, id: &str| -> egui::Response {
            let resp = ui.interact(r, ui.scope_id().with(("phone", id)), Sense::click());
            if resp.is_pointer_button_down_on() || resp.hovered() && !crate::tablet::touch_ui() {
                let k = r.shrink(3.0);
                p.rect_filled(k, radius((k.height().min(k.width()) / 2.0) as u8), Color32::from_white_alpha(30));
            }
            resp
        };
        // Sections, and the title (tapping it opens the same menu).
        glass(&p, c.sections);
        let a = press(ui, c.sections, "sections");
        icons::paint(&p, Rect::from_center_size(c.sections.center(), vec2(22.0, 22.0)), Icon::SidebarToggle, Tokens::text());
        glass(&p, c.title);
        let b = press(ui, c.title, "title");
        p.galley(pos2(c.title.left() + 16.0, c.title.center().y - c.title_text.size().y / 2.0), c.title_text.clone(), Tokens::text());
        if a.clicked() || b.clicked() {
            self.nav_menu = if self.nav_menu.is_some() { None } else { Some((pos2(c.sections.left(), c.sections.bottom() + 8.0), now)) };
            self.sort_menu = None;
        }
        self.phone_search(ui, c.search);
        // Back and forward.
        glass(&p, c.nav);
        for (k, (icon, enabled, id)) in
            [(Icon::ChevronLeft, !self.back.is_empty(), "back"), (Icon::ChevronRight, !self.forward.is_empty(), "forward")]
                .into_iter()
                .enumerate()
        {
            let r = c.cell(c.nav, k);
            let resp = if enabled { Some(press(ui, r, id)) } else { None };
            let ink = if enabled { Tokens::text() } else { Tokens::text_faint() };
            icons::paint(&p, Rect::from_center_size(r.center(), vec2(22.0, 22.0)), icon, ink);
            if resp.is_some_and(|r| r.clicked()) {
                if k == 0 {
                    self.go_back();
                } else {
                    self.go_forward();
                }
            }
        }
        // New, Import, New Folder, Sort.
        glass(&p, c.tools);
        let tools = [(Icon::Plus, "new"), (Icon::Import, "import"), (Icon::NewFolder, "folder"), (Icon::Sort, "sort")];
        let mut hit = [false; 4];
        for (k, (icon, id)) in tools.into_iter().enumerate() {
            let r = c.cell(c.tools, k);
            if k == 3 && self.sort_menu.is_some() {
                let s = r.shrink(3.0);
                p.rect_filled(s, radius((s.width() / 2.0) as u8), Color32::from_white_alpha(30));
            }
            hit[k] = press(ui, r, id).clicked();
            icons::paint(&p, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), icon, Tokens::text());
        }
        let sort_at = pos2(c.tools.left() - 194.0, c.cell(c.tools, 3).top());
        let action = self.tool_hits(hit, sort_at, now);
        self.nav(ui, now).or(action)
    }

    /// A phone's page for one of the computer's menus: its items that make sense on the home screen, in groups as the
    /// menu's separators have them, one tap each. Check for Updates says how the check went right there (a phone
    /// can't install an update itself: a newer version links to its download page).
    fn menu_page(&mut self, ui: &mut Ui, title: &str, top: f32, left: f32, right: f32, now: f64) -> (Option<LandingAction>, f32) {
        use crate::menubar::{AppAction, MenuAction};
        let p = ui.painter().clone();
        let mut action = None;
        let width = (right - left - 20.0).clamp(200.0, 560.0);
        let mut y = top;
        let updates = shapecraft_update::Updater::global().snapshot();
        let mut groups = phone_menu_rows(title);
        if title == "Help" {
            groups.push(vec![MenuRow { label: format!("Version {}", env!("CARGO_PKG_VERSION")), action: None }]);
        }
        for (g, group) in groups.into_iter().enumerate() {
            // Under Check for Updates, what the check found.
            let mut rows: Vec<(MenuRow, Option<String>)> = Vec::new();
            for row in group {
                let sub = (row.action == Some(MenuAction::App(AppAction::CheckUpdates))).then(|| update_note(&updates)).flatten();
                let newer = row.action == Some(MenuAction::App(AppAction::CheckUpdates))
                    && updates.offered.is_some()
                    && !matches!(updates.phase, shapecraft_update::Phase::Checking);
                rows.push((row, sub));
                if newer {
                    let v = updates.offered.as_ref().map(|o| o.0.clone()).unwrap_or_default();
                    rows.push((
                        MenuRow {
                            label: format!("Get ShapeCraft {v} on GitHub"),
                            action: Some(MenuAction::App(AppAction::OpenPage(crate::menubar::RELEASES))),
                        },
                        None,
                    ));
                }
            }
            let heights: Vec<f32> = rows.iter().map(|(_, sub)| if sub.is_some() { 62.0 } else { 48.0 }).collect();
            let block = Rect::from_min_size(pos2(left, y), vec2(width, heights.iter().sum()));
            p.rect_filled(block, radius(12), theme::c(0x2c2c2e));
            let mut ry = y;
            for (i, ((row, sub), h)) in rows.iter().zip(&heights).enumerate() {
                let r = Rect::from_min_size(pos2(left, ry), vec2(width, *h));
                if i > 0 {
                    p.line_segment([pos2(r.left() + 16.0, r.top()), pos2(r.right(), r.top())], Stroke::new(0.5, theme::c(0x3d3d40)));
                }
                if let Some(a) = row.action {
                    let resp = ui.interact(r, ui.scope_id().with(("menu-row", title, g, i)), Sense::click());
                    if resp.is_pointer_button_down_on() {
                        p.rect_filled(r.shrink(1.0), radius(12), Color32::from_white_alpha(18));
                    }
                    if resp.clicked() {
                        action = Some(LandingAction::Menu(a));
                    }
                }
                let ink = if row.action.is_some() { Tokens::text() } else { Tokens::text_dim() };
                let ty = if sub.is_some() { r.top() + 22.0 } else { r.center().y };
                p.text(pos2(r.left() + 16.0, ty), Align2::LEFT_CENTER, &row.label, theme::body(16.0), ink);
                if let Some(sub) = sub {
                    p.text(pos2(r.left() + 16.0, r.top() + 44.0), Align2::LEFT_CENTER, sub, theme::body(13.0), Tokens::text_dim());
                }
                ry += h;
            }
            y = block.bottom() + 24.0;
        }
        if matches!(updates.phase, shapecraft_update::Phase::Checking) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        let _ = now;
        (action, y + 24.0)
    }

    /// A phone's sections menu: what the sidebar holds on a bigger screen.
    fn nav(&mut self, ui: &mut Ui, now: f64) -> Option<LandingAction> {
        let (at, opened) = self.nav_menu?;
        let mut action = None;
        let mut entries: Vec<String> = ["Get Started", "Tutorials", "Recents", "All Projects"].map(String::from).to_vec();
        entries.extend(self.places.iter().map(|p| format!("{} Backup", p.label)));
        entries.push("Show Projects Folder".into());
        // The computer's menu bar, as pages (the rest of it is on this screen already: Go is Back, Forward and the
        // sections above; View is Sort and Search; Edit has nothing to do here), and Preferences.
        let pages = PHONE_MENUS.len();
        entries.extend(PHONE_MENUS.iter().map(|t| format!("{t}  ›")));
        entries.push("Settings…".into());
        let refs: Vec<&str> = entries.iter().map(String::as_str).collect();
        let (chosen, rect) = popup_menu(ui, "sections-menu", at, &refs, "");
        let clicked_at = ui.input(|i| if i.pointer.any_click() { i.pointer.interact_pos() } else { None });
        if let Some(i) = chosen {
            self.search.clear();
            self.page.reset(0.0);
            let places = self.places.len();
            match i {
                0 => self.visit(Section::GetStarted, self.folder.clone()),
                1 => self.visit(Section::Tutorials, self.folder.clone()),
                2 => self.visit(Section::Recents, self.folder.clone()),
                3 => {
                    let root = self.root.clone();
                    self.go_to(root);
                }
                k if k < 4 + places => {
                    let dir = self.places[k - 4].dir.clone();
                    if dir.is_dir() {
                        self.go_to(dir);
                    } else {
                        self.toast = Some(("Nothing's backed up there yet (or it can't be reached now)".into(), now + 4.0));
                    }
                }
                k if k == 4 + places => self.toast = Some((format!("Your projects are kept in {}", shorten_home(&self.root)), now + 4.0)),
                k if k <= 4 + places + pages => self.visit(Section::Menu(PHONE_MENUS[k - 5 - places]), self.folder.clone()),
                _ => action = Some(LandingAction::Menu(crate::menubar::MenuAction::Ui(crate::workspace::toolbar::UiAction::Preferences))),
            }
        }
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if chosen.is_some() || esc || (now - opened > 0.2 && clicked_at.is_some_and(|q| !rect.contains(q))) {
            self.nav_menu = None;
        }
        action
    }

    /// The tile and sort menus, when open.
    fn menus(&mut self, ui: &mut Ui, now: f64) -> Option<LandingAction> {
        let clicked_at = ui.input(|i| if i.pointer.any_click() { i.pointer.interact_pos() } else { None });
        let mut action = None;
        if let Some(at) = self.sort_menu {
            let entries = [
                if self.sort == Sort::Date { "✓ Date Modified" } else { "   Date Modified" },
                if self.sort == Sort::Name { "✓ Name" } else { "   Name" },
                if self.tree { "✓ Folder Tree" } else { "   Folder Tree" },
            ];
            let (chosen, rect) = popup_menu(ui, "sort-menu", at, &entries, "");
            match chosen {
                Some(2) => {
                    // Folders as a tree on the left, or as big cards among the projects.
                    self.tree = !self.tree;
                    self.sort_menu = None;
                    if self.tree && self.section != Section::All {
                        let root = self.root.clone();
                        self.go_to(root);
                    }
                }
                Some(i) => {
                    self.sort = if i == 0 { Sort::Date } else { Sort::Name };
                    self.sort_reversed = false;
                    self.sort_menu = None;
                    self.rescan();
                }
                // (Not the tap on its own button: above it on a computer, beside it in a phone's column.)
                None if now - self.sort_opened > 0.2
                    && clicked_at.is_some_and(|p| !rect.contains(p) && p.y > at.y && (p.x < rect.right() || p.y > rect.bottom())) =>
                {
                    self.sort_menu = None
                }
                None => {}
            }
        }
        if let Some((tile, at, opened)) = self.menu.clone() {
            let (Tile::Project(dir) | Tile::Folder(dir)) = &tile;
            let backed_up = self.backup_of(dir).is_some();
            let split = !backed_up && self.projects.iter().any(|p| &p.dir == dir && p.versions > 0);
            let entries: &[&str] = match (&tile, backed_up) {
                (Tile::Project(_), false) if split => {
                    &["Open", "Rename", "Duplicate", "Show Versions", "Split Off Other Version", "Delete"]
                }
                (Tile::Project(_), false) => &["Open", "Rename", "Duplicate", "Show Versions", "Delete"],
                (Tile::Folder(_), false) => &["Open", "Rename", "Delete"],
                // A backup's copies: brought back, never changed in place.
                (Tile::Project(_), true) => &["Restore", "Restore and Open", SHOW_IN],
                (Tile::Folder(_), true) => &["Open", "Restore", SHOW_IN],
            };
            let (chosen, rect) = popup_menu(ui, "tile-menu", at, entries, "Delete");
            match chosen.map(|i| entries[i]) {
                Some("Open") => match &tile {
                    Tile::Project(d) => action = self.open_tile(d.clone(), now),
                    Tile::Folder(d) => self.go_to(d.clone()),
                },
                Some("Restore") => {
                    self.restore(dir, now);
                }
                Some("Restore and Open") => action = self.open_tile(dir.clone(), now),
                Some("Split Off Other Version") => {
                    for other in project::other_versions(dir) {
                        let r = project::split_version(dir, &other);
                        if let Some(to) = self.report(now, r) {
                            let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            self.toast = Some((format!("The other computer's version is now “{name}”"), now + 4.0));
                        }
                    }
                }
                Some(SHOW_IN) => reveal(dir.parent().unwrap_or(dir)),
                // The project's older versions, kept by the backups (Preferences ▸ Sync): opened like a folder,
                // each dated copy ready to Restore.
                Some("Show Versions") => match self.versions_of(dir) {
                    Some(at) => self.go_to(at),
                    None => {
                        self.toast = Some((
                            "No older versions yet. Backups keep them (Preferences ▸ Sync), once a saved copy is replaced".into(),
                            now + 5.0,
                        ))
                    }
                },
                Some("Rename") => {
                    let name = match &tile {
                        Tile::Project(d) => self.projects.iter().find(|p| &p.dir == d).map(|p| p.name.clone()),
                        Tile::Folder(d) => d.file_name().map(|n| n.to_string_lossy().into_owned()),
                    };
                    self.renaming = Some((dir.clone(), name.unwrap_or_default(), true));
                }
                Some("Duplicate") => {
                    let r = project::duplicate(dir);
                    self.report(now, r);
                }
                Some("Delete") => {
                    let r = project::trash(dir, &self.root);
                    if self.report(now, r).is_some() {
                        self.toast = Some((format!("Moved to {} in your projects folder", project::TRASH_DIR), now + 4.0));
                    }
                }
                _ => {}
            }
            if chosen.is_some() || (now - opened > 0.2 && clicked_at.is_some_and(|p| !rect.contains(p))) {
                self.menu = None;
            }
        }
        action
    }

    /// The name field under a tile being renamed.
    fn rename_field(&mut self, ui: &mut Ui, dir: &Path, tile: Rect, now: f64) {
        let Some((_, text, focus)) = &mut self.renaming else { return };
        let field = Rect::from_center_size(pos2(tile.center().x, tile.bottom() + 20.0), vec2(190.0, 22.0));
        let id = ui.scope_id().with(("rename-tile", dir));
        let resp = ui.put(field, egui::TextEdit::singleline(text).id(id).font(theme::body(12.0)));
        if std::mem::take(focus) {
            resp.request_focus();
            crate::workspace::select_all(ui.ctx(), id, text.chars().count());
        }
        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if esc {
            self.renaming = None;
        } else if enter || resp.lost_focus() {
            let name = text.clone();
            self.renaming = None;
            let r = project::rename(dir, &name);
            self.report(now, r);
        }
    }
}

pub fn load_png(path: &Path) -> Option<egui::ColorImage> {
    let file = std::fs::File::open(path).ok()?;
    decode_png(std::io::BufReader::new(file))
}

fn decode_png(source: impl std::io::BufRead + std::io::Seek) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(source);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let data = buf.get(..info.buffer_size())?;
    match info.color_type {
        png::ColorType::Rgba => Some(egui::ColorImage::from_rgba_unmultiplied([w, h], data)),
        png::ColorType::Rgb => Some(egui::ColorImage::from_rgb([w, h], data)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_whats_new_card_has_its_picture() {
        for e in crate::whats_new::ENTRIES {
            assert!(super::ART.iter().any(|(n, _)| *n == e.art), "What's new '{}' has no picture '{}' in ART", e.title, e.art);
        }
    }

    #[test]
    fn dates_format_like_the_reference() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
        assert_eq!(super::short_date(t), "9/21/26");
    }
}
