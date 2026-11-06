//! The first-run setup, after the device and navigation questions: how ShapeCraft looks, where projects live (the
//! Work Folder: this computer, or a cloud app's folder to have the same projects on every computer) and where they're
//! backed up. Everything here is in Preferences later (General, Sync).

use std::path::PathBuf;

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::cloud::Found;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

/// Where the person is in it, and what they picked so far.
#[derive(Clone, Debug)]
pub struct Setup {
    pub step: usize,
    pub theme: String,
    /// None: this computer (Documents/ShapeCraft).
    pub work: Option<PathBuf>,
    /// The cloud app the Work Folder is in (its key), when it is.
    pub work_cloud: Option<String>,
    pub backups: Vec<String>,
    pub after_change: bool,
}

impl Setup {
    pub fn new(s: &crate::settings::Settings) -> Self {
        Self {
            step: 0,
            theme: if s.theme.is_empty() { "System".into() } else { s.theme.clone() },
            work: None,
            work_cloud: None,
            backups: s.backups.clone(),
            after_change: s.backup_after_change,
        }
    }
}

const STEPS: [&str; 3] = ["Look", "Your Projects", "Backups"];

/// Draws it over everything. Returns true when Start Modeling is pressed (the picks are in `st`).
pub fn show(ui: &mut Ui, st: &mut Setup, found: Option<&Found>) -> bool {
    let full = ui.ctx().content_rect();
    let mut done = false;
    egui::Area::new(ui.scope_id().with("setup")).fixed_pos(full.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let _ = ui.allocate_exact_size(full.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(full, 0.0, Color32::from_black_alpha(150));
        let card = Rect::from_center_size(full.center(), vec2(720.0, 570.0));
        p.rect_filled(card.translate(vec2(0.0, 6.0)).expand(2.0), radius(14), Color32::from_black_alpha(90));
        p.rect_filled(card, radius(12), theme::c(0x232324));
        p.rect_stroke(card, radius(12), Stroke::new(1.0, theme::c(0x4b4b4b)), StrokeKind::Outside);
        // The steps across the top: a dot each, lit up to the current one.
        let n = STEPS.len() as f32;
        for (k, name) in STEPS.iter().enumerate() {
            let x = card.center().x + (k as f32 - (n - 1.0) / 2.0) * 130.0;
            let lit = k <= st.step;
            if k > 0 {
                let x0 = x - 130.0 + 10.0;
                p.line_segment(
                    [pos2(x0, card.top() + 30.0), pos2(x - 10.0, card.top() + 30.0)],
                    Stroke::new(1.5, if lit { Tokens::accent() } else { theme::c(0x46464a) }),
                );
            }
            p.circle_filled(pos2(x, card.top() + 30.0), 6.0, if lit { Tokens::accent() } else { theme::c(0x46464a) });
            p.text(
                pos2(x, card.top() + 50.0),
                Align2::CENTER_CENTER,
                *name,
                theme::body(11.0),
                if k == st.step { Tokens::text_strong() } else { Tokens::text_dim() },
            );
        }
        let body = Rect::from_min_max(pos2(card.left() + 40.0, card.top() + 76.0), pos2(card.right() - 40.0, card.bottom() - 70.0));
        match st.step {
            0 => look(ui, body, st),
            1 => projects(ui, body, st, found),
            _ => backups(ui, body, st, found),
        }
        // Back / Continue (Start Modeling on the last step).
        let last = st.step + 1 == STEPS.len();
        let go = Rect::from_min_size(pos2(card.right() - 40.0 - 150.0, card.bottom() - 52.0), vec2(150.0, 32.0));
        if pill(ui, go, if last { "Start Modeling" } else { "Continue" }, true, "setup-go") {
            if last {
                done = true;
            } else {
                st.step += 1;
            }
        }
        if st.step > 0 && pill(ui, Rect::from_min_size(pos2(go.left() - 110.0, go.top()), vec2(96.0, 32.0)), "Back", false, "setup-back") {
            st.step -= 1;
        }
        p.text(
            pos2(card.left() + 40.0, go.center().y),
            Align2::LEFT_CENTER,
            "All of this is in Preferences later.",
            theme::body(11.5),
            Tokens::text_dim(),
        );
    });
    done
}

