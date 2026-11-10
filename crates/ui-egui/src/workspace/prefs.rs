//! The Help popover, the Preferences window and the Navigation Presets modal, laid out as observed.

use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use super::Workspace;
use super::drawings::{button, dialog};
use super::toolbar::UiAction;
use crate::graphics;
use crate::icons::{self, Icon};
use crate::settings::{Button, Mods, NavAction, PRESETS, Settings};
use crate::theme::{self, Tokens, radius};

/// The Variables dialog: named values with their expressions and what they come to.
fn variables(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(mut rows) = ws.variables_dialog.take() else { return };
    let h = 150.0 + rows.len() as f32 * 34.0;
    let r = dialog(ui, full, vec2(600.0, h), "Variables");
    let p = ui.painter().clone();
    let (cn, ck, cv, cr) = (r.left() + 18.0, r.left() + 166.0, r.left() + 244.0, r.right() - 70.0);
    let mut y = r.top() + 50.0;
    for (x, t) in [(cn, "Name"), (ck, "Type"), (cv, "Value"), (cr - 40.0, "=")] {
        p.text(pos2(x, y), Align2::LEFT_CENTER, t, theme::body(11.0), Tokens::text_dim());
    }
    y += 22.0;
    // What the rows come to, with every row's current text.
    let vars: Vec<shapecraft_doc::Variable> = rows
        .iter()
        .map(|(n, e, _, k)| shapecraft_doc::Variable { name: n.trim().to_string(), expr: e.trim().to_string(), kind: *k })
        .collect();
    let mut remove = None;
    for (i, (name, expr, _, kind)) in rows.iter_mut().enumerate() {
        let _ = ui.put(
            Rect::from_min_size(pos2(cn, y - 11.0), vec2(140.0, 22.0)),
            egui::TextEdit::singleline(name).hint_text("name").font(theme::body(12.5)),
        );
        // The type: a small pill that turns Length → Angle → Number.
        let pill = Rect::from_min_size(pos2(ck, y - 10.0), vec2(66.0, 20.0));
        let pr = ui.interact(pill, ui.scope_id().with(("var-kind", i)), Sense::click());
        p.rect_filled(pill, radius(5), if pr.hovered() { theme::c(0x4a4a4e) } else { theme::c(0x3a3a3c) });
        p.text(pill.center(), Align2::CENTER_CENTER, kind.label(), theme::body(11.5), Tokens::text_strong());
        if pr.clicked() {
            use shapecraft_doc::VarKind as K;
            *kind = match *kind {
                K::Length => K::Angle,
                K::Angle => K::Number,
                K::Number => K::Length,
            };
        }
        let _ = ui.put(
            Rect::from_min_size(pos2(cv, y - 11.0), vec2(cr - cv - 50.0, 22.0)),
            egui::TextEdit::singleline(expr).hint_text("10 mm").font(theme::body(12.5)),
        );
        let result = shapecraft_doc::expr::eval(expr, &vars);
        let (text, col) = match result {
            Ok(v) => (
                match kind {
                    shapecraft_doc::VarKind::Length => shapecraft_doc::format_length(v, ws.session.doc.units),
                    shapecraft_doc::VarKind::Angle => format!("{}°", (v * 1e4).round() / 1e4),
                    shapecraft_doc::VarKind::Number => format!("{}", (v * 1e6).round() / 1e6),
                },
                Tokens::text(),
            ),
            Err(_) if expr.trim().is_empty() => (String::new(), Tokens::text_dim()),
            Err(_) => ("?".to_string(), Tokens::danger()),
        };
        p.text(pos2(cr - 40.0, y), Align2::LEFT_CENTER, text, theme::body(12.0), col);
        let x = Rect::from_center_size(pos2(r.right() - 22.0, y), vec2(18.0, 18.0));
        let xr = ui.interact(x, ui.scope_id().with(("var-del", i)), Sense::click());
        icons::paint(&p, x.shrink(3.0), Icon::Close, if xr.hovered() { Tokens::text() } else { Tokens::text_dim() });
        if xr.clicked() {
            remove = Some(i);
        }
        y += 34.0;
    }
    if let Some(i) = remove {
        rows.remove(i);
    }
    let add = Rect::from_min_size(pos2(cn, y - 12.0), vec2(130.0, 24.0));
    let add_resp = ui.interact(add, ui.scope_id().with("var-add"), Sense::click());
    icons::paint(&p, Rect::from_center_size(pos2(add.left() + 8.0, add.center().y), vec2(12.0, 12.0)), Icon::Plus, Tokens::icon_blue());
    p.text(
        pos2(add.left() + 20.0, add.center().y),
        Align2::LEFT_CENTER,
        "Add Variable",
        theme::body(12.5),
        if add_resp.hovered() { Tokens::text_strong() } else { Tokens::icon_blue() },
    );
    if add_resp.clicked() {
        let mut n = rows.len() + 1;
        while rows.iter().any(|(name, ..)| *name == format!("var{n}")) {
            n += 1;
        }
        rows.push((format!("var{n}"), "10 mm".into(), None, shapecraft_doc::VarKind::Length));
    }
    let cancel =
        button(ui, Rect::from_min_size(pos2(r.right() - 166.0, r.bottom() - 36.0), vec2(70.0, 20.0)), "Cancel", false, "var-cancel");
    let done = button(ui, Rect::from_min_size(pos2(r.right() - 86.0, r.bottom() - 36.0), vec2(70.0, 20.0)), "Done", true, "var-done");
    if cancel || close_on_escape(ui) {
        return;
    }
    if done {
        // Deleted rows first, then changed and new ones; stop at the first refusal so it can be fixed.
        let keep: Vec<String> = rows.iter().filter_map(|(_, _, o, _)| o.clone()).collect();
        let gone: Vec<String> = ws.session.doc.variables.iter().map(|v| v.name.clone()).filter(|n| !keep.contains(n)).collect();
        let mut ok = true;
        for name in gone {
            ok &= ws.run("delete_variable", serde_json::json!({ "name": name })).is_some();
        }
        for (name, expr, orig, kind) in &rows {
            if !ok {
                break;
            }
            let (name, expr) = (name.trim(), expr.trim());
            let current =
                orig.as_ref().and_then(|o| ws.session.doc.variables.iter().find(|v| v.name == *o)).map(|v| (v.expr.clone(), v.kind));
            let k = kind.label().to_lowercase();
            match orig {
                Some(o) if o == name && current == Some((expr.to_string(), *kind)) => {}
                Some(o) => {
                    ok &= ws.run("set_variable", serde_json::json!({ "name": o, "value": expr, "rename": name, "kind": k })).is_some()
                }
                None => ok &= ws.run("set_variable", serde_json::json!({ "name": name, "value": expr, "kind": k })).is_some(),
            }
        }
        if ok {
            return;
        }
        // Keep the dialog open, now matching what was applied.
        rows = ws
            .session
            .doc
            .variables
            .iter()
            .map(|v| (v.name.clone(), v.expr.clone(), Some(v.name.clone()), v.kind))
            .chain(rows.into_iter().filter(|(n, ..)| !ws.session.doc.variables.iter().any(|v| v.name == n.trim())))
            .collect();
    }
    ws.variables_dialog = Some(rows);
}

pub(super) fn modals(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    if ws.variables_dialog.is_some() {
        variables(ws, ui, full);
    }
    if ws.prefs_open {
        preferences(ws, ui, full);
    }
    if ws.nav_presets.is_some() {
        nav_presets(ws, ui, full);
    }
}

fn close_on_escape(ui: &Ui) -> bool {
    ui.input(|i| i.key_pressed(egui::Key::Escape))
}

