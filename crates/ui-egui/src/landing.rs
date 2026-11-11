//! The projects screen. Same shape as the reference app's, but it browses a folder on your disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, pos2, vec2};
use shapecraft_doc::project::{self, ProjectSummary};

use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, hex, radius};
use crate::window_chrome;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    GetStarted,
    Tutorials,
    Recents,
    All,
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
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
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
    whats_new_scroll: f32,
    whats_new_open: Option<usize>,
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
            menu: None,
            renaming: None,
            sidebar_focus: false,
            tutorials_done: Vec::new(),
            art: HashMap::new(),
            whats_new_scroll: 0.0,
            whats_new_open: None,
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
            HomeCmd::Back => {
                if self.section == Section::All
                    && let Some(prev) = self.back.pop()
                {
                    self.forward.push(std::mem::replace(&mut self.folder, prev));
                    self.rescan();
                }
            }
            HomeCmd::Forward => {
                if self.section == Section::All
                    && let Some(next) = self.forward.pop()
                {
                    self.back.push(std::mem::replace(&mut self.folder, next));
                    self.rescan();
                }
            }
            HomeCmd::EnclosingFolder => {
                if self.section == Section::All
                    && !self.roots().contains(&self.folder)
                    && let Some(up) = self.folder.parent().map(Path::to_path_buf)
                {
                    self.go_to(up);
                }
            }
            HomeCmd::Recents => {
                self.section = Section::Recents;
                self.rescan();
            }
            HomeCmd::Projects => {
                self.section = Section::All;
                self.rescan();
            }
            HomeCmd::Sort(i) => {
                self.sort = if i < 2 { Sort::Date } else { Sort::Name };
                self.sort_reversed = i % 2 == 1;
                self.rescan();
            }
            HomeCmd::Search => self.focus_search = true,
        }
    }

    fn go_to(&mut self, folder: PathBuf) {
        if folder != self.folder {
            self.back.push(std::mem::replace(&mut self.folder, folder));
            self.forward.clear();
        }
        self.section = Section::All;
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
        // Measured (s23 landing): #1E1E1E (light #FFFFFF) under a #373636 bar.
        p.rect_filled(full, 0.0, theme::c(0x1e1e1e));
        let mut action = None;

        // Sidebar (measured, s30-011/012 Light, D-landing Dark). The macOS window it sits in greys its sidebar,
        // title strip and selection when it isn't the key window, and the selected row is blue only while the
        // sidebar has the keyboard (the last click was in it).
        let focused = ui.input(|i| i.focused);
        let light = theme::is_light();
        if ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| p.x > full.left() + 250.0)) {
            self.sidebar_focus = false;
        }
        let side = Rect::from_min_size(full.min, vec2(250.0, full.height()));
        let side_fill = match (light, focused) {
            (true, true) => hex(0xe1e0e0),
            (true, false) => hex(0xe8e7e7),
            (false, _) => Tokens::landing_sidebar(),
        };
        p.rect_filled(side, 0.0, side_fill);
        p.line_segment([side.right_top(), side.right_bottom()], Stroke::new(1.0, if light { hex(0xc2c0c0) } else { hex(0x000000) }));
        // Where the reference has its team: a grey square with a white letter, the name, and a short line under it.
        let badge = Rect::from_min_size(pos2(side.left() + 16.0, side.top() + 59.0), vec2(32.0, 32.0));
        p.rect_filled(badge, radius(6), hex(0x808080));
        p.text(badge.center(), Align2::CENTER_CENTER, "S", theme::strong(17.0), Color32::WHITE);
        p.text(pos2(badge.right() + 9.0, badge.top() + 8.0), Align2::LEFT_CENTER, "ShapeCraft", theme::strong(13.5), Tokens::text_strong());
        let folder = self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| shorten_home(&self.root));
        let mut job = egui::text::LayoutJob::simple_singleline(format!("Local · {folder}"), theme::body(10.0), Tokens::text_dim());
        job.wrap = egui::text::TextWrapping {
            max_width: side.right() - badge.right() - 21.0,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let g = ui.fonts_mut(|f| f.layout_job(job));
        p.galley(pos2(badge.right() + 9.0, badge.top() + 24.0 - g.size().y / 2.0), g, Tokens::text_dim());
        // The reference's Upgrade button (left out) holds the projects folder instead, so the spacing stays.
        let folder_button = Rect::from_min_max(pos2(side.left() + 10.0, side.top() + 101.0), pos2(side.left() + 240.0, side.top() + 121.0));
        let fb = ui.interact(folder_button, ui.scope_id().with("projects-folder"), Sense::click());
        let fb_fill = match (light, fb.hovered()) {
            (true, false) => hex(0xcac9c9),
            (true, true) => hex(0xbebdbd),
            (false, false) => hex(0x5c5c5c),
            (false, true) => hex(0x676767),
        };
        p.rect_filled(folder_button, radius(5), fb_fill);
        p.text(folder_button.center(), Align2::CENTER_CENTER, "Show Projects Folder", theme::strong(12.0), Tokens::text_strong());
        if fb.clicked() {
            if crate::tablet::on_tablet() {
                // The folder is on the computer serving this page, not on the tablet.
                self.toast = Some((format!("Your projects are kept on the computer, in {}", shorten_home(&self.root)), f64::MAX));
            } else {
                reveal(&self.root);
            }
        }

        let search_rect = Rect::from_min_max(pos2(side.left() + 10.0, side.top() + 138.0), pos2(side.left() + 240.0, side.top() + 164.0));
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
        let searched = ui
            .put(field, egui::TextEdit::singleline(&mut self.search).hint_text("Search").frame(egui::Frame::NONE).font(theme::body(12.0)));
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
                label,
                theme::body(12.5),
                if on_blue { Color32::WHITE } else { Tokens::text() },
            );
            resp.clicked()
        };
        let searching = !self.search.is_empty();
        let here = self.backup_of(&self.folder).map(|p| p.dir.clone());
        let local = "All Projects";
        let rows = [
            (186.0, Icon::GetStarted, "Get Started", Section::GetStarted, "get-started"),
            (215.0, Icon::Tutorials, "Tutorials", Section::Tutorials, "tutorials"),
            (266.0, Icon::Recents, "Recents", Section::Recents, "recents"),
            (294.0, Icon::Projects, local, Section::All, "all"),
        ];
        // The backups, first first, each opening its ShapeCraft Backups folder (every computer's copies).
        let mut open_place = None;
        if !self.places.is_empty() {
            p.text(pos2(side.left() + 16.0, side.top() + 337.0), Align2::LEFT_CENTER, "Backups", theme::strong(10.5), Tokens::text_faint());
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
                    self.section = section;
                    self.rescan();
                }
            }
        }
        p.text(pos2(side.left() + 16.0, side.top() + 243.0), Align2::LEFT_CENTER, "Projects", theme::strong(10.5), Tokens::text_faint());

        // Content: the title strip (white with no line in a key window, grey with a line otherwise, as measured).
        let content = Rect::from_min_max(pos2(side.right(), full.top()), full.max);
        let strip = Rect::from_min_max(content.min, pos2(content.right(), content.top() + 51.0));
        let (strip_fill, strip_line) = match (light, focused) {
            (true, true) => (hex(0xffffff), None),
            (true, false) => (hex(0xe8e7e7), Some(hex(0xd9d9d9))),
            (false, _) => (theme::c(0x373636), Some(hex(0x000000))),
        };
        p.rect_filled(strip, 0.0, strip_fill);
        let folder_name = self.folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let title = match (self.search.is_empty(), self.section) {
            (false, _) => "Search".to_string(),
            (true, Section::GetStarted) => "Get Started".to_string(),
            (true, Section::Tutorials) => "Tutorials".to_string(),
            (true, Section::Recents) => "Recents".to_string(),
            (true, Section::All) if self.folder == self.root => local.to_string(),
            (true, Section::All) if let Some(p) = self.places.iter().find(|p| p.dir == self.folder) => format!("{} Backup", p.label),
            (true, Section::All) => folder_name,
        };
        let chevron = |ui: &mut Ui, x: f32, icon: Icon, enabled: bool, id: &str| -> bool {
            let r = Rect::from_center_size(pos2(content.left() + x, content.top() + 25.0), vec2(24.0, 24.0));
            let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
            if enabled && resp.hovered() {
                ui.painter().rect_filled(r, radius(6), Tokens::button_hover());
            }
            icons::paint(ui.painter(), r.shrink(5.0), icon, if enabled { Tokens::text_dim() } else { Tokens::text_faint() });
            enabled && resp.clicked()
        };
        let in_folders = self.section == Section::All;
        if chevron(ui, 23.0, Icon::ChevronLeft, in_folders && !self.back.is_empty(), "back")
            && let Some(prev) = self.back.pop()
        {
            self.forward.push(std::mem::replace(&mut self.folder, prev));
            self.rescan();
        }
        if chevron(ui, 54.0, Icon::ChevronRight, in_folders && !self.forward.is_empty(), "forward")
            && let Some(next) = self.forward.pop()
        {
            self.back.push(std::mem::replace(&mut self.folder, next));
            self.rescan();
        }
        p.text(pos2(content.left() + 75.0, content.top() + 25.0), Align2::LEFT_CENTER, title, theme::strong(13.0), Tokens::text());
        if let Some(line) = strip_line {
            p.line_segment(
                [pos2(content.left(), content.top() + 51.0), pos2(content.right(), content.top() + 51.0)],
                Stroke::new(1.0, line),
            );
        }
        // Clear of the window's own buttons, where it has them.
        let bar_right = content.right() - window_chrome::buttons_w();
        if window_chrome::own_frame() {
            let title_end = pos2(content.left() + 320.0, content.top());
            window_chrome::drag_area(ui, Rect::from_min_max(title_end, pos2(bar_right - 130.0, content.top() + 51.0)), "home-drag");
        }
        let tb = |ui: &mut Ui, i: usize, icon: Icon, id: &str| -> bool {
            let r = Rect::from_center_size(pos2(bar_right - 32.0 - i as f32 * 38.0, content.top() + 25.0), vec2(26.0, 26.0));
            let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, radius(6), Tokens::button_hover());
            }
            icons::paint(ui.painter(), r.shrink(5.0), icon, Tokens::text_dim());
            resp.clicked()
        };
        if tb(ui, 0, Icon::Sort, "sort") {
            self.sort_menu = Some(pos2(bar_right - 190.0, content.top() + 42.0));
            self.menu = None;
        }
        if tb(ui, 1, Icon::NewFolder, "folder") {
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
        if tb(ui, 2, Icon::Import, "import") {
            action = Some(LandingAction::Import);
        }
        if tb(ui, 3, Icon::Plus, "new") {
            action = Some(LandingAction::New(self.target_dir()));
        }

        let left = content.left() + 33.0;
        let mut y = content.top() + 94.0;
        if self.search.is_empty() && matches!(self.section, Section::GetStarted | Section::Tutorials) {
            if let Some(a) = self.learn_page(ui, content, left, now) {
                action = Some(a);
            }
            self.toast_and_detail(ui, content, full, now);
            return action;
        }
        if self.search.is_empty() && self.section == Section::Recents {
            p.text(pos2(left, y), Align2::LEFT_CENTER, "What's new", theme::strong(16.0), Tokens::text_strong());
            self.whats_new_row(ui, Rect::from_min_max(pos2(content.left(), y + 26.0), pos2(content.right(), y + 34.0 + 219.0)), left);
            y += 252.0;
            p.text(pos2(left, y + 20.0), Align2::LEFT_CENTER, "Recent Projects", theme::strong(16.0), Tokens::text_strong());
            y += 52.0;
        }

        let q = self.search.to_lowercase();
        let mut list: Vec<ProjectSummary> =
            self.projects.iter().filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q)).cloned().collect();
        if self.section == Section::Recents && q.is_empty() {
            list.truncate(15);
        }
        // Folders come first when browsing, as tiles of their own.
        let mut tiles: Vec<Tile> = self.folders.iter().cloned().map(Tile::Folder).collect();
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
            p.text(c, Align2::CENTER_CENTER, line, theme::strong(13.0), Tokens::text());
            if let Some(hint) = hint {
                p.text(c + vec2(0.0, 20.0), Align2::CENTER_CENTER, hint, theme::body(11.0), Tokens::text_dim());
            }
        } else if tiles.is_empty() && !self.search.is_empty() {
            // As the reference words and places it, in the middle of the page (26.202, s39-refsearch).
            let c = content.center() - vec2(0.0, 10.0);
            p.text(c, Align2::CENTER_CENTER, format!("No results for \"{}\"", self.search), theme::strong(13.0), Tokens::text());
            p.text(
                c + vec2(0.0, 20.0),
                Align2::CENTER_CENTER,
                "Check the spelling or try a new search.",
                theme::body(11.0),
                Tokens::text_dim(),
            );
        } else if tiles.is_empty() {
            let c = pos2(content.center().x, y + 120.0);
            p.text(c, Align2::CENTER_CENTER, "No projects yet", theme::strong(13.0), Tokens::text());
            p.text(
                c + vec2(0.0, 20.0),
                Align2::CENTER_CENTER,
                if crate::tablet::shows_shortcuts() {
                    format!("Press + or {} to start one", theme::key_text("⌘N"))
                } else {
                    "Tap + to start one".to_string()
                },
                theme::body(11.0),
                Tokens::text_dim(),
            );
        }
        // As observed (L12): as many columns as fit at 195 px or more, the cards stretched to fill the row (200 px
        // at 1840 wide, 215 at 1000), 21 px apart, pictures 0.74 as tall as wide.
        // A tablet gets bigger cards, easier to tap (three across an iPad in landscape).
        let pitch = if crate::tablet::on_tablet() { 300.0 } else { 216.0 };
        let avail = content.width() - 65.0;
        let cols = ((avail + 21.0) / pitch).floor().max(1.0) as usize;
        let card_w = ((avail - 21.0 * (cols as f32 - 1.0)) / cols as f32).max(120.0);
        let card_h = (card_w * 0.74).round();
        for (i, t) in tiles.iter().enumerate() {
            let (cx, cy) = (i % cols, i / cols);
            let tile = Rect::from_min_size(pos2(left + cx as f32 * (card_w + 21.0), y + cy as f32 * (card_h + 76.0)), vec2(card_w, card_h));
            if tile.top() > full.bottom() {
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
                        (Some(p), _) => format!("{} Backup", p.label),
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
            } else if resp.double_clicked() && !editing {
                match t {
                    Tile::Folder(d) => self.go_to(d.clone()),
                    Tile::Project(d) => action = self.open_tile(d.clone(), now),
                }
            } else if resp.clicked() && !editing {
                self.selected = Some(tile_dir);
            }
        }
        if let Some(a) = self.menus(ui, now) {
            action = Some(a);
        }
        // Return opens the picked card, Escape lets it go.
        if self.renaming.is_none() && action.is_none() && !ui.ctx().egui_wants_keyboard_input() {
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if esc {
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
                let g = p.layout_no_wrap(text.clone(), theme::body(12.0), Tokens::text());
                let r = Rect::from_center_size(pos2(content.center().x, full.bottom() - 40.0), vec2(g.size().x + 24.0, 30.0));
                p.rect_filled(r, radius(8), Tokens::banner());
                p.text(r.center(), Align2::CENTER_CENTER, text, theme::body(12.0), Tokens::text());
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
            p.text(card.min + vec2(24.0, 26.0), Align2::LEFT_CENTER, e.title, theme::strong(18.0), Color32::WHITE);
            p.text(card.min + vec2(24.0, 50.0), Align2::LEFT_CENTER, format!("New in {}", e.version), theme::body(11.5), hex(0xa9a9ab));
            let g = p.layout(e.text.to_string(), theme::body(12.5), hex(0xdfdfdf), card.width() - 48.0);
            p.galley(pos2(card.left() + 24.0, pic.bottom() + 16.0), g, hex(0xdfdfdf));
            let b = Rect::from_min_size(pos2(card.right() - 104.0, card.bottom() - 46.0), vec2(80.0, 28.0));
            let r = ui.interact(b, ui.scope_id().with("whats-new-close"), Sense::click());
            p.rect_filled(b, radius(14), hex(if r.hovered() { 0x2a5cff } else { 0x1d4fff }));
            p.text(b.center(), Align2::CENTER_CENTER, "Close", theme::strong(12.5), Color32::WHITE);
            // Every version's notes, in the changelog.
            let notes = Rect::from_min_size(pos2(card.left() + 24.0, card.bottom() - 46.0), vec2(150.0, 28.0));
            let n = ui.interact(notes, ui.scope_id().with("whats-new-notes"), Sense::click());
            let nc = if n.hovered() { hex(0xffffff) } else { hex(0xa9a9ab) };
            p.text(notes.left_center(), Align2::LEFT_CENTER, "All release notes", theme::body(12.0), nc);
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
    fn whats_new_row(&mut self, ui: &mut Ui, row: Rect, left: f32) {
        let entries = crate::whats_new::ENTRIES;
        let width = entries.len() as f32 * 190.0 - 20.0;
        let room = (row.right() - 33.0 - left).max(170.0);
        let resp = ui.interact(row, ui.scope_id().with("whats-new-row"), Sense::hover());
        if resp.hovered() {
            let d = ui.input(|i| i.smooth_scroll_delta);
            let d = if d.x.abs() > 0.0 { d.x } else { d.y };
            self.whats_new_scroll = (self.whats_new_scroll - d).clamp(0.0, (width - room).max(0.0));
        }
        let clip = Rect::from_min_max(pos2(row.left() + 1.0, row.top()), row.max);
        let p = ui.painter().with_clip_rect(clip);
        for (i, e) in entries.iter().enumerate() {
            let r = Rect::from_min_size(pos2(left + i as f32 * 190.0 - self.whats_new_scroll, row.top() + 8.0), vec2(170.0, 219.0));
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
                p.layout(e.title.to_string(), theme::strong(14.0), Color32::WHITE, 140.0),
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
    fn learn_page(&mut self, ui: &mut Ui, content: Rect, left: f32, now: f64) -> Option<LandingAction> {
        let _ = (content, now);
        let p = ui.painter().clone();
        let mut action = None;
        let tutorials_page = self.section == Section::Tutorials;
        let mut y = content.top() + 94.0;
        let heading = |y: f32, text: &str| {
            p.text(pos2(left, y), Align2::LEFT_CENTER, text, theme::strong(16.0), Tokens::text_strong());
        };
        heading(y, if tutorials_page { "In-App Tutorials" } else { "New to CAD" });
        let fill = if tutorials_page { hex(0x333333) } else { hex(0x1d4fff) };
        for (i, lesson) in crate::tutorials::LESSONS.iter().enumerate() {
            let r = Rect::from_min_size(pos2(left + i as f32 * 190.0, y + 34.0), vec2(170.0, 219.0));
            let hit = ui.interact(r, ui.scope_id().with(("lesson", i)), Sense::click());
            p.rect_filled(r, radius(5), fill);
            if let Some(tex) = self.art(ui.ctx(), &format!("tut-{}", lesson.id)) {
                let pic = Rect::from_min_size(pos2(r.left() + 18.0, r.top() + 44.0), vec2(152.0, 196.0));
                p.with_clip_rect(r).image(tex, pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            p.galley(
                pos2(r.left() + 16.0, r.top() + 15.0),
                p.layout(lesson.title.to_string(), theme::strong(14.0), Color32::WHITE, 140.0),
                Color32::WHITE,
            );
            p.text(
                pos2(r.left() + 16.0, r.bottom() - 21.0),
                Align2::LEFT_CENTER,
                format!("{} min", lesson.minutes),
                theme::strong(10.5),
                Color32::WHITE,
            );
            // ▶ in a ring, bottom right; a tick instead once it's been done.
            let c = pos2(r.right() - 25.0, r.bottom() - 21.0);
            p.circle_stroke(c, 7.5, Stroke::new(1.4, Color32::WHITE));
            if self.tutorials_done.iter().any(|d| d == lesson.id) {
                p.line_segment([c + vec2(-3.4, 0.2), c + vec2(-0.9, 2.8)], Stroke::new(1.6, Color32::WHITE));
                p.line_segment([c + vec2(-0.9, 2.8), c + vec2(3.6, -2.6)], Stroke::new(1.6, Color32::WHITE));
            } else {
                p.add(egui::Shape::convex_polygon(
                    vec![c + vec2(-2.2, -3.6), c + vec2(3.8, 0.0), c + vec2(-2.2, 3.6)],
                    Color32::WHITE,
                    Stroke::NONE,
                ));
            }
            if hit.hovered() {
                p.rect_stroke(r, radius(5), Stroke::new(2.0, Color32::from_white_alpha(70)), StrokeKind::Inside);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hit.clicked() {
                action = Some(LandingAction::Tutorial(lesson.id));
            }
        }
        if tutorials_page {
            return action;
        }
        y += 34.0 + 219.0 + 53.0;
        heading(y, "Coming from another CAD");
        let cards = [("Navigation Presets", LandingAction::NavigationPresets), ("Keyboard Shortcuts", LandingAction::Shortcuts)];
        for (i, (title, act)) in cards.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(left + i as f32 * 190.0, y + 34.0), vec2(170.0, 219.0));
            let hit = ui.interact(r, ui.scope_id().with(("from-cad", i)), Sense::click());
            p.rect_filled(r, radius(5), hex(0x505050));
            p.galley(
                pos2(r.left() + 16.0, r.top() + 15.0),
                p.layout(title.to_string(), theme::strong(14.0), Color32::WHITE, 140.0),
                Color32::WHITE,
            );
            if i == 0 {
                // The presets to choose from, in a 2 × 2 grid as the reference draws its own.
                let grid = Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 62.0), vec2(138.0, 138.0));
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
                // Key caps in a checkerboard, as the reference's card has them.
                let keys = ["⌘F", "", "E", "", "⇧⌘A", "F", "", "⌥⌘P", "", "R", "⌘1", ""];
                for (k, key) in keys.iter().enumerate() {
                    let cell = Rect::from_min_size(
                        pos2(r.left() + (k % 4) as f32 * 42.5, r.top() + 88.0 + (k / 4) as f32 * 42.5),
                        vec2(42.5, 42.5),
                    );
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
        action
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
        self.search.clear();
        self.section = Section::Tutorials;
        self.rescan();
    }

    pub fn show_section(&mut self, name: &str) -> Result<(), String> {
        self.search.clear();
        match name {
            "get_started" => self.section = Section::GetStarted,
            "tutorials" => self.section = Section::Tutorials,
            "recents" => self.section = Section::Recents,
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
    ("tut-phone-stand", include_bytes!("../../../assets/home/tut-phone-stand.png")),
    ("tut-pencil-cup", include_bytes!("../../../assets/home/tut-pencil-cup.png")),
    ("tut-key-tag", include_bytes!("../../../assets/home/tut-key-tag.png")),
    ("new-navigation", include_bytes!("../../../assets/home/new-navigation.png")),
    ("new-tutorials", include_bytes!("../../../assets/home/new-tutorials.png")),
    ("new-view-cube", include_bytes!("../../../assets/home/new-view-cube.png")),
    ("new-cards", include_bytes!("../../../assets/home/new-cards.png")),
    ("new-copies", include_bytes!("../../../assets/home/new-copies.png")),
    ("new-languages", include_bytes!("../../../assets/home/new-languages.png")),
    ("new-shapr", include_bytes!("../../../assets/home/new-shapr.png")),
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
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(url).spawn();
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
            ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, *label, theme::body(12.5), col);
            if r.clicked() {
                chosen = Some(i);
            }
        }
    });
    (chosen, shown.response.rect)
}

impl Landing {
    /// The tile and sort menus, when open.
    fn menus(&mut self, ui: &mut Ui, now: f64) -> Option<LandingAction> {
        let clicked_at = ui.input(|i| if i.pointer.any_click() { i.pointer.interact_pos() } else { None });
        let mut action = None;
        if let Some(at) = self.sort_menu {
            let entries = [
                if self.sort == Sort::Date { "✓ Date Modified" } else { "   Date Modified" },
                if self.sort == Sort::Name { "✓ Name" } else { "   Name" },
            ];
            let (chosen, rect) = popup_menu(ui, "sort-menu", at, &entries, "");
            match chosen {
                Some(i) => {
                    self.sort = if i == 0 { Sort::Date } else { Sort::Name };
                    self.sort_reversed = false;
                    self.sort_menu = None;
                    self.rescan();
                }
                None if clicked_at.is_some_and(|p| !rect.contains(p) && p.y > at.y) => self.sort_menu = None,
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
    fn dates_format_like_the_reference() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
        assert_eq!(super::short_date(t), "9/21/26");
    }
}