fn title(p: &egui::Painter, body: Rect, big: &str, small: &str) {
    p.text(pos2(body.center().x, body.top() + 14.0), Align2::CENTER_CENTER, big, theme::strong(18.0), Tokens::text_strong());
    p.text(pos2(body.center().x, body.top() + 40.0), Align2::CENTER_CENTER, small, theme::body(12.5), Tokens::text_dim());
}

fn pill(ui: &mut Ui, r: Rect, label: &str, primary: bool, id: &str) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
    let fill = match (primary, resp.hovered()) {
        (true, false) => theme::c(0x2f55d4),
        (true, true) => theme::c(0x3a62e0),
        (false, false) => theme::c(0x3a3a3e),
        (false, true) => theme::c(0x45454a),
    };
    ui.painter().rect_filled(r, radius(8), fill);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, theme::strong(13.0), Color32::WHITE);
    resp.clicked()
}

/// A big choice: a tile with an outline that lights when it's picked or hovered.
fn tile(ui: &mut Ui, r: Rect, on: bool, id: &str) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, radius(10), if resp.hovered() { theme::c(0x34343a) } else { theme::c(0x2c2c2e) });
    let (w, c) = if on {
        (2.0, Tokens::accent())
    } else if resp.hovered() {
        (1.5, theme::c(0x5a5a60))
    } else {
        (1.0, theme::c(0x3e3e42))
    };
    p.rect_stroke(r, radius(10), Stroke::new(w, c), StrokeKind::Inside);
    resp.clicked()
}

/// Step 1: Light, Dark, or as the system is, each shown as a little window.
fn look(ui: &mut Ui, body: Rect, st: &mut Setup) {
    let p = ui.painter().clone();
    title(&p, body, "How should ShapeCraft look?", "Match your system, or keep it light or dark.");
    let names = [("System", "Match System"), ("Light", "Light"), ("Dark", "Dark")];
    let w = (body.width() - 40.0) / 3.0;
    for (k, (value, name)) in names.iter().enumerate() {
        let r = Rect::from_min_size(pos2(body.left() + k as f32 * (w + 20.0), body.top() + 80.0), vec2(w, 250.0));
        if tile(ui, r, st.theme == *value, &format!("setup-look-{k}")) {
            st.theme = (*value).to_string();
        }
        let art = Rect::from_center_size(pos2(r.center().x, r.top() + 100.0), vec2(w - 50.0, 130.0));
        match k {
            0 => {
                // Half and half.
                let (l, d) = (
                    Rect::from_min_max(art.min, pos2(art.center().x, art.bottom())),
                    Rect::from_min_max(pos2(art.center().x, art.top()), art.max),
                );
                window_art(&p, art, true, Some(l));
                window_art(&p, art, false, Some(d));
            }
            1 => window_art(&p, art, true, None),
            _ => window_art(&p, art, false, None),
        }
        p.text(pos2(r.center().x, r.bottom() - 30.0), Align2::CENTER_CENTER, *name, theme::strong(13.5), Tokens::text_strong());
    }
}