/// The ? popover under the title bar: What's New, the manual (shortcuts, navigation) and command search.
pub(super) fn help(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let w = 300.0;
    let r = Rect::from_min_size(pos2(full.right() - w - 8.0, full.top() + Tokens::TITLEBAR_H + 2.0), vec2(w, 318.0));
    let mut action = None;
    let shown = egui::Area::new(ui.scope_id().with("help-pop")).fixed_pos(r.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (r, _) = ui.allocate_exact_size(r.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(r.translate(vec2(0.0, 4.0)), radius(12), Color32::from_black_alpha(70));
        p.rect_filled(r, radius(11), Tokens::banner());
        p.rect_stroke(r, radius(11), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
        let left = r.left() + 14.0;
        let mut y = r.top() + 22.0;
        p.text(pos2(left, y), Align2::LEFT_CENTER, "Help", theme::strong(14.0), Tokens::text_strong());
        y += 26.0;
        p.text(pos2(left, y), Align2::LEFT_CENTER, "What's New?", theme::body(12.0), Tokens::text_dim());
        y += 22.0;
        // The newest features, as the home screen's What's new cards name them.
        for e in crate::whats_new::ENTRIES.iter().take(3) {
            let chip = Rect::from_min_size(pos2(left, y - 8.0), vec2(40.0, 16.0));
            p.rect_filled(chip, radius(8), theme::c(0x48484b));
            p.text(chip.center(), Align2::CENTER_CENTER, e.version, theme::body(10.0), Tokens::text());
            p.text(pos2(chip.right() + 10.0, y), Align2::LEFT_CENTER, e.title, theme::body(11.5), Tokens::text_strong());
            y += 21.0;
        }
        y += 6.0;
        p.line_segment([pos2(left, y), pos2(r.right() - 14.0, y)], Stroke::new(1.0, Tokens::chip_border()));
        y += 18.0;
        p.text(pos2(left, y), Align2::LEFT_CENTER, "Manual", theme::body(12.0), Tokens::text_dim());
        y += 14.0;
        let tile_w = (w - 28.0 - 10.0) / 2.0;
        for (k, (label, glyph, act)) in
            [("Keyboard\nShortcuts", "⌘", UiAction::Shortcuts), ("Navigation\nPresets", "", UiAction::NavPresets)].into_iter().enumerate()
        {
            let t = Rect::from_min_size(pos2(left + k as f32 * (tile_w + 10.0), y), vec2(tile_w, 70.0));
            let resp = ui.interact(t, ui.scope_id().with(("help-tile", k)), Sense::click());
            p.rect_filled(t, radius(8), if resp.hovered() { theme::c(0x48484b) } else { theme::c(0x3a3a3d) });
            if glyph.is_empty() {
                mouse_glyph(&p, pos2(t.center().x, t.top() + 20.0), 0.55, None, false);
            } else {
                p.text(
                    pos2(t.center().x, t.top() + 20.0),
                    Align2::CENTER_CENTER,
                    theme::key_text(glyph),
                    theme::body(20.0),
                    Tokens::text_strong(),
                );
            }
            p.text(pos2(t.center().x, t.top() + 49.0), Align2::CENTER_CENTER, label, theme::body(11.5), Tokens::text_strong());
            if resp.clicked() {
                action = Some(act);
            }
        }
        y += 82.0;
        let search = Rect::from_min_size(pos2(left, y), vec2(w - 28.0, 30.0));
        let resp = ui.interact(search, ui.scope_id().with("help-search"), Sense::click());
        p.rect_filled(search, radius(7), if resp.hovered() { theme::c(0x48484b) } else { theme::c(0x3a3a3d) });
        icons::paint(
            &p,
            Rect::from_center_size(pos2(search.left() + 16.0, search.center().y), vec2(13.0, 13.0)),
            Icon::Search,
            Tokens::text(),
        );
        p.text(
            pos2(search.left() + 32.0, search.center().y),
            Align2::LEFT_CENTER,
            "Search Commands",
            theme::body(12.0),
            Tokens::text_strong(),
        );
        if resp.clicked() {
            action = Some(UiAction::Search);
        }
        y += 46.0;
        let _ = y;
    });
    let outside = !ws.just_opened()
        && ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !shown.response.rect.contains(p)));
    if let Some(a) = action {
        ws.help_open = false;
        ws.ui_action(a);
    } else if outside || close_on_escape(ui) {
        ws.help_open = false;
    }
}

/// A label on the left, right-aligned at the column, as the Preferences window lays its rows out.
fn row_label(p: &egui::Painter, col: f32, y: f32, text: &str) {
    p.text(pos2(col - 8.0, y), Align2::RIGHT_CENTER, text, theme::body(12.5), Tokens::text_strong());
}

fn popup(ui: &mut Ui, at: Pos2, width: f32, id: &str, current: &mut String, options: &[&str]) {
    let names: Vec<String> = options.iter().map(|o| (*o).to_string()).collect();
    let now = names.iter().position(|n| n == current);
    if let Some(i) = super::popover::pill_select(ui, id, pos2(at.x + width, at.y), width, &names, now)
        && let Some(n) = names.get(i)
    {
        *current = n.clone();
    }
}

fn check(ui: &mut Ui, at: Pos2, id: &str, value: &mut bool, text: &str) {
    let p = ui.painter().clone();
    let g = p.layout_no_wrap(text.to_string(), theme::body(12.5), Tokens::text_strong());
    let hit = Rect::from_min_size(pos2(at.x, at.y - 9.0), vec2(22.0 + g.size().x, 18.0));
    let resp = ui.interact(hit, ui.scope_id().with(id), Sense::click());
    let bx = Rect::from_min_size(pos2(at.x, at.y - 7.0), vec2(14.0, 14.0));
    if *value {
        p.rect_filled(bx, radius(3), Tokens::accent());
        p.add(Shape::line(
            vec![bx.left_center() + vec2(3.0, 0.0), bx.center_bottom() + vec2(-1.0, -3.5), bx.right_top() + vec2(-3.0, 3.5)],
            Stroke::new(1.8, Color32::WHITE),
        ));
    } else {
        p.rect_filled(bx, radius(3), theme::c(0x58585c));
    }
    p.galley(pos2(at.x + 20.0, at.y - g.size().y / 2.0), g, Tokens::text_strong());
    if resp.clicked() {
        *value = !*value;
    }
}

fn small_button(ui: &mut Ui, at: Pos2, id: &str, text: &str) -> bool {
    let p = ui.painter().clone();
    let g = p.layout_no_wrap(text.to_string(), theme::body(12.0), Tokens::text_strong());
    let r = Rect::from_min_size(pos2(at.x, at.y - 10.0), vec2(g.size().x + 18.0, 20.0));
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
    p.rect_filled(r, radius(5), if resp.hovered() { theme::c(0x68686c) } else { theme::c(0x58585c) });
    p.galley(pos2(r.left() + 9.0, at.y - g.size().y / 2.0), g, Tokens::text_strong());
    resp.clicked()
}

/// Preferences (⌘,). General is as observed; ShapeCraft, Tablet, Plugins and Sync are ours (no Account tab: there are
/// no accounts). Changes apply at once and are saved by the app.
fn preferences(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let mut s = ws.settings.clone();
    let out = window(ui, full, &mut ws.prefs_tab, &mut s, true);
    if s != ws.settings {
        ws.settings = s;
    }
    if out.customize {
        ws.ui_action(UiAction::Shortcuts);
    }
    if out.presets {
        ws.ui_action(UiAction::NavPresets);
    }
    if out.done && ws.nav_presets.is_none() {
        ws.prefs_open = false;
    }
}

/// What a frame of the Preferences window asked for.
pub(crate) struct PrefsOut {
    pub done: bool,
    pub customize: bool,
    pub presets: bool,
}

