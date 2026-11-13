//! Updates in the window: a quiet pill in the bottom-right corner while a new version downloads and once it's ready,
//! a card with its notes, and the question when ShapeCraft closes with an update waiting. The flow itself (checks,
//! downloads, installing) is `shapecraft_update::Updater`.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use shapecraft_update::{Install, Phase, Prefs, Snapshot, Updater};

use crate::settings::Settings;
use crate::theme::{self, Tokens, radius};

/// What closing does with a ready update.
pub const ON_CLOSE: [&str; 2] = ["Ask", "Always Update"];

/// A manual check's "up to date" stays this long.
const NOTE_SECONDS: f64 = 5.0;

#[derive(Default)]
pub struct UpdateUi {
    /// The notes card is open.
    card: bool,
    /// The pill was closed for this session (the update still installs when ShapeCraft closes).
    hidden: bool,
    /// Asking whether to update before closing.
    pub asking: bool,
    /// The close was decided; let the next close through.
    pub closing: bool,
    /// The "Always update" box in the closing question.
    always: bool,
    /// When a manual check's result was first shown.
    shown_at: Option<f64>,
    /// A failed install from the last run has been reported.
    pub told_failure: bool,
}

/// What the user asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Install now: close (saving first), install, and start again when `restart`.
    Install {
        restart: bool,
    },
    /// Close without installing.
    CloseAnyway,
    Skip(String),
    /// Show the download page (only for copies that can't install updates themselves).
    OpenPage(String),
    /// Closing's question was answered "always".
    AlwaysOnClose,
}

pub fn prefs(s: &Settings) -> Prefs {
    Prefs { auto_check: s.update_check, auto_download: s.update_download, skipped: s.update_skip.clone() }
}

fn percent(done: u64, total: Option<u64>) -> String {
    match total.filter(|t| *t > 0) {
        Some(t) => format!("{}%", (done * 100 / t).min(100)),
        None => format!("{:.0} MB", done as f64 / 1e6),
    }
}

/// A rounded button; true when clicked.
fn button(ui: &mut Ui, r: Rect, label: &str, primary: bool, id: &str) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(("upd-btn", id)), Sense::click());
    let fill = match (primary, resp.hovered()) {
        (true, false) => theme::fixed(0x2f55d4),
        (true, true) => theme::fixed(0x3a62e8),
        (false, false) => Tokens::button_active(),
        (false, true) => Tokens::button_hover(),
    };
    ui.painter().rect_filled(r, radius(6), fill);
    let text = if primary { Color32::WHITE } else { Tokens::text_strong() };
    ui.painter().text(r.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(label), theme::body(12.0), text);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// The pill, its card and the closing question. `full` is the window.