/// A tiny ShapeCraft window: sidebar, title strip and a part, in light or dark (clipped to `clip` for half of one).
fn window_art(p: &egui::Painter, r: Rect, light: bool, clip: Option<Rect>) {
    let p = match clip {
        Some(c) => p.with_clip_rect(c),
        None => p.clone(),
    };
    let (bg, side, strip, ink) = if light {
        (theme::c(0xf4f4f5), theme::c(0xe2e2e4), theme::c(0xffffff), theme::c(0x9a9aa0))
    } else {
        (theme::c(0x1e1e1f), theme::c(0x2a2a2c), theme::c(0x333335), theme::c(0x7a7a80))
    };
    p.rect_filled(r, radius(6), bg);
    p.rect_filled(Rect::from_min_size(r.min, vec2(r.width() * 0.28, r.height())), egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, side);
    p.rect_filled(
        Rect::from_min_max(pos2(r.left() + r.width() * 0.28, r.top()), pos2(r.right(), r.top() + 16.0)),
        egui::CornerRadius { ne: 6, ..Default::default() },
        strip,
    );
    for k in 0..4 {
        let y = r.top() + 14.0 + k as f32 * 12.0;
        p.rect_filled(Rect::from_min_size(pos2(r.left() + 7.0, y), vec2(r.width() * 0.18, 5.0)), radius(2), ink.gamma_multiply(0.6));
    }
    // A block with a hole, lit from the upper left.
    let c = pos2(r.left() + r.width() * 0.64, r.center().y + 10.0);
    let top = [c + vec2(-30.0, -10.0), c + vec2(0.0, -24.0), c + vec2(30.0, -10.0), c + vec2(0.0, 4.0)];
    let left = [c + vec2(-30.0, -10.0), c + vec2(0.0, 4.0), c + vec2(0.0, 30.0), c + vec2(-30.0, 16.0)];
    let right = [c + vec2(0.0, 4.0), c + vec2(30.0, -10.0), c + vec2(30.0, 16.0), c + vec2(0.0, 30.0)];
    let shade = |v: u32| if light { theme::c(v) } else { theme::c(v).gamma_multiply(0.8) };
    p.add(egui::Shape::convex_polygon(top.to_vec(), shade(0xd8dbe3), Stroke::NONE));
    p.add(egui::Shape::convex_polygon(left.to_vec(), shade(0xa9adb8), Stroke::NONE));
    p.add(egui::Shape::convex_polygon(right.to_vec(), shade(0x8c909b), Stroke::NONE));
    p.circle_filled(c + vec2(0.0, -10.0), 5.0, shade(0x6f727c));
    p.rect_stroke(r, radius(6), Stroke::new(1.0, theme::c(0x55555a)), StrokeKind::Inside);
}