/// The Preferences window itself, with no Workspace behind it, so the projects screen can show it too.
/// `shortcuts` is false where the Keyboard Shortcuts window can't open (projects screen).
pub(crate) fn window(ui: &mut Ui, full: Rect, tab: &mut usize, s: &mut Settings, shortcuts: bool) -> PrefsOut {
    let r = dialog(ui, full, vec2(560.0, 848.0), "Preferences");
    let p = ui.painter().clone();
    let tabs = [
        ("General", Icon::Tools, true),
        ("ShapeCraft", Icon::Gear, true),
        ("Tablet", Icon::Tablet, true),
        ("Plugins", Icon::Insert, true),
        ("Sync", Icon::Cloud, true),
    ];
    let middle = (tabs.len() as f32 - 1.0) / 2.0;
    for (k, (label, icon, enabled)) in tabs.into_iter().enumerate() {
        let c = pos2(r.center().x + (k as f32 - middle) * 58.0, r.top() + 44.0);
        let cell = Rect::from_center_size(c + vec2(0.0, 4.0), vec2(56.0, 36.0));
        let on = enabled && *tab == k;
        if enabled && ui.interact(cell, ui.scope_id().with(("pref-tab", k)), Sense::click()).clicked() {
            *tab = k;
        }
        if on {
            p.rect_filled(cell, radius(5), Tokens::button());
        }
        let col = if on {
            Tokens::icon_blue()
        } else if enabled {
            Tokens::text_dim()
        } else {
            Tokens::text_faint()
        };
        icons::paint(&p, Rect::from_center_size(c, vec2(15.0, 15.0)), icon, col);
        p.text(c + vec2(0.0, 15.0), Align2::CENTER_CENTER, label, theme::body(9.5), col);
    }
    p.line_segment([pos2(r.left(), r.top() + 70.0), pos2(r.right(), r.top() + 70.0)], Stroke::new(1.0, Tokens::chip_border()));
    let (mut customize, mut presets) = (false, false);
    match *tab {
        1 => shapecraft_tab(ui, r, s),
        2 => tablet_tab(ui, r, s),
        3 => plugins_tab(ui, r),
        4 => sync_tab(ui, r, s),
        _ => (customize, presets) = general_tab(ui, r, s, shortcuts),
    }
    let footer = Rect::from_min_max(pos2(r.left(), r.bottom() - 52.0), r.max);
    p.rect_filled(footer, egui::CornerRadius { nw: 0, ne: 0, sw: 11, se: 11 }, theme::c(0x232325));
    let done = button(ui, Rect::from_min_size(pos2(r.right() - 86.0, r.bottom() - 36.0), vec2(70.0, 20.0)), "Done", true, "pref-done");
    PrefsOut { done: done || close_on_escape(ui), customize, presets }
}

/// A radio row: the name, and a line of detail under it. Disabled rows are shown but can't be picked.
fn radio(ui: &mut Ui, at: Pos2, id: &str, on: bool, enabled: bool, name: &str, detail: &str) -> bool {
    let p = ui.painter().clone();
    let hit = Rect::from_min_size(pos2(at.x, at.y - 10.0), vec2(320.0, 34.0));
    let resp = ui.interact(hit, ui.scope_id().with(id), if enabled { Sense::click() } else { Sense::hover() });
    let dot = pos2(at.x + 7.0, at.y);
    let (fill, text) = if enabled { (theme::c(0x58585c), Tokens::text_strong()) } else { (theme::c(0x3a3a3c), Tokens::text_faint()) };
    p.circle_filled(dot, 7.0, if on { Tokens::accent() } else { fill });
    if on {
        p.circle_filled(dot, 2.5, Color32::WHITE);
    }
    p.text(pos2(at.x + 22.0, at.y), Align2::LEFT_CENTER, name, theme::body(12.5), text);
    let dim = if enabled { Tokens::text_dim() } else { Tokens::text_faint() };
    p.text(pos2(at.x + 22.0, at.y + 15.0), Align2::LEFT_CENTER, detail, theme::body(10.0), dim);
    enabled && resp.clicked()
}

/// What each graphics choice would use here, and whether it can be picked.
fn graphics_row(status: &graphics::Status, choice: &str, gpu: &str) -> (bool, String) {
    let mut on_path = status.found.iter().filter(|f| f.path == choice);
    let found = on_path.clone().find(|f| f.name == gpu).or_else(|| on_path.next());
    let warning = match choice {
        graphics::CPU => " · slower, works almost anywhere",
        graphics::OPENGL => " · older GPUs, some effects may be off",
        _ => "",
    };
    match (choice, found) {
        (graphics::AUTOMATIC, _) => (true, "Recommended: the best path this computer has".to_string()),
        (_, None) if status.unchecked(choice) => (true, format!("Not checked in this run{warning}")),
        (_, Some(f)) => (true, format!("{}{warning}", f.name)),
        (_, None) => (false, "Not available on this computer".to_string()),
    }
}

/// Everything ShapeCraft adds that the reference doesn't have.
fn shapecraft_tab(ui: &mut Ui, r: Rect, s: &mut Settings) {
    let p = ui.painter().clone();
    let col = r.left() + 170.0;
    let sep = |y: f32| p.line_segment([pos2(r.left() + 20.0, y), pos2(r.right() - 20.0, y)], Stroke::new(1.0, Tokens::chip_border()));
    let mut y = r.top() + 92.0;
    p.text(pos2(r.center().x, y), Align2::CENTER_CENTER, "Options you'll only find in ShapeCraft.", theme::body(11.0), Tokens::text_dim());
    y += 22.0;
    sep(y);
    y += 24.0;
    row_label(&p, col, y, "Graphics:");
    let status = graphics::status();
    for choice in graphics::choices() {
        let (enabled, detail) = graphics_row(&status, choice, &s.gpu);
        if radio(ui, pos2(col, y), &format!("pref-gfx-{choice}"), s.graphics == *choice, enabled, choice, &detail) {
            s.graphics = (*choice).to_string();
        }
        y += 38.0;
    }
    // Only worth a row on a machine with more than one.
    let gpus = status.gpus();
    if gpus.len() > 1 {
        y += 6.0;
        row_label(&p, col, y, "GPU:");
        let names: Vec<&str> = std::iter::once(graphics::BEST_GPU).chain(gpus.iter().map(String::as_str)).collect();
        popup(ui, pos2(col, y), 280.0, "pref-gpu", &mut s.gpu, &names);
        y += 30.0;
    }
    if let Some(a) = &status.active {
        p.text(pos2(col, y - 4.0), Align2::LEFT_CENTER, format!("In use: {} ({})", a.path, a.name), theme::body(10.0), Tokens::text_dim());
        y += 16.0;
    }
    if !status.choice.is_empty() && (s.graphics != status.choice || s.gpu != status.gpu) {
        p.text(pos2(col, y - 4.0), Align2::LEFT_CENTER, "Restart ShapeCraft to apply.", theme::body(10.0), Tokens::text_strong());
        y += 16.0;
    }
    y += 6.0;
    sep(y);
    y += 24.0;
    row_label(&p, col, y, "Orbit:");
    check(ui, pos2(col, y), "pref-orbit-x", &mut s.orbit_invert_x, "Invert X");
    check(ui, pos2(col + 110.0, y), "pref-orbit-y", &mut s.orbit_invert_y, "Invert Y");
    y += 30.0;
    row_label(&p, col, y, "Pan:");
    check(ui, pos2(col, y), "pref-pan-x", &mut s.pan_invert_x, "Invert X");
    check(ui, pos2(col + 110.0, y), "pref-pan-y", &mut s.pan_invert_y, "Invert Y");
    y += 24.0;
    sep(y);
    y += 24.0;
    // The classic left-column ✓ / ✕ for those used to earlier versions of the reference.
    row_label(&p, col, y, "Done / Cancel Buttons:");
    popup(ui, pos2(col, y), 140.0, "pref-confirm", &mut s.confirm_buttons, &["Bottom of View", "Toolbar", "Both"]);
    y += 26.0;
    sep(y);
    y += 24.0;
    y += crate::updates::prefs_section(ui, col, y, s, |p, y, t| row_label(p, col, y, t));
    row_label(&p, col, y, "When Closing:");
    popup(ui, pos2(col, y), 140.0, "pref-upd-close", &mut s.update_on_close, &crate::updates::ON_CLOSE);
    p.text(pos2(col + 150.0, y), Align2::LEFT_CENTER, "with an update downloaded", theme::body(10.5), Tokens::text_dim());
}

/// Tablet access is early: said plainly so nobody takes it for finished.
const IN_TESTING: &str = "In testing: an early look, not finished. Rough edges are expected.";