pub fn show(ui: &mut Ui, full: Rect, state: &mut UpdateUi) -> Vec<Action> {
    let snap = Updater::global().snapshot();
    let now = ui.input(|i| i.time);
    let mut actions = if state.asking { closing_question(ui, full, state, &snap) } else { Vec::new() };
    let Some(line) = pill_line(&snap) else {
        state.card = false;
        state.shown_at = None;
        return actions;
    };
    // A manual check's "up to date" goes away by itself.
    if matches!(snap.phase, Phase::UpToDate | Phase::Checking) {
        let at = *state.shown_at.get_or_insert(now);
        if matches!(snap.phase, Phase::UpToDate) && now - at > NOTE_SECONDS {
            Updater::global().dismiss();
            state.shown_at = None;
            return actions;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    } else {
        state.shown_at = None;
    }
    if state.hidden && !snap.manual {
        return actions;
    }
    let pill_action = egui::Area::new(ui.scope_id().with("update-pill"))
        .order(egui::Order::Middle)
        .fixed_pos(full.min)
        .show(ui.ctx(), |ui| pill(ui, full, state, &snap, &line))
        .inner;
    actions.extend(pill_action);
    actions
}

/// What the pill says, and its button, if there's anything to say.
struct Line {
    text: String,
    button: Option<&'static str>,
    strong: bool,
    progress: Option<f32>,
}

fn pill_line(s: &Snapshot) -> Option<Line> {
    let version = s.offered.as_ref().map(|(v, _, _)| v.clone()).unwrap_or_default();
    let line = |text: String, button, strong| Some(Line { text, button, strong, progress: None });
    match &s.phase {
        Phase::Ready => line(format!("ShapeCraft {version} is ready"), Some("Restart"), true),
        Phase::Downloading { done, total } => Some(Line {
            text: format!("Downloading ShapeCraft {version} · {}", percent(*done, *total)),
            button: None,
            strong: false,
            progress: total.filter(|t| *t > 0).map(|t| *done as f32 / t as f32),
        }),
        Phase::Available => line(format!("ShapeCraft {version} is available"), Some("Download"), true),
        Phase::Elsewhere(_) => line(format!("ShapeCraft {version} is available"), Some("Details"), false),
        Phase::Checking if s.manual => line("Checking for updates…".into(), None, false),
        Phase::UpToDate if s.manual => line(format!("ShapeCraft {} is up to date", s.current), None, false),
        Phase::Failed(e) if s.manual => line(format!("Couldn't check for updates: {e}"), None, false),
        Phase::Failed(e) if s.offered.is_some() => line(e.clone(), Some("Retry"), false),
        _ => None,
    }
}

fn pill(ui: &mut Ui, full: Rect, state: &mut UpdateUi, snap: &Snapshot, line: &Line) -> Option<Action> {
    let p = ui.painter().clone();
    let font = theme::body(11.5);
    let text = p.layout_no_wrap(
        crate::i18n::tr_s(line.text.clone()),
        font.clone(),
        if line.strong { Tokens::text_strong() } else { Tokens::text_dim() },
    );
    let button_w = line.button.map_or(0.0, |b| p.layout_no_wrap(crate::i18n::tr_s(b), font.clone(), Color32::WHITE).size().x + 22.0);
    let w = (16.0 + 14.0 + text.size().x + 10.0 + button_w + if line.button.is_some() { 8.0 } else { 0.0 } + 22.0).min(full.width() - 32.0);
    let r = Rect::from_min_size(pos2(full.right() - 16.0 - w, full.bottom() - 16.0 - 32.0), vec2(w, 32.0));
    p.rect_filled(r.translate(vec2(0.0, 2.0)), radius(16), Color32::from_black_alpha(50));
    p.rect_filled(r, radius(16), Tokens::panel());
    let edge = if line.strong { Tokens::accent() } else { Tokens::panel_border() };
    p.rect_stroke(r, radius(16), Stroke::new(1.0, edge), StrokeKind::Inside);
    // A small mark: an arrow down while downloading, a dot once there's something to act on.
    let mark = pos2(r.left() + 18.0, r.center().y);
    if matches!(snap.phase, Phase::Downloading { .. } | Phase::Checking) {
        p.line_segment([mark + vec2(0.0, -5.0), mark + vec2(0.0, 4.0)], Stroke::new(1.5, Tokens::text_dim()));
        p.line_segment([mark + vec2(-3.5, 1.0), mark + vec2(0.0, 4.5)], Stroke::new(1.5, Tokens::text_dim()));
        p.line_segment([mark + vec2(3.5, 1.0), mark + vec2(0.0, 4.5)], Stroke::new(1.5, Tokens::text_dim()));
    } else {
        p.circle_filled(mark, 4.0, if line.strong { Tokens::accent() } else { Tokens::text_faint() });
    }
    let tx = r.left() + 30.0;
    p.galley(pos2(tx, r.center().y - text.size().y / 2.0), text, Tokens::text());
    if let Some(f) = line.progress {
        let bar = Rect::from_min_size(pos2(r.left() + 14.0, r.bottom() - 5.0), vec2((r.width() - 28.0) * f.clamp(0.0, 1.0), 2.0));
        p.rect_filled(bar, radius(1), Tokens::accent());
    }
    let mut action = None;
    // The close mark, then the button left of it.
    let x = Rect::from_center_size(pos2(r.right() - 15.0, r.center().y), vec2(16.0, 16.0));
    let xr = ui.interact(x, ui.scope_id().with("upd-hide"), Sense::click());
    let xc = if xr.hovered() { Tokens::text_strong() } else { Tokens::text_faint() };
    p.line_segment([x.center() + vec2(-3.5, -3.5), x.center() + vec2(3.5, 3.5)], Stroke::new(1.3, xc));
    p.line_segment([x.center() + vec2(3.5, -3.5), x.center() + vec2(-3.5, 3.5)], Stroke::new(1.3, xc));
    if xr.clicked() {
        if snap.manual {
            Updater::global().dismiss();
        } else {
            state.hidden = true;
        }
        state.card = false;
    }
    if let Some(label) = line.button {
        let b = Rect::from_min_size(pos2(r.right() - 26.0 - button_w, r.top() + 6.0), vec2(button_w, 20.0));
        if button(ui, b, label, line.strong, "pill") {
            match snap.phase {
                Phase::Ready => action = Some(Action::Install { restart: true }),
                Phase::Available => Updater::global().download(),
                Phase::Failed(_) => Updater::global().download(),
                _ => state.card = true,
            }
        }
    }
    // The text opens the card with the notes.
    let body = Rect::from_min_max(r.min, pos2(r.right() - 30.0 - button_w, r.max.y));
    let br = ui.interact(body, ui.scope_id().with("upd-body"), Sense::click());
    if snap.offered.is_some() && br.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if br.clicked() && snap.offered.is_some() {
        state.card = !state.card;
    }
    if state.card
        && let Some(a) = card(ui, full, r, state, snap)
    {
        action = Some(a);
    }
    action
}

/// Release notes as plain lines: headings and emphasis marks left out, links shown as their text.
fn plain_notes(md: &str, max_lines: usize) -> Vec<String> {
    let mut out = Vec::new();
    for raw in md.lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with("<!--") || t.starts_with("**Full Changelog**") {
            continue;
        }
        let mut s = t.trim_start_matches('#').trim().replace("**", "").replace('`', "");
        if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
            s = format!("•  {rest}");
        }
        // [text](url) → text
        while let (Some(a), Some(b)) = (s.find("]("), s.find('[')) {
            let Some(close) = s.get(a..).and_then(|r| r.find(')')).map(|c| a + c) else { break };
            if b > a {
                break;
            }
            let text = s.get(b + 1..a).unwrap_or("").to_string();
            s.replace_range(b..=close, &text);
        }
        out.push(s);
        if out.len() >= max_lines {
            break;
        }
    }
    out
}