/// Step 2: the Work Folder. This computer, or a cloud app's folder (same projects and Recents everywhere).
fn projects(ui: &mut Ui, body: Rect, st: &mut Setup, found: Option<&Found>) {
    let p = ui.painter().clone();
    title(&p, body, "Where should your projects live?", "Projects save here as you work. This is your Work Folder.");
    let mut y = body.top() + 74.0;
    let home = crate::app::Options::default_root();
    let choice = |ui: &mut Ui, y: f32, on: bool, icon: Icon, name: &str, detail: &str, id: &str| -> bool {
        let r = Rect::from_min_size(pos2(body.left(), y), vec2(body.width(), 50.0));
        let hit = tile(ui, r, on, id);
        let p = ui.painter();
        icons::paint(p, Rect::from_center_size(pos2(r.left() + 28.0, r.center().y), vec2(18.0, 18.0)), icon, Tokens::icon_blue());
        p.text(pos2(r.left() + 52.0, r.center().y - 9.0), Align2::LEFT_CENTER, name, theme::strong(13.0), Tokens::text_strong());
        p.text(pos2(r.left() + 52.0, r.center().y + 10.0), Align2::LEFT_CENTER, detail, theme::body(11.0), Tokens::text_dim());
        if on {
            p.circle_filled(pos2(r.right() - 24.0, r.center().y), 7.0, Tokens::accent());
            p.circle_filled(pos2(r.right() - 24.0, r.center().y), 2.5, Color32::WHITE);
        } else {
            p.circle_stroke(pos2(r.right() - 24.0, r.center().y), 7.0, Stroke::new(1.2, theme::c(0x6a6a70)));
        }
        hit
    };
    if choice(
        ui,
        y,
        st.work.is_none(),
        Icon::Folder,
        "On This Computer",
        &format!("{}  ·  fastest, works offline. Recommended.", crate::landing::shorten_home(&home)),
        "setup-work-local",
    ) {
        (st.work, st.work_cloud) = (None, None);
    }
    y += 58.0;
    let Some(found) = found else {
        p.text(
            pos2(body.left() + 4.0, y + 14.0),
            Align2::LEFT_CENTER,
            "Looking for cloud storage apps…",
            theme::body(12.0),
            Tokens::text_dim(),
        );
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        return;
    };
    let ready: Vec<_> = found.ready().cloned().collect();
    for (k, c) in ready.iter().take(4).enumerate() {
        let dir = c.root.join("ShapeCraft");
        let on = st.work.as_ref() == Some(&dir);
        let detail = "Same projects and Recents on every computer signed in to it.".to_string();
        if choice(ui, y, on, Icon::Cloud, &format!("In {}", c.label()), &detail, &format!("setup-work-{k}")) {
            st.work = Some(dir);
            st.work_cloud = Some(c.key());
            st.backups.retain(|b| *b != c.key());
        }
        y += 58.0;
    }
    if let Some(chosen) = st.work.clone().filter(|w| st.work_cloud.is_none() && ready.iter().all(|c| c.root.join("ShapeCraft") != *w)) {
        p.text(
            pos2(body.left() + 4.0, y + 6.0),
            Align2::LEFT_CENTER,
            format!("Chosen: {}", crate::landing::shorten_home(&chosen)),
            theme::body(11.5),
            Tokens::accent(),
        );
    }
    let r = Rect::from_min_size(pos2(body.right() - 170.0, body.bottom() - 34.0), vec2(170.0, 28.0));
    if pill(ui, r, "Choose a Folder…", false, "setup-work-pick")
        && let Some(dir) = rfd::FileDialog::new().set_title("Choose your Work Folder").pick_folder()
    {
        (st.work, st.work_cloud) = (Some(dir), None);
    }
    // What was found on this computer, in one line.
    let names: Vec<String> =
        found.clouds.iter().filter(|c| c.state == crate::cloud::State::Ready).map(|c| c.provider.name().to_string()).collect();
    let mut names = names;
    names.dedup();
    let line = if names.is_empty() {
        "No cloud storage apps found (a NAS or any folder works too).".to_string()
    } else {
        format!("Found on this computer: {}.", names.join(", "))
    };
    p.text(pos2(body.left(), r.center().y), Align2::LEFT_CENTER, line, theme::body(11.0), Tokens::text_dim());
}