/// Serving ShapeCraft to a browser on the network (see `crate::tablet`): the switch, where to point the tablet (with a
/// QR code for its camera), and who's connected.
fn tablet_tab(ui: &mut Ui, r: Rect, s: &mut Settings) {
    let p = ui.painter().clone();
    let col = r.left() + 170.0;
    let sep = |y: f32| p.line_segment([pos2(r.left() + 20.0, y), pos2(r.right() - 20.0, y)], Stroke::new(1.0, Tokens::chip_border()));
    let mut y = r.top() + 92.0;
    p.text(
        pos2(r.center().x, y),
        Align2::CENTER_CENTER,
        "Use ShapeCraft on an iPad, iPhone or another computer, in its browser.",
        theme::body(11.0),
        Tokens::text_dim(),
    );
    y += 18.0;
    p.text(pos2(r.center().x, y), Align2::CENTER_CENTER, IN_TESTING, theme::body(11.0), Tokens::accent());
    y += 22.0;
    sep(y);
    y += 24.0;
    if crate::tablet::on_tablet() {
        p.text(
            pos2(r.center().x, y + 10.0),
            Align2::CENTER_CENTER,
            "You're using ShapeCraft from a browser.",
            theme::body(12.5),
            Tokens::text_strong(),
        );
        p.text(
            pos2(r.center().x, y + 30.0),
            Align2::CENTER_CENTER,
            "Turn tablet access on or off on the computer that runs it.",
            theme::body(11.0),
            Tokens::text_dim(),
        );
        p.text(pos2(r.center().x, y + 50.0), Align2::CENTER_CENTER, IN_TESTING, theme::body(11.0), Tokens::accent());
        return;
    }
    row_label(&p, col, y, "Tablet Access:");
    check(ui, pos2(col, y), "pref-tablet-on", &mut s.tablet_serve, "Serve ShapeCraft to browsers");
    y += 26.0;
    row_label(&p, col, y, "Reachable From:");
    let mut reach = if s.tablet_network { "This Network" } else { "This Computer Only" }.to_string();
    popup(ui, pos2(col, y), 170.0, "pref-tablet-net", &mut reach, &["This Network", "This Computer Only"]);
    s.tablet_network = reach == "This Network";
    y += 28.0;
    row_label(&p, col, y, "Port:");
    let ports = ["8780", "8781", "8782", "8790", "8080"];
    let mut port = s.tablet_port.to_string();
    let mut names: Vec<&str> = ports.to_vec();
    if !ports.contains(&port.as_str()) {
        names.insert(0, port.as_str());
    }
    let names: Vec<String> = names.iter().map(|n| (*n).to_string()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    popup(ui, pos2(col, y), 90.0, "pref-tablet-port", &mut port, &refs);
    if let Ok(n) = port.parse() {
        s.tablet_port = n;
    }
    y += 22.0;
    sep(y);
    y += 22.0;
    let status = crate::tablet::status();
    let Some(st) = status.filter(|_| s.tablet_serve) else {
        let lines = [
            "Turn it on, then open the address it shows in Safari on your iPad,",
            "or scan the code with its camera. Add it to the Home Screen to use",
            "it full screen, like an app. Your projects stay on this computer.",
        ];
        for l in lines {
            p.text(pos2(col - 120.0, y), Align2::LEFT_CENTER, l, theme::body(11.0), Tokens::text_dim());
            y += 16.0;
        }
        return;
    };
    if let Some(e) = &st.error {
        p.text(pos2(col - 120.0, y), Align2::LEFT_CENTER, e, theme::body(11.0), Tokens::text_strong());
        y += 20.0;
    }
    let Some(url) = st.urls.first().cloned() else {
        p.text(pos2(col - 120.0, y), Align2::LEFT_CENTER, "Starting…", theme::body(11.0), Tokens::text_dim());
        return;
    };
    // The code a tablet's camera opens, the first address under it, the others below.
    let qr_side = 176.0;
    let qr = Rect::from_min_size(pos2(r.center().x - qr_side / 2.0, y), vec2(qr_side, qr_side));
    qr_code(&p, qr, &url);
    y = qr.bottom() + 18.0;
    for (i, u) in st.urls.iter().enumerate() {
        let font = if i == 0 { theme::strong(12.0) } else { theme::body(11.5) };
        let resp = ui.interact(
            Rect::from_center_size(pos2(r.center().x, y), vec2(r.width() - 40.0, 16.0)),
            ui.scope_id().with(("tablet-url", i)),
            Sense::click(),
        );
        if resp.clicked() {
            ui.ctx().copy_text(u.clone());
        }
        p.text(pos2(r.center().x, y), Align2::CENTER_CENTER, u, font, if i == 0 { Tokens::text_strong() } else { Tokens::text_dim() });
        y += 18.0;
    }
    p.text(pos2(r.center().x, y), Align2::CENTER_CENTER, "Click an address to copy it.", theme::body(10.0), Tokens::text_faint());
    y += 24.0;
    let who = if st.clients.is_empty() { "No browser connected".to_string() } else { format!("Connected: {}", st.clients.join(", ")) };
    p.text(pos2(r.center().x, y), Align2::CENTER_CENTER, who, theme::body(11.5), Tokens::text_strong());
    y += 26.0;
    if small_button(ui, pos2(r.center().x - 60.0, y), "pref-tablet-key", "New Pairing Key") {
        // Devices paired with the old key have to scan again; the host restarts with the new one.
        s.tablet_key = crate::tablet::new_token();
    }
}

/// A QR code for `text`, black on white with its quiet zone, filling `r`.
fn qr_code(p: &egui::Painter, r: Rect, text: &str) {
    let Ok(code) = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M) else { return };
    let n = code.width();
    let colors = code.to_colors();
    p.rect_filled(r, radius(8), Color32::WHITE);
    let quiet = 3.0;
    let cell = r.width() / (n as f32 + 2.0 * quiet);
    let o = r.min + vec2(quiet * cell, quiet * cell);
    for (i, c) in colors.iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            // A hair over a cell, so neighbours join without seams.
            p.rect_filled(Rect::from_min_size(o + vec2(x * cell, y * cell), vec2(cell + 0.35, cell + 0.35)), 0.0, Color32::BLACK);
        }
    }
}

/// Waiting for the plugin system.
fn plugins_tab(ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    let c = pos2(r.center().x, r.top() + 150.0);
    p.text(c, Align2::CENTER_CENTER, "No plugins yet", theme::body(14.0), Tokens::text_strong());
    p.text(
        c + vec2(0.0, 22.0),
        Align2::CENTER_CENTER,
        "Plugin support is planned. Your plugins will be listed here.",
        theme::body(11.0),
        Tokens::text_dim(),
    );
}