fn card(ui: &mut Ui, full: Rect, pill: Rect, state: &mut UpdateUi, snap: &Snapshot) -> Option<Action> {
    let (version, notes, page) = snap.offered.clone()?;
    let w = 360.0_f32.min(full.width() - 32.0);
    let p = ui.painter().clone();
    let wrap = w - 32.0;
    let lines = plain_notes(&notes, 14);
    let galleys: Vec<_> = lines.iter().map(|l| p.layout(l.clone(), theme::body(11.5), Tokens::text(), wrap)).collect();
    let notes_h: f32 = galleys.iter().map(|g| g.size().y + 3.0).sum::<f32>().min(300.0);
    let footer = match &snap.phase {
        Phase::Ready => "It installs when you close ShapeCraft.".to_string(),
        Phase::Elsewhere(why) => why.clone(),
        Phase::Downloading { .. } => "Downloading in the background.".to_string(),
        _ => String::new(),
    };
    let foot = p.layout(crate::i18n::tr_s(footer), theme::body(10.5), Tokens::text_dim(), wrap);
    let h = 64.0 + notes_h + 12.0 + foot.size().y + 50.0;
    let r = Rect::from_min_size(pos2(pill.right() - w, pill.top() - 10.0 - h), vec2(w, h));
    // Clicks on the card stay on it.
    let _ = ui.interact(r, ui.scope_id().with("upd-card"), Sense::click());
    p.rect_filled(r.translate(vec2(0.0, 4.0)), radius(12), Color32::from_black_alpha(70));
    p.rect_filled(r, radius(12), Tokens::panel());
    p.rect_stroke(r, radius(12), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
    p.text(
        pos2(r.left() + 16.0, r.top() + 22.0),
        Align2::LEFT_CENTER,
        format!("ShapeCraft {version}"),
        theme::strong(14.0),
        Tokens::text_strong(),
    );
    p.text(
        pos2(r.left() + 16.0, r.top() + 42.0),
        Align2::LEFT_CENTER,
        format!("You have {}. What's new:", snap.current),
        theme::body(11.0),
        Tokens::text_dim(),
    );
    let mut y = r.top() + 60.0;
    let clip = Rect::from_min_max(pos2(r.left(), y), pos2(r.right(), y + notes_h));
    let cp = p.with_clip_rect(clip);
    for g in galleys {
        let gh = g.size().y;
        cp.galley(pos2(r.left() + 16.0, y), g, Tokens::text());
        y += gh + 3.0;
    }
    let y = clip.bottom() + 12.0;
    let fh = foot.size().y;
    p.galley(pos2(r.left() + 16.0, y), foot, Tokens::text_dim());
    let by = y + fh + 14.0;
    let mut action = None;
    let mut right = r.right() - 16.0;
    let mut place = |_label: &str, width: f32| {
        right -= width;
        let b = Rect::from_min_size(pos2(right, by), vec2(width, 24.0));
        right -= 8.0;
        b
    };
    match &snap.phase {
        Phase::Ready => {
            let b = place("Restart Now", 104.0);
            if button(ui, b, "Restart Now", true, "card-restart") {
                action = Some(Action::Install { restart: true });
            }
            let b = place("Later", 64.0);
            if button(ui, b, "Later", false, "card-later") {
                state.card = false;
            }
        }
        Phase::Available => {
            let b = place("Download", 96.0);
            if button(ui, b, "Download", true, "card-download") {
                Updater::global().download();
            }
        }
        Phase::Elsewhere(_) if matches!(snap.install, Install::Elsewhere(_) | Install::Dev) => {
            let b = place("Open Download Page", 150.0);
            if button(ui, b, "Open Download Page", false, "card-page") {
                action = Some(Action::OpenPage(page.clone()));
            }
        }
        _ => {}
    }
    if !matches!(snap.phase, Phase::Downloading { .. }) {
        let b = Rect::from_min_size(pos2(r.left() + 10.0, by), vec2(120.0, 24.0));
        let resp = ui.interact(b, ui.scope_id().with("upd-skip"), Sense::click());
        let c = if resp.hovered() { Tokens::text_strong() } else { Tokens::text_dim() };
        p.text(pos2(b.left() + 6.0, b.center().y), Align2::LEFT_CENTER, crate::i18n::tr("Skip This Version"), theme::body(11.5), c);
        if resp.clicked() {
            state.card = false;
            action = Some(Action::Skip(version));
        }
    }
    action
}

/// Asked when ShapeCraft closes with an update downloaded (Preferences can make it always update instead).
fn closing_question(ui: &mut Ui, full: Rect, state: &mut UpdateUi, snap: &Snapshot) -> Vec<Action> {
    let version = snap.offered.as_ref().map(|(v, _, _)| v.clone()).unwrap_or_default();
    egui::Area::new(ui.scope_id().with("update-closing"))
        .order(egui::Order::Foreground)
        .fixed_pos(full.min)
        .show(ui.ctx(), |ui| {
            let p = ui.painter().clone();
            let shade = ui.interact(full, ui.scope_id().with("upd-shade"), Sense::click());
            p.rect_filled(full, 0.0, Color32::from_black_alpha(110));
            let r = Rect::from_center_size(full.center(), vec2(452.0, 196.0));
            p.rect_filled(r.translate(vec2(0.0, 6.0)), radius(12), Color32::from_black_alpha(90));
            p.rect_filled(r, radius(12), Tokens::panel());
            p.rect_stroke(r, radius(12), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
            p.text(
                pos2(r.left() + 20.0, r.top() + 28.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr("Update ShapeCraft before closing?"),
                theme::strong(14.0),
                Tokens::text_strong(),
            );
            let body = p.layout(
                format!("ShapeCraft {version} is downloaded and ready. Installing takes a few seconds, and your work is saved first."),
                theme::body(12.0),
                Tokens::text(),
                r.width() - 40.0,
            );
            p.galley(pos2(r.left() + 20.0, r.top() + 50.0), body, Tokens::text());
            // "Always update" box.
            let at = pos2(r.left() + 20.0, r.top() + 112.0);
            let bx = Rect::from_min_size(pos2(at.x, at.y - 7.0), vec2(14.0, 14.0));
            let label = p.layout_no_wrap(crate::i18n::tr_s("Always update when closing"), theme::body(11.5), Tokens::text_dim());
            let hit = Rect::from_min_size(bx.min, vec2(22.0 + label.size().x, 14.0));
            if ui.interact(hit, ui.scope_id().with("upd-always"), Sense::click()).clicked() {
                state.always = !state.always;
            }
            p.rect_filled(bx, radius(3), if state.always { Tokens::accent() } else { Tokens::button_active() });
            if state.always {
                p.line_segment(
                    [bx.left_center() + vec2(3.0, 0.0), bx.center_bottom() + vec2(-1.0, -3.5)],
                    Stroke::new(1.8, Color32::WHITE),
                );
                p.line_segment([bx.center_bottom() + vec2(-1.0, -3.5), bx.right_top() + vec2(-3.0, 3.5)], Stroke::new(1.8, Color32::WHITE));
            }
            p.galley(pos2(at.x + 20.0, at.y - label.size().y / 2.0), label, Tokens::text_dim());
            let by = r.bottom() - 46.0;
            let update = Rect::from_min_size(pos2(r.right() - 20.0 - 132.0, by), vec2(132.0, 28.0));
            let restart = Rect::from_min_size(pos2(update.left() - 8.0 - 140.0, by), vec2(140.0, 28.0));
            let later = Rect::from_min_size(pos2(r.left() + 20.0, by), vec2(120.0, 28.0));
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let mut chosen = None;
            if button(ui, update, "Update and Close", true, "q-update") || enter {
                chosen = Some(Action::Install { restart: false });
            } else if button(ui, restart, "Update and Reopen", false, "q-restart") {
                chosen = Some(Action::Install { restart: true });
            } else if button(ui, later, "Not Now", false, "q-later") {
                chosen = Some(Action::CloseAnyway);
            } else if escape || (shade.clicked() && !r.contains(shade.interact_pointer_pos().unwrap_or(r.center()))) {
                // Changed their mind about closing.
                state.asking = false;
            }
            let mut out = Vec::new();
            if let Some(c) = chosen {
                state.asking = false;
                if state.always && c != Action::CloseAnyway {
                    out.push(Action::AlwaysOnClose);
                }
                out.push(c);
            }
            out
        })
        .inner
}

/// Preferences ▸ ShapeCraft ▸ Updates. Returns the height used.
pub fn prefs_section(ui: &mut Ui, col: f32, mut y: f32, s: &mut Settings, label: impl Fn(&egui::Painter, f32, &str)) -> f32 {
    let p = ui.painter().clone();
    let start = y;
    let snap = Updater::global().snapshot();
    label(&p, y, "Updates:");
    let status = match &snap.phase {
        Phase::Ready => format!("{} is ready: it installs when you close ShapeCraft", snap.offered.as_ref().map_or("", |o| o.0.as_str())),
        Phase::Downloading { done, total } => {
            format!("Downloading {} · {}", snap.offered.as_ref().map_or("", |o| o.0.as_str()), percent(*done, *total))
        }
        Phase::Checking => "Checking…".to_string(),
        Phase::UpToDate => "Up to date".to_string(),
        Phase::Available => format!("{} is available", snap.offered.as_ref().map_or("", |o| o.0.as_str())),
        Phase::Elsewhere(why) => why.clone(),
        Phase::Failed(e) => format!("Couldn't check: {e}"),
        Phase::Idle => match snap.checked_ago {
            Some(_) => "Up to date".to_string(),
            None => String::new(),
        },
    };
    p.text(
        pos2(col, y),
        Align2::LEFT_CENTER,
        format!("ShapeCraft {} ({})", snap.current, snap.install.label()),
        theme::body(12.5),
        Tokens::text_strong(),
    );
    let check_now = Rect::from_min_size(pos2(col + 250.0, y - 10.0), vec2(96.0, 20.0));
    if button(ui, check_now, "Check Now", false, "pref-check") {
        Updater::global().check_now();
    }
    y += 18.0;
    if !status.is_empty() {
        let g = p.layout(crate::i18n::tr_s(status), theme::body(10.0), Tokens::text_dim(), 340.0);
        let gh = g.size().y;
        p.galley(pos2(col, y - 6.0), g, Tokens::text_dim());
        y += gh;
    }
    y += 16.0;
    let row = |ui: &mut Ui, y: f32, id: &str, value: &mut bool, text: &str| {
        let g = p.layout_no_wrap(crate::i18n::tr_s(text), theme::body(12.5), Tokens::text_strong());
        let hit = Rect::from_min_size(pos2(col, y - 9.0), vec2(22.0 + g.size().x, 18.0));
        if ui.interact(hit, ui.scope_id().with(id), Sense::click()).clicked() {
            *value = !*value;
        }
        let bx = Rect::from_min_size(pos2(col, y - 7.0), vec2(14.0, 14.0));
        p.rect_filled(bx, radius(3), if *value { Tokens::accent() } else { theme::c(0x58585c) });
        if *value {
            p.line_segment([bx.left_center() + vec2(3.0, 0.0), bx.center_bottom() + vec2(-1.0, -3.5)], Stroke::new(1.8, Color32::WHITE));
            p.line_segment([bx.center_bottom() + vec2(-1.0, -3.5), bx.right_top() + vec2(-3.0, 3.5)], Stroke::new(1.8, Color32::WHITE));
        }
        p.galley(pos2(col + 20.0, y - g.size().y / 2.0), g, Tokens::text_strong());
    };
    row(ui, y, "pref-upd-check", &mut s.update_check, "Check for updates automatically");
    y += 26.0;
    row(ui, y, "pref-upd-download", &mut s.update_download, "Download updates in the background");
    y += 26.0;
    y - start
}