/// Step 3: backups, each a cloud app's folder or any folder (a NAS), and when they're made.
fn backups(ui: &mut Ui, body: Rect, st: &mut Setup, found: Option<&Found>) {
    let p = ui.painter().clone();
    title(&p, body, "Keep a backup?", "A copy of every project, kept up to date somewhere else, with older versions to go back to.");
    let mut y = body.top() + 76.0;
    let ready: Vec<_> = found.map(|f| f.ready().cloned().collect()).unwrap_or_default();
    for (k, c) in ready.iter().enumerate().take(4) {
        if st.work_cloud.as_deref() == Some(c.key().as_str()) {
            continue;
        }
        let key = c.key();
        let on = st.backups.contains(&key);
        let r = Rect::from_min_size(pos2(body.left(), y), vec2(body.width(), 40.0));
        if tile(ui, r, on, &format!("setup-backup-{k}")) {
            if on {
                st.backups.retain(|b| *b != key);
            } else {
                st.backups.push(key.clone());
            }
        }
        check_box(&p, pos2(r.left() + 22.0, r.center().y), on);
        icons::paint(&p, Rect::from_center_size(pos2(r.left() + 50.0, r.center().y), vec2(16.0, 16.0)), Icon::Cloud, Tokens::icon_blue());
        p.text(pos2(r.left() + 68.0, r.center().y), Align2::LEFT_CENTER, c.label(), theme::strong(13.0), Tokens::text_strong());
        if let Some(n) = st.backups.iter().position(|b| *b == key) {
            p.text(
                pos2(r.right() - 16.0, r.center().y),
                Align2::RIGHT_CENTER,
                format!("Backup {}", n + 1),
                theme::body(11.0),
                Tokens::text_dim(),
            );
        }
        y += 48.0;
    }
    // Folders added (a NAS share, another drive).
    let folders: Vec<String> = st.backups.iter().filter(|b| b.starts_with("folder:")).cloned().collect();
    for f in folders {
        let t = crate::backup::Target::parse(&f);
        let r = Rect::from_min_size(pos2(body.left(), y), vec2(body.width(), 40.0));
        if tile(ui, r, true, &format!("setup-backup-{f}")) {
            st.backups.retain(|b| *b != f);
        }
        check_box(&p, pos2(r.left() + 22.0, r.center().y), true);
        icons::paint(&p, Rect::from_center_size(pos2(r.left() + 50.0, r.center().y), vec2(16.0, 16.0)), Icon::Folder, Tokens::icon_blue());
        p.text(
            pos2(r.left() + 68.0, r.center().y),
            Align2::LEFT_CENTER,
            t.label(&Found::default()),
            theme::strong(13.0),
            Tokens::text_strong(),
        );
        y += 48.0;
    }
    let r = Rect::from_min_size(pos2(body.left(), y + 2.0), vec2(190.0, 28.0));
    if pill(ui, r, "Add a Folder or NAS…", false, "setup-backup-folder")
        && let Some(dir) = rfd::FileDialog::new().set_title("Choose a folder to back up to").pick_folder()
    {
        let s = crate::backup::Target::Folder(dir).setting();
        if !st.backups.contains(&s) {
            st.backups.push(s);
        }
    }
    // When: on Save (⌘S), closing and quitting leaves a gap to go back to; every change keeps it closest.
    let y = body.bottom() - 58.0;
    p.text(pos2(body.left(), y), Align2::LEFT_CENTER, "Back up:", theme::strong(12.5), Tokens::text_strong());
    let save_key = if cfg!(target_os = "macos") { "⌘S" } else { "Ctrl+S" };
    let opts = [(false, format!("When I press {save_key}, close a project or quit")), (true, "A moment after every change".to_string())];
    for (k, (value, text)) in opts.iter().enumerate() {
        let at = pos2(body.left() + 80.0, y + k as f32 * 24.0);
        let hit = Rect::from_min_size(pos2(at.x - 10.0, at.y - 10.0), vec2(360.0, 20.0));
        if ui.interact(hit, ui.scope_id().with(("setup-when", k)), Sense::click()).clicked() {
            st.after_change = *value;
        }
        let on = st.after_change == *value;
        if on {
            p.circle_filled(at, 6.0, Tokens::accent());
            p.circle_filled(at, 2.2, Color32::WHITE);
        } else {
            p.circle_stroke(at, 6.0, Stroke::new(1.2, theme::c(0x6a6a70)));
        }
        p.text(pos2(at.x + 14.0, at.y), Align2::LEFT_CENTER, text, theme::body(12.0), Tokens::text());
    }
}

fn check_box(p: &egui::Painter, c: egui::Pos2, on: bool) {
    let r = Rect::from_center_size(c, vec2(16.0, 16.0));
    if on {
        p.rect_filled(r, radius(4), Tokens::accent());
        p.line(vec![c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.5), c + vec2(4.5, -3.5)], Stroke::new(1.8, Color32::WHITE));
    } else {
        p.rect_stroke(r, radius(4), Stroke::new(1.2, theme::c(0x6a6a70)), StrokeKind::Inside);
    }
}