/// Preferences ▸ Sync: the cloud storage apps on this computer (see `crate::cloud`). ShapeCraft saves into their
/// folders and they do the syncing; this shows which are set up, which locations the projects screen lists, any
/// folders added by hand, and where new projects go.
fn sync_tab(ui: &mut Ui, r: Rect, s: &mut Settings) {
    use crate::backup::{self, Target};
    use crate::cloud::{self, State};
    let p = ui.painter().clone();
    let sep = |y: f32| p.line_segment([pos2(r.left() + 20.0, y), pos2(r.right() - 20.0, y)], Stroke::new(1.0, Tokens::chip_border()));
    let heading =
        |y: f32, text: &str| p.text(pos2(r.left() + 28.0, y), Align2::LEFT_CENTER, text, theme::strong(11.0), Tokens::text_faint());
    // One line of small grey text, cut short with … when it doesn't fit.
    let detail = |ui: &mut Ui, at: Pos2, text: String, width: f32, color: Color32| {
        let mut job = egui::text::LayoutJob::simple_singleline(text, theme::body(10.5), color);
        job.wrap = egui::text::TextWrapping { max_width: width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
        let g = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(at, g, color);
    };
    let detector = cloud::detector();
    let found = detector.found();
    if found.is_none() && !detector.busy() {
        let ctx = ui.ctx().clone();
        detector.refresh(move || ctx.request_repaint());
    }
    let mut y = r.top() + 92.0;
    for line in [
        "Projects save to your Work Folder as you go. Backups keep a copy",
        "in your cloud storage, a NAS or another drive, kept up to date.",
    ] {
        p.text(pos2(r.center().x, y), Align2::CENTER_CENTER, line, theme::body(11.0), Tokens::text_dim());
        y += 16.0;
    }
    y += 6.0;
    sep(y);
    y += 24.0;
    if crate::tablet::on_tablet() {
        p.text(
            pos2(r.center().x, y + 10.0),
            Align2::CENTER_CENTER,
            "Set up the Work Folder and backups on the computer that runs ShapeCraft.",
            theme::body(12.0),
            Tokens::text_strong(),
        );
        return;
    }
    let left = r.left() + 40.0;
    let right = r.right() - 30.0;
    let found = found.unwrap_or_default();

    // The Work Folder: where autosave writes. Usually on this computer; a network folder works, more slowly.
    heading(y, "WORK FOLDER");
    y += 24.0;
    let work = backup::work().unwrap_or_else(|| {
        if s.work_folder.is_empty() { crate::app::Options::default_root() } else { std::path::PathBuf::from(&s.work_folder) }
    });
    icons::paint(&p, Rect::from_center_size(pos2(left - 12.0, y), vec2(15.0, 15.0)), Icon::Folder, Tokens::icon_blue());
    let name = work.file_name().map_or_else(|| work.display().to_string(), |n| n.to_string_lossy().into_owned());
    p.text(pos2(left + 4.0, y), Align2::LEFT_CENTER, name, theme::strong(12.5), Tokens::text_strong());
    detail(ui, pos2(left + 4.0, y + 9.0), crate::landing::shorten_home(&work), right - left - 150.0, Tokens::text_dim());
    let pending_id = ui.scope_id().with("sync-work-pending");
    let mut pending: Option<std::path::PathBuf> = ui.data(|d| d.get_temp(pending_id)).flatten();
    if small_button(ui, pos2(right - 136.0, y + 2.0), "sync-work-change", "Change…")
        && let Some(dir) = crate::dialogs::FileDialog::new().set_title("Choose your Work Folder").set_directory(&work).pick_folder()
        && dir != work
    {
        pending = Some(dir);
    }
    if small_button(ui, pos2(right - 62.0, y + 2.0), "sync-work-show", "Show") {
        crate::landing::reveal(&work);
    }
    y += 34.0;
    // Where it is says something worth knowing: synced by a cloud app already, or across the network.
    let in_cloud = found.ready().find(|c| work.starts_with(&c.root)).map(|c| c.provider);
    let documents = found.documents_synced_by.filter(|_| dirs::document_dir().is_some_and(|d| work.starts_with(d)));
    let note = if let Some(by) = in_cloud {
        Some(format!(
            "In {}: every computer that uses this folder has the same projects and Recents. You're warned before opening a project \
             that's open on another computer.",
            by.name()
        ))
    } else if let Some(by) = documents {
        Some(format!("{} syncs this folder too. Projects open fastest when it keeps them downloaded on this computer.", by.name()))
    } else if on_network(&work) {
        Some("A network folder: saving waits on the network, and projects can't open while it's away.".to_string())
    } else {
        None
    };
    if let Some(n) = note {
        let g = p.layout(n, theme::body(10.5), Tokens::text_dim(), right - left);
        let h = g.size().y;
        p.galley(pos2(left + 4.0, y - 8.0), g, Tokens::text_dim());
        y += h + 6.0;
    }
    if let Some(to) = pending.clone() {
        let ask = format!("Bring your projects to {}?", crate::landing::shorten_home(&to));
        detail(ui, pos2(left + 4.0, y - 6.0), ask, right - left - 250.0, Tokens::accent());
        if small_button(ui, pos2(right - 250.0, y + 2.0), "sync-work-move", "Move Them") {
            backup::request_switch(to.clone(), true);
            pending = None;
        } else if small_button(ui, pos2(right - 160.0, y + 2.0), "sync-work-leave", "Leave Them") {
            backup::request_switch(to.clone(), false);
            pending = None;
        } else if small_button(ui, pos2(right - 66.0, y + 2.0), "sync-work-cancel", "Cancel") {
            pending = None;
        }
        y += 30.0;
    } else {
        let note = backup::note();
        if !note.is_empty() {
            detail(ui, pos2(left + 4.0, y - 8.0), note, right - left, Tokens::accent());
            y += 18.0;
        }
    }
    ui.data_mut(|d| d.insert_temp(pending_id, pending));
    y += 4.0;
    sep(y);
    y += 24.0;

    // The backups, first first: the first is brought up to date first, and is where Restore looks first.
    heading(y, "BACKUPS");
    y += 24.0;
    let jobs = backup::backups();
    let mut action: Option<(usize, i32)> = None;
    if s.backups.is_empty() {
        detail(ui, pos2(left + 4.0, y - 6.0), "None yet. Add one below.".into(), right - left, Tokens::text_dim());
        y += 26.0;
    }
    for (k, b) in s.backups.iter().enumerate() {
        let t = Target::parse(b);
        let root = t.root(&found);
        p.text(pos2(left - 26.0, y), Align2::CENTER_CENTER, format!("{}", k + 1), theme::strong(11.0), Tokens::text_faint());
        let icon = if t.is_cloud() { Icon::Cloud } else { Icon::Folder };
        icons::paint(&p, Rect::from_center_size(pos2(left - 6.0, y), vec2(15.0, 15.0)), icon, Tokens::icon_blue());
        p.text(pos2(left + 8.0, y), Align2::LEFT_CENTER, t.label(&found), theme::strong(12.5), Tokens::text_strong());
        let st = jobs.status(b);
        let (line, col) = match (&root, &st) {
            (None, _) if detector.busy() => ("Looking for it…".to_string(), Tokens::text_dim()),
            (None, _) => ("Its app isn't signed in on this computer.".to_string(), Tokens::danger()),
            (_, backup::Status { busy: Some((d, n)), .. }) if *n > 0 => (format!("Backing up… {d} of {n}"), Tokens::accent()),
            (_, backup::Status { busy: Some(_), .. }) => ("Backing up…".to_string(), Tokens::accent()),
            (_, backup::Status { problem: Some(e), .. }) => (e.clone(), Tokens::danger()),
            (Some(_), backup::Status { done: Some(t), .. }) => (format!("Up to date · backed up {}", backup::ago(*t)), Tokens::text_dim()),
            (Some(root), _) => (crate::landing::shorten_home(&backup::dest(root, s.backup_per_computer)), Tokens::text_dim()),
        };
        detail(ui, pos2(left + 8.0, y + 9.0), line, right - left - 200.0, col);
        if k > 0 && small_button(ui, pos2(right - 188.0, y + 2.0), &format!("sync-up-{k}"), "Up") {
            action = Some((k, -1));
        }
        if k + 1 < s.backups.len() && small_button(ui, pos2(right - 142.0, y + 2.0), &format!("sync-down-{k}"), "Down") {
            action = Some((k, 1));
        }
        if small_button(ui, pos2(right - 68.0, y + 2.0), &format!("sync-remove-{k}"), "Remove") {
            action = Some((k, 0));
        }
        y += 44.0;
    }
    match action {
        Some((k, 0)) => {
            s.backups.remove(k);
        }
        Some((k, d)) => s.backups.swap(k, (k as i32 + d) as usize),
        None => {}
    }
    if jobs.busy() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    // Add: each signed-in cloud not in the list yet, or any folder (a NAS share, another drive).
    let adding_id = ui.scope_id().with("sync-adding");
    let mut adding: bool = ui.data(|d| d.get_temp(adding_id)).unwrap_or(false);
    if small_button(ui, pos2(left - 20.0, y - 4.0), "sync-add", if adding { "Done Adding" } else { "Add Backup…" }) {
        adding = !adding;
    }
    if !s.backups.is_empty() && small_button(ui, pos2(right - 104.0, y - 4.0), "sync-now", "Back Up Now") {
        jobs.now();
    }
    y += 26.0;
    if adding {
        let mut x = left - 20.0;
        let free: Vec<(String, String)> = found.ready().filter(|c| !s.backups.contains(&c.key())).map(|c| (c.key(), c.label())).collect();
        for (key, label) in free {
            let text = format!("+ {label}");
            let w = p.layout_no_wrap(text.clone(), theme::body(12.0), Tokens::text_strong()).size().x + 26.0;
            if x + w > right {
                x = left - 20.0;
                y += 26.0;
            }
            if small_button(ui, pos2(x, y), &format!("sync-add-{key}"), &text) {
                s.backups.push(key);
            }
            x += w;
        }
        if x + 150.0 > right {
            x = left - 20.0;
            y += 26.0;
        }
        if small_button(ui, pos2(x, y), "sync-add-folder", "+ Folder or NAS…")
            && let Some(dir) = crate::dialogs::FileDialog::new().set_title("Choose a folder to back up to").pick_folder()
        {
            let setting = Target::Folder(dir.clone()).setting();
            if dir.starts_with(&work) || work.starts_with(&dir) {
                backup::set_note("A backup can't be inside the Work Folder (or hold it).".into());
            } else if !s.backups.contains(&setting) {
                s.backups.push(setting);
            }
        }
        y += 28.0;
    }
    ui.data_mut(|d| d.insert_temp(adding_id, adding));
    // When, how far back, and whose: how the backups are made.
    let col = r.left() + 210.0;
    row_label(&p, col, y, "Back Up:");
    let whens = ["When I Save, Close or Quit", "After Every Change"];
    let mut when = whens[s.backup_after_change as usize].to_string();
    popup(ui, pos2(col, y), 220.0, "pref-sync-when", &mut when, &whens);
    s.backup_after_change = when == whens[1];
    y += 28.0;
    row_label(&p, col, y, "Keep Older Versions:");
    let keeps = ["Off", "5", "10", "25", "50"];
    let mut keep = if s.backup_versions == 0 { "Off".to_string() } else { s.backup_versions.to_string() };
    if !keeps.contains(&keep.as_str()) {
        keep = "10".into();
    }
    popup(ui, pos2(col, y), 80.0, "pref-sync-keep", &mut keep, &keeps);
    s.backup_versions = keep.parse().unwrap_or(0);
    y += 28.0;
    check(ui, pos2(col, y), "pref-sync-per-pc", &mut s.backup_per_computer, "A Folder for Each Computer");
    y += 16.0;
    let why = if s.backup_per_computer {
        format!("This computer backs up into ShapeCraft Backups/{}.", backup::computer_name())
    } else {
        "Computers sharing a Work Folder share one backup too.".to_string()
    };
    detail(ui, pos2(col + 22.0, y - 4.0), why, right - col - 22.0, Tokens::text_dim());
    y += 18.0;
    sep(y);
    y += 24.0;

    // What's on this computer: each cloud storage app and whether it can take a backup.
    heading(y, "CLOUD STORAGE ON THIS COMPUTER");
    y += 22.0;
    if found.clouds.is_empty() {
        p.text(pos2(left + 4.0, y), Align2::LEFT_CENTER, "Looking…", theme::body(12.0), Tokens::text_dim());
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        return;
    }
    let mut refresh = false;
    for (k, c) in found.clouds.iter().enumerate() {
        let ready = c.state == State::Ready;
        icons::paint(
            &p,
            Rect::from_center_size(pos2(left - 12.0, y), vec2(15.0, 15.0)),
            Icon::Cloud,
            if ready { Tokens::icon_blue() } else { Tokens::text_faint() },
        );
        p.text(
            pos2(left + 4.0, y),
            Align2::LEFT_CENTER,
            c.label(),
            theme::strong(12.0),
            if ready { Tokens::text_strong() } else { Tokens::text() },
        );
        let text = match c.state {
            State::Ready => crate::landing::shorten_home(&c.root),
            State::NotSignedIn if c.provider == cloud::Provider::ICloud && cfg!(target_os = "macos") => {
                "Turn on iCloud Drive in System Settings ▸ Apple Account ▸ iCloud.".to_string()
            }
            State::NotSignedIn => format!("Installed. Open {} and sign in.", c.provider.name()),
            State::NotInstalled => "Not on this computer.".to_string(),
            State::Unsupported => "No official app for Linux. Its folder from rclone or Insync can be added as a folder.".to_string(),
        };
        detail(ui, pos2(left + 4.0, y + 8.0), text, right - left - 250.0, Tokens::text_dim());
        let id = format!("sync-cloud-{k}");
        match c.state {
            State::Ready if work.starts_with(&c.root) => tag(&p, right, y, "Work Folder"),
            State::Ready => {
                if s.backups.contains(&c.key()) {
                    tag(&p, right, y, "Backing Up");
                } else if small_button(ui, pos2(right - 92.0, y + 2.0), &id, "Back Up Here") {
                    s.backups.push(c.key());
                }
                // Same projects on every computer: the Work Folder in this app's folder.
                if small_button(ui, pos2(right - 186.0, y + 2.0), &format!("{id}-work"), "Work Here") {
                    s.backups.retain(|b| *b != c.key());
                    let to = c.root.join("ShapeCraft");
                    ui.data_mut(|d| d.insert_temp(pending_id, Some(to)));
                }
            }
            State::NotInstalled => {
                let label = format!("Get {}", c.provider.name());
                let w = p.layout_no_wrap(label.clone(), theme::body(12.0), Tokens::text_strong()).size().x + 18.0;
                if small_button(ui, pos2(right - w, y + 2.0), &id, &label) {
                    crate::landing::open_link(c.provider.download_url());
                }
            }
            State::NotSignedIn => {
                if small_button(ui, pos2(right - 84.0, y + 2.0), &id, "Check Again") {
                    refresh = true;
                }
            }
            // A tag, not a button: there's nothing to get for this system.
            State::Unsupported => tag(&p, right, y, "Not Supported"),
        }
        y += 34.0;
    }
    if small_button(ui, pos2(right - 84.0, y - 2.0), "sync-refresh", "Check Again") {
        refresh = true;
    }
    if refresh && !detector.busy() {
        let ctx = ui.ctx().clone();
        detector.refresh(move || ctx.request_repaint());
    }
}

/// A small outlined label at the right of a row ("Not Supported", "Backing Up").
fn tag(p: &egui::Painter, right: f32, y: f32, text: &str) {
    let g = p.layout_no_wrap(text.into(), theme::body(11.0), Tokens::text_dim());
    let r = Rect::from_min_size(pos2(right - g.size().x - 16.0, y - 7.0), vec2(g.size().x + 16.0, 20.0));
    p.rect_stroke(r, 10.0, Stroke::new(1.0, Tokens::text_dim().gamma_multiply(0.5)), egui::StrokeKind::Inside);
    p.galley(pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, Tokens::text_dim());
}

/// Whether a folder is on another computer (a mounted share, a \\server\share path, an SMB/NFS mount).
fn on_network(dir: &std::path::Path) -> bool {
    let s = dir.to_string_lossy();
    if s.starts_with(r"\\") || s.starts_with("//") {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        // Mounted shares sit in /Volumes; local drives there say so in their mount table entry.
        if let Ok(out) = std::process::Command::new("/sbin/mount").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            return text.lines().any(|l| {
                let Some((_, rest)) = l.split_once(" on ") else { return false };
                let Some((point, kind)) = rest.split_once(" (") else { return false };
                point != "/" && dir.starts_with(point) && ["smbfs", "nfs", "afpfs", "webdav"].iter().any(|k| kind.starts_with(k))
            });
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(text) = std::fs::read_to_string("/proc/mounts") {
        return text.lines().any(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.len() > 2 && f[1] != "/" && dir.starts_with(f[1]) && ["cifs", "smb3", "nfs", "nfs4", "sshfs", "fuse.sshfs"].contains(&f[2])
        });
    }
    false
}

/// The General tab, as observed. Returns whether Keyboard Shortcuts or Navigation Presets was pressed.
fn general_tab(ui: &mut Ui, r: Rect, s: &mut Settings, shortcuts: bool) -> (bool, bool) {
    let p = ui.painter().clone();
    let col = r.left() + 216.0;
    let mut y = r.top() + 92.0;
    let pitch = 30.0;
    let sep = |p: &egui::Painter, y: f32| {
        p.line_segment([pos2(r.left() + 20.0, y), pos2(r.right() - 20.0, y)], Stroke::new(1.0, Tokens::chip_border()))
    };
    row_label(&p, col, y, "Language:");
    popup(ui, pos2(col, y), 150.0, "pref-lang", &mut s.language, crate::i18n::LANGUAGES);
    y += pitch;
    row_label(&p, col, y, "Theme:");
    popup(ui, pos2(col, y), 120.0, "pref-theme", &mut s.theme, &["System", "Light", "Dark"]);
    y += pitch;
    row_label(&p, col, y, "Start ShapeCraft with:");
    popup(ui, pos2(col, y), 120.0, "pref-start", &mut s.start_with, &["Recents", "New Project"]);
    y += pitch;
    row_label(&p, col, y, "Show Menu Button Labels:");
    popup(ui, pos2(col, y), 120.0, "pref-labels", &mut s.menu_labels, &["Always", "Never"]);
    y += pitch;
    row_label(&p, col, y, "Circular Annotations:");
    popup(ui, pos2(col, y), 160.0, "pref-circ", &mut s.circular_annotations, &["Radius and Diameter", "Always Radius"]);
    y += 22.0;
    let tip = "Radius and Diameter labels circles and round holes by\ndiameter; Always Radius uses the radius everywhere.";
    p.text(pos2(col, y), Align2::LEFT_TOP, tip, theme::body(10.0), Tokens::text_dim());
    y += 44.0;
    sep(&p, y);
    y += 24.0;
    row_label(&p, col, y, "Import:");
    check(ui, pos2(col, y), "pref-import", &mut s.always_advanced_import, "Always show advanced preferences");
    y += 24.0;
    sep(&p, y);
    y += 24.0;
    row_label(&p, col, y, "Single Key Action:");
    popup(ui, pos2(col, y), 120.0, "pref-single", &mut s.single_key_action, &["Hotkeys", "Off"]);
    y += pitch;
    row_label(&p, col, y, "Keyboard Shortcuts:");
    let customize = if shortcuts {
        small_button(ui, pos2(col, y), "pref-keys", "Customize…")
    } else {
        p.text(pos2(col, y), Align2::LEFT_CENTER, "Open a project to customize", theme::body(12.0), Tokens::text_dim());
        false
    };
    y += pitch;
    row_label(&p, col, y, "View Navigation:");
    let presets = small_button(ui, pos2(col, y), "pref-nav", "Navigation Presets…");
    y += pitch;
    // The first run's device question (onboarding); changing it picks that device's navigation preset.
    row_label(&p, col, y, "Input Device:");
    let names: Vec<&str> = crate::onboarding::DEVICES.iter().map(|d| d.0).collect();
    let mut device = crate::onboarding::DEVICES.iter().find(|d| d.1 == s.device).map_or("Mouse and Keyboard", |d| d.0).to_string();
    let before = device.clone();
    popup(ui, pos2(col, y), 170.0, "pref-device", &mut device, &names);
    if device != before
        && let Some(d) = crate::onboarding::DEVICES.iter().find(|d| d.0 == device)
    {
        s.device = d.1.to_string();
        s.nav_preset = d.2.to_string();
    }
    y += pitch;
    row_label(&p, col, y, "Zoom:");
    check(ui, pos2(col, y), "pref-zoom", &mut s.reverse_scroll, "Reverse scroll direction");
    y += pitch;
    row_label(&p, col, y, "Selection Extension:");
    check(ui, pos2(col, y), "pref-ext", &mut s.extend_needs_shift, "Require holding shift");
    y += 24.0;
    sep(&p, y);
    y += 24.0;
    row_label(&p, col, y, "Rendering Quality:");
    popup(ui, pos2(col, y), 120.0, "pref-render", &mut s.rendering_quality, &["High", "Medium", "Low"]);
    y += pitch;
    row_label(&p, col, y, "Tessellation Quality:");
    popup(ui, pos2(col, y), 120.0, "pref-tess", &mut s.tessellation_quality, &["High (Default)", "Medium", "Low"]);
    y += pitch;
    row_label(&p, col, y, "Tutorial Mode:");
    check(ui, pos2(col, y), "pref-clicks", &mut s.tutorial_clicks, "Display clicks");
    y += 26.0;
    check(ui, pos2(col, y), "pref-keyshow", &mut s.tutorial_keys, "Display keyboard presses");
    y += 22.0;
    p.text(pos2(col, y), Align2::LEFT_CENTER, "Restart the app to apply these settings.", theme::body(10.0), Tokens::text_dim());
    y += 20.0;
    sep(&p, y);
    y += 20.0;
    p.text(
        pos2(r.center().x, y),
        Align2::CENTER_CENTER,
        format!("App Version: {}", env!("CARGO_PKG_VERSION")),
        theme::body(10.5),
        Tokens::text_dim(),
    );
    (customize, presets)
}

fn mouse_glyph(p: &egui::Painter, c: Pos2, scale: f32, lit: Option<Button>, wheel: bool) {
    let body = Rect::from_center_size(c, vec2(22.0, 34.0) * scale);
    p.rect_filled(body, radius((11.0 * scale) as u8), theme::c(0x6e6e72));
    p.line_segment(
        [pos2(body.left(), body.top() + 13.0 * scale), pos2(body.right(), body.top() + 13.0 * scale)],
        Stroke::new(1.0, theme::c(0x4a4a4e)),
    );
    p.line_segment([pos2(c.x, body.top()), pos2(c.x, body.top() + 13.0 * scale)], Stroke::new(1.0, theme::c(0x4a4a4e)));
    let half = |left: bool| {
        Rect::from_min_max(
            pos2(if left { body.left() + 2.0 } else { c.x + 1.0 }, body.top() + 2.0),
            pos2(if left { c.x - 1.0 } else { body.right() - 2.0 }, body.top() + 12.0 * scale),
        )
    };
    if let Some(b @ (Button::Left | Button::Right)) = lit {
        p.rect_filled(half(b == Button::Left), radius(3), Tokens::accent());
    }
    let wheel_r = Rect::from_center_size(pos2(c.x, body.top() + 8.0 * scale), vec2(4.0, 9.0) * scale.max(0.6));
    p.rect_filled(wheel_r, radius(2), if lit == Some(Button::Middle) || wheel { Tokens::accent() } else { theme::c(0x3a3a3e) });
}

fn arrow(p: &egui::Painter, from: Pos2, to: Pos2) {
    let col = theme::c(0x3f7bff);
    p.line_segment([from, to], Stroke::new(1.6, col));
    let d = (to - from).normalized();
    let n = vec2(-d.y, d.x);
    p.add(Shape::convex_polygon(vec![to + d * 2.0, to - d * 5.0 + n * 4.0, to - d * 5.0 - n * 4.0], col, Stroke::NONE));
}

fn keycap(p: &egui::Painter, c: Pos2, text: &str) {
    let g = p.layout_no_wrap(text.to_string(), theme::body(8.0), Tokens::text());
    let r = Rect::from_center_size(c, vec2(g.size().x + 8.0, 11.0));
    p.rect_stroke(r, radius(2), Stroke::new(1.0, Tokens::text_dim()), StrokeKind::Inside);
    p.galley(pos2(r.left() + 4.0, c.y - g.size().y / 2.0), g, Tokens::text());
    p.text(pos2(r.left() - 5.0, c.y), Align2::CENTER_CENTER, "+", theme::body(9.0), Tokens::text_dim());
}

fn mods_label(m: Mods) -> Option<&'static str> {
    match m {
        Mods::None => None,
        Mods::Shift => Some("SHIFT"),
        Mods::Ctrl => Some("CTRL"),
        Mods::Alt => Some("ALT"),
        Mods::AltShift => Some("ALT+SHIFT"),
    }
}

/// One gesture in the diagram: the device, arrows for the motion, and the keys to hold.
fn gesture(p: &egui::Painter, c: Pos2, action: NavAction, button: Option<Button>, mods: Option<&str>, trackpad: bool) {
    if trackpad {
        let pad = Rect::from_center_size(c, vec2(36.0, 26.0));
        p.rect_filled(pad, radius(4), theme::c(0x6e6e72));
        p.rect_stroke(pad, radius(4), Stroke::new(1.0, theme::c(0x8a8a8e)), StrokeKind::Inside);
        let fingers =
            if action == NavAction::Zoom { [c + vec2(-6.0, 4.0), c + vec2(6.0, -4.0)] } else { [c + vec2(-3.0, 2.0), c + vec2(3.0, 2.0)] };
        for f in fingers {
            p.circle_filled(f, 3.5, Color32::WHITE);
        }
    } else {
        mouse_glyph(p, c, 1.0, button, button.is_none());
    }
    match action {
        NavAction::Zoom if !trackpad && button.is_none() => arrow(p, c + vec2(-22.0, 0.0), c + vec2(-22.0, -18.0)),
        NavAction::Zoom if trackpad => {
            arrow(p, c + vec2(8.0, -6.0), c + vec2(18.0, -16.0));
            arrow(p, c + vec2(-8.0, 6.0), c + vec2(-18.0, 16.0));
        }
        _ => {
            arrow(p, c + vec2(-22.0, 4.0), c + vec2(-34.0, 4.0));
            arrow(p, c + vec2(22.0, 4.0), c + vec2(34.0, 4.0));
            arrow(p, c + vec2(0.0, -22.0), c + vec2(0.0, -34.0));
            arrow(p, c + vec2(0.0, 22.0), c + vec2(0.0, 34.0));
        }
    }
    if let Some(m) = mods {
        keycap(p, c + vec2(4.0, 48.0), m);
    }
}

/// The Navigation Presets modal: presets on the left, how the chosen one pans, orbits and zooms on the right.
fn nav_presets(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    presets_window(ui, full, &mut ws.nav_presets, &mut ws.settings, false);
}

/// The Navigation Presets modal with no Workspace behind it. `chosen` is None once it closes.
///
/// `welcome` is the first run's second question (ours, the reference stops at the device): the same window, asked
/// as "which app do your hands already know", with Skip (keep the device's preset) and Use This.
pub(crate) fn presets_window(ui: &mut Ui, full: Rect, nav: &mut Option<&'static str>, settings: &mut Settings, welcome: bool) {
    let Some(chosen) = *nav else { return };
    // Tall enough for all twelve presets when the window allows it (at 644 the last three were cut off).
    let r = dialog(ui, full, vec2(742.0, (full.height() - 40.0).clamp(560.0, 720.0)), "");
    let p = ui.painter().clone();
    if welcome {
        p.text(
            pos2(r.left() + 19.0, r.top() + 27.0),
            Align2::LEFT_CENTER,
            "Make yourself at home",
            theme::strong(21.0),
            Tokens::text_strong(),
        );
        p.text(
            pos2(r.left() + 19.0, r.top() + 52.0),
            Align2::LEFT_CENTER,
            "Pick the app your hands already know, and orbit, pan and zoom work the way they do there. \
             Change it any time in Preferences.",
            theme::body(12.0),
            Tokens::text_dim(),
        );
    } else {
        p.text(
            pos2(r.left() + 19.0, r.top() + 36.0),
            Align2::LEFT_CENTER,
            "Navigation Presets",
            theme::strong(21.0),
            Tokens::text_strong(),
        );
    }
    let body = Rect::from_min_max(pos2(r.left(), r.top() + 70.0), pos2(r.right(), r.bottom() - 60.0));
    p.line_segment([body.left_top(), body.right_top()], Stroke::new(1.0, Tokens::chip_border()));
    let list = Rect::from_min_max(body.min, pos2(body.left() + 238.0, body.bottom()));
    p.line_segment([list.right_top(), list.right_bottom()], Stroke::new(1.0, Tokens::chip_border()));
    let mut y = list.top() + 20.0;
    let mut picked = None;
    for (group, own) in [("ShapeCraft Presets", true), ("Popular Presets", false)] {
        p.text(pos2(list.left() + 12.0, y), Align2::LEFT_CENTER, group, theme::strong(11.5), Tokens::text_dim());
        y += 16.0;
        for preset in PRESETS.iter().filter(|p| p.own == own) {
            let h = if preset.recommended { 52.0 } else { 40.0 };
            if y + h > list.bottom() {
                break;
            }
            let row = Rect::from_min_size(pos2(list.left() + 8.0, y), vec2(list.width() - 16.0, h));
            let resp = ui.interact(row, ui.scope_id().with(("preset", preset.name)), Sense::click());
            let on = preset.name == chosen;
            if on {
                p.rect_filled(row, radius(5), theme::c(0x2453c9));
            } else if resp.hovered() {
                p.rect_filled(row, radius(5), Tokens::row_hover());
            }
            let mut ty = row.center().y - 6.0;
            if preset.recommended {
                p.text(pos2(row.left() + 36.0, row.top() + 10.0), Align2::LEFT_CENTER, "Recommended", theme::body(9.5), Tokens::text_dim());
                ty += 6.0;
            }
            let tile = Rect::from_center_size(pos2(row.left() + 18.0, ty + 6.0), vec2(18.0, 18.0));
            p.rect_filled(tile, radius(4), if on { Color32::WHITE } else { Tokens::button() });
            let initial: String = preset.name.chars().take(1).collect();
            p.text(
                tile.center(),
                Align2::CENTER_CENTER,
                initial,
                theme::strong(11.0),
                if on { theme::c(0x2453c9) } else { Tokens::text_strong() },
            );
            p.text(pos2(row.left() + 36.0, ty), Align2::LEFT_CENTER, preset.name, theme::strong(12.5), Tokens::text_strong());
            p.text(pos2(row.left() + 36.0, ty + 14.0), Align2::LEFT_CENTER, preset.devices, theme::body(10.0), Tokens::text_strong());
            if on {
                p.text(pos2(row.right() - 12.0, row.center().y), Align2::CENTER_CENTER, "✓", theme::body(12.5), Color32::WHITE);
            }
            if resp.clicked() {
                picked = Some(preset.name);
            }
            y += h + 2.0;
        }
        y += 10.0;
    }
    // The diagram.
    let preset = PRESETS.iter().find(|p| p.name == chosen).unwrap_or_else(|| settings.preset());
    let dx = list.right();
    let cols = [("Pan", NavAction::Pan), ("Orbit", NavAction::Orbit), ("Zoom", NavAction::Zoom)];
    let col_x = |k: usize| dx + 82.0 + k as f32 * 152.0;
    for (k, (label, _)) in cols.iter().enumerate() {
        p.text(pos2(col_x(k), body.top() + 34.0), Align2::CENTER_CENTER, *label, theme::body(12.0), Tokens::text_strong());
    }
    let mut lowest: f32 = body.top() + 140.0;
    for (k, (_, action)) in cols.iter().enumerate() {
        let mut gy = body.top() + 104.0;
        let mut any = false;
        for b in preset.mouse.iter().filter(|b| b.action == *action) {
            gesture(&p, pos2(col_x(k), gy), *action, Some(b.button), mods_label(b.mods), false);
            gy += 126.0;
            any = true;
        }
        if *action == NavAction::Zoom {
            gesture(&p, pos2(col_x(k), gy), NavAction::Zoom, None, None, false);
            gy += 126.0;
            any = true;
        }
        if !any {
            p.text(pos2(col_x(k), gy), Align2::CENTER_CENTER, "—", theme::body(12.0), Tokens::text_dim());
        }
        lowest = lowest.max(gy - 50.0);
    }
    if preset.devices.contains("Trackpad") {
        p.line_segment([pos2(dx + 16.0, lowest), pos2(body.right() - 16.0, lowest)], Stroke::new(1.0, Tokens::chip_border()));
        let ty = lowest + 70.0;
        if ty + 40.0 < body.bottom() {
            gesture(&p, pos2(col_x(0), ty), NavAction::Pan, None, Some("SHIFT"), true);
            gesture(&p, pos2(col_x(1), ty), NavAction::Orbit, None, None, true);
            gesture(&p, pos2(col_x(2), ty), NavAction::Zoom, None, None, true);
        }
    }
    let footer = Rect::from_min_max(pos2(r.left(), body.bottom()), r.max);
    p.rect_filled(footer, egui::CornerRadius { nw: 0, ne: 0, sw: 11, se: 11 }, theme::c(0x232325));
    let (no, yes) = if welcome { ("Skip", "Use This") } else { ("Cancel", "Apply") };
    let cancel = button(ui, Rect::from_min_size(pos2(r.right() - 166.0, r.bottom() - 36.0), vec2(70.0, 20.0)), no, false, "nav-cancel");
    let apply = button(ui, Rect::from_min_size(pos2(r.right() - 86.0, r.bottom() - 36.0), vec2(70.0, 20.0)), yes, true, "nav-apply");
    if let Some(name) = picked {
        *nav = Some(name);
    }
    if apply {
        settings.nav_preset = chosen.to_string();
        *nav = None;
    } else if cancel || close_on_escape(ui) {
        *nav = None;
    }
}
