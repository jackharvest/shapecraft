//! The title bar: home button, project name (renamed in place), the place for the menus and Preferences where there is
//! no system menu bar (Linux, Windows), and window dragging.

use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::select_all;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};
use crate::window_chrome;
use crate::workspace::toolbar::UiAction;
use crate::workspace::{PEN_BUBBLE_SECS, Workspace};

/// The title bar's home button, as observed: a filled grey house (19×17) with a chimney and a door.
fn home_button(ui: &mut Ui, center: Pos2, ink: Color32, bar: Color32) -> bool {
    let hit = Rect::from_center_size(center, vec2(28.0, 26.0));
    let resp = ui.interact(hit, ui.scope_id().with("home"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(hit, radius(6), Tokens::button_hover());
    }
    let p = ui.painter();
    let o = center - vec2(9.5, 8.5);
    let at = |x: f32, y: f32| o + vec2(x, y);
    p.rect_filled(Rect::from_min_max(at(13.0, 1.5), at(15.5, 7.0)), 0.0, ink);
    p.add(Shape::convex_polygon(
        vec![at(9.5, 0.0), at(19.0, 8.5), at(17.5, 10.0), at(9.5, 2.8), at(1.5, 10.0), at(0.0, 8.5)],
        ink,
        Stroke::NONE,
    ));
    p.add(Shape::convex_polygon(vec![at(9.5, 4.5), at(16.0, 10.0), at(16.0, 17.0), at(3.0, 17.0), at(3.0, 10.0)], ink, Stroke::NONE));
    p.rect_filled(Rect::from_min_max(at(7.5, 11.0), at(11.5, 17.0)), 0.0, bar);
    resp.clicked()
}

/// A round-cornered icon button on the bar, like Help.
fn bar_button(ui: &mut Ui, center: Pos2, id: &str, icon: Icon, ink: Color32) -> egui::Response {
    let hit = Rect::from_center_size(center, vec2(28.0, 26.0));
    let resp = ui.interact(hit, ui.scope_id().with(id), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(hit, radius(6), Tokens::button_hover());
    }
    icons::paint(ui.painter(), Rect::from_center_size(center, vec2(20.0, 20.0)), icon, ink);
    resp
}

pub(in crate::workspace) fn titlebar(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    // An inactive window greys its title bar (as observed: #363233 behind, #777374 text).
    let focused = ui.input(|i| i.viewport().focused).unwrap_or(true);
    let bar = if focused { Tokens::titlebar() } else { theme::c(0x363233) };
    let ink = if focused { theme::c(0x9d9c9d) } else { theme::c(0x686566) };
    // The window title reads #EBEAEB (light #484747), a little brighter than other text (s23).
    let text = if focused { theme::c(0xebeaeb) } else { theme::c(0x777374) };
    ui.painter().rect_filled(r, 0.0, bar);
    ui.painter().line_segment([r.left_bottom() + vec2(0.0, 0.5), r.right_bottom() + vec2(0.0, 0.5)], Stroke::new(1.0, theme::c(0x000000)));
    // Leave room for the window buttons on macOS (the title bar is ours, drawn under them); in full screen there
    // are none and everything moves 77 px left (measured). Elsewhere the system draws its buttons in its own bar, and
    // the menus and Preferences join ours: home, name, menus, as Shapr3D's help describes its Windows bar.
    // A tablet has no menu bar of its own: there the menus join ours, as elsewhere.
    let system_menu_bar = cfg!(target_os = "macos") && !crate::tablet::on_tablet();
    let fullscreen = ui.input(|i| i.viewport().fullscreen).unwrap_or(false);
    let shift = if fullscreen || !system_menu_bar { 77.0 } else { 0.0 };
    if home_button(ui, pos2(r.left() + 107.5 - shift, r.top() + 25.5), ink, bar) {
        ws.go_home = true;
    }
    let name_rect = Rect::from_min_size(pos2(r.left() + 128.0 - shift, r.center().y - 13.0), vec2(320.0, 26.0));
    let mut name_end = name_rect.right();
    if let Some((None, text, focus)) = &mut ws.renaming {
        let edit_id = ui.scope_id().with("rename-project");
        let edit = egui::TextEdit::singleline(text).id(edit_id).font(theme::strong(16.0)).desired_width(300.0);
        let resp = ui.put(name_rect, edit);
        if *focus {
            resp.request_focus();
            select_all(ui.ctx(), edit_id, text.chars().count());
            *focus = false;
        }
        // The key that ends the rename is used up here: the workspace reads keys later in the frame, and would take
        // Return for Search Commands and Escape for deselecting.
        let (enter, esc) = ui.input_mut(|i| {
            (i.consume_key(egui::Modifiers::NONE, egui::Key::Enter), i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        });
        if enter || resp.lost_focus() && !esc {
            let new = text.clone();
            ws.renaming = None;
            let _ = ws.run("rename_project", json!({ "name": new }));
        } else if esc {
            ws.renaming = None;
        }
    } else {
        let name = ws.session.doc.name.clone();
        // A long name stops short of the menus; on macOS nothing follows it.
        let painter = if system_menu_bar { ui.painter().clone() } else { ui.painter().with_clip_rect(name_rect) };
        let drawn = painter.text(pos2(r.left() + 134.0 - shift, r.top() + 25.0), Align2::LEFT_CENTER, name, theme::strong(16.0), text);
        name_end = name_end.min(drawn.right());
        // A double-click renames, as on the desktop; on a touch screen one tap does (two taps are rarely quick enough).
        let resp = ui.interact(name_rect, ui.scope_id().with("title-name"), Sense::click());
        if resp.double_clicked() || crate::tablet::touch_ui() && resp.clicked() {
            ws.renaming = Some((None, ws.session.doc.name.clone(), true));
        }
    }
    ws.menus_at = pos2(name_end + 12.0, r.top() + 25.0);
    // A 17 px ring in the home button's grey (measured).
    let right = r.right() - window_chrome::buttons_w();
    let help = pos2(right - 27.0, r.top() + 25.0);
    if bar_button(ui, help, "help", Icon::Help, ink).clicked() {
        ws.ui_action(UiAction::Help);
    }
    if !system_menu_bar
        && bar_button(ui, help - vec2(34.0, 0.0), "preferences", Icon::Gear, ink).on_hover_text(crate::i18n::tr("Preferences")).clicked()
    {
        ws.ui_action(UiAction::Preferences);
    }
    // Drag the window from the empty title bar.
    let (drag_left, drag_right) = if system_menu_bar { (r.left() + 360.0, right - 60.0) } else { (ws.menus_at.x, right - 94.0) };
    window_chrome::drag_area(ui, Rect::from_min_max(pos2(drag_left, r.top()), pos2(drag_right, r.bottom())), "titledrag");
}

/// The title on a touch screen, as on iPad: no title bar. Sideways, a pill floats at the top
/// centre of the view (home | project name | ⋯), level with the tool column's first button; upright, a slim bar runs
/// across the top (`bar` has height then). The name opens a NAME popover to rename the project; ⋯ holds what the menu
/// bar holds elsewhere: Import, Export, Help, Settings.
pub(in crate::workspace) fn touch_title(ws: &mut Workspace, ui: &mut Ui, bar: Rect) {
    let ink = theme::c(0xd6d5d6);
    let fill = theme::c(0x2b2b2d);
    let name = ws.session.doc.name.clone();
    let font = theme::strong(17.0);
    let name_w = ui.painter().layout_no_wrap(name.clone(), font.clone(), Color32::WHITE).size().x.min(360.0);
    let (frame, home_c, name_left, more_c) = if bar.height() > 0.0 {
        ui.painter().rect_filled(bar, 0.0, Tokens::titlebar());
        (bar, pos2(bar.left() + 33.0, bar.center().y), bar.left() + 62.0, pos2(bar.right() - 31.0, bar.center().y))
    } else {
        // Measured on iPad: 44 pt tall; home 50.5 wide, the name, ⋯ 46 wide, 8 pt in from each end.
        let vp = ws.viewport_rect;
        let w = 8.0 + 50.0 + 8.0 + name_w + 14.0 + 46.0 + 8.0;
        let pill = Rect::from_center_size(pos2(vp.center().x, vp.top() + 9.0 + 22.0), vec2(w, 44.0));
        let p = ui.painter();
        p.rect_filled(pill.translate(vec2(0.0, 3.0)).expand(1.0), radius(14), Color32::from_black_alpha(60));
        p.rect_filled(pill, radius(13), fill);
        p.rect_stroke(pill, radius(13), Stroke::new(1.0, theme::c(0x3c3c3e)), egui::StrokeKind::Inside);
        (pill, pos2(pill.left() + 33.0, pill.center().y), pill.left() + 66.0, pos2(pill.right() - 31.0, pill.center().y))
    };
    ws.pill_bottom = if bar.height() > 0.0 { 0.0 } else { frame.bottom() };
    if ws.touch_opened.is_none() {
        ws.touch_opened = Some(ws.now);
    }
    if ws.pen_bubble() {
        // Measured on iPad: a red-tinted capsule 10 pt under the pill, 27.5 tall. A tap opens its card.
        let text = if crate::tablet::on_tablet() { "Apple Pencil may not be connected" } else { "Stylus may not be connected" };
        let g = ui.painter().layout_no_wrap(text.into(), theme::body(13.0), theme::c(0xff6961));
        let r = Rect::from_center_size(pos2(frame.center().x, frame.bottom() + 10.0 + 14.0), vec2(g.size().x + 28.0, 27.5));
        ui.painter().rect_filled(r, radius(14), theme::c(0x3a1f1f));
        ui.painter().galley(r.center() - g.size() / 2.0, g, theme::c(0xff6961));
        if ui.interact(r.expand(6.0), ui.scope_id().with("pen-bubble"), Sense::click()).clicked() {
            ws.pen_help = !ws.pen_help;
        }
        if ws.pen_help {
            pen_help_card(ws, ui, pos2(frame.center().x, r.bottom() + 8.0));
        }
        // Wake up to take it away even if nothing else moves.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    } else {
        ws.pen_help = false;
    }
    if home_button(ui, home_c, ink, fill) {
        ws.go_home = true;
    }
    let name_rect = Rect::from_min_size(pos2(name_left, frame.center().y - 14.0), vec2(name_w, 28.0));
    ui.painter().with_clip_rect(name_rect.expand(2.0)).text(
        pos2(name_left, frame.center().y),
        Align2::LEFT_CENTER,
        &name,
        font,
        Tokens::text_strong(),
    );
    if ui.interact(name_rect.expand2(vec2(4.0, 8.0)), ui.scope_id().with("touch-name"), Sense::click()).clicked() {
        // The field starts empty, the current name showing as its placeholder.
        ws.renaming = Some((None, String::new(), true));
        ws.title_menu = false;
    }
    let more = Rect::from_center_size(more_c, vec2(46.0, 40.0));
    let more_resp = ui.interact(more, ui.scope_id().with("touch-more"), Sense::click());
    ui.painter().circle_stroke(more_c, 11.0, Stroke::new(1.6, ink));
    for dx in [-5.0, 0.0, 5.0] {
        ui.painter().circle_filled(more_c + vec2(dx, 0.0), 1.6, ink);
    }
    if more_resp.clicked() {
        ws.title_menu = !ws.title_menu;
    }
    let below = frame.bottom() + 8.0;
    if let Some((None, text, focus)) = &mut ws.renaming {
        let popover = Rect::from_center_size(pos2(name_rect.center().x, below + 50.0), vec2(375.0, 100.0));
        let mut done: Option<bool> = None;
        egui::Area::new(ui.scope_id().with("name-popover")).fixed_pos(popover.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            let p = ui.painter();
            p.rect_filled(popover.translate(vec2(0.0, 6.0)), radius(14), Color32::from_black_alpha(80));
            p.rect_filled(popover, radius(13), theme::c(0x2c2c2e));
            p.text(
                popover.left_top() + vec2(32.0, 30.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr("NAME"),
                theme::body(13.0),
                Tokens::text_dim(),
            );
            let field = Rect::from_min_size(popover.left_top() + vec2(16.0, 50.0), vec2(343.0, 36.0));
            ui.painter().rect_filled(field, radius(9), theme::c(0x3a3a3c));
            let edit_id = ui.scope_id().with("rename-project-touch");
            let edit = egui::TextEdit::singleline(text)
                .id(edit_id)
                .hint_text(&name)
                .font(theme::body(16.0))
                .frame(egui::Frame::NONE)
                .desired_width(field.width() - 24.0);
            let resp = ui.put(field.shrink2(vec2(12.0, 7.0)), edit);
            if *focus {
                resp.request_focus();
                *focus = false;
            }
            let (enter, esc) = ui.input_mut(|i| {
                (i.consume_key(egui::Modifiers::NONE, egui::Key::Enter), i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            });
            if enter || resp.lost_focus() && !esc {
                done = Some(true);
            } else if esc {
                done = Some(false);
            }
        });
        match done {
            Some(true) => {
                let new = text.trim().to_string();
                ws.renaming = None;
                if !new.is_empty() && new != name {
                    let _ = ws.run("rename_project", json!({ "name": new }));
                }
            }
            Some(false) => ws.renaming = None,
            None => {}
        }
    }
    if ws.title_menu {
        touch_more_menu(ws, ui, more, more_c, below, ink);
    } else {
        ws.title_sub = None;
    }
}

/// The pen bubble's card: what a stylus needs (connected, and seen by the device as a pen, since fingers only move
/// the view), and the mouse instead.
fn pen_help_card(ws: &mut Workspace, ui: &mut Ui, top: Pos2) {
    let ipad = crate::tablet::on_tablet();
    let (title, body, alt) = if ipad {
        (
            "No Apple Pencil yet",
            "Connect or pair your Apple Pencil, then tap with it. ShapeCraft picks and draws with the Pencil; fingers \
             only move the view.",
            "No Pencil? A mouse or trackpad works too.",
        )
    } else {
        (
            "No stylus yet",
            "Connect or pair your stylus, then tap with it. ShapeCraft needs a stylus the device reports as a pen (an \
             S Pen or a USI pen, for example). Rubber-tip styluses count as fingers, and fingers only move the view.",
            "No stylus? A USB-C or Bluetooth mouse works too.",
        )
    };
    let width = 400.0;
    let wrap = |t: &str, font| ui.painter().layout(crate::i18n::tr_s(t), font, Tokens::text_dim(), width - 40.0);
    let (g_body, g_alt) = (wrap(body, theme::body(14.0)), wrap(alt, theme::body(14.0)));
    let height = 20.0 + 24.0 + 10.0 + g_body.size().y + 10.0 + g_alt.size().y + 16.0 + 36.0 + 16.0;
    let card = Rect::from_min_size(pos2(top.x - width / 2.0, top.y), vec2(width, height));
    let mut close = false;
    egui::Area::new(ui.scope_id().with("pen-help")).fixed_pos(card.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (_, _) = ui.allocate_exact_size(card.size(), Sense::click());
        let p = ui.painter();
        p.rect_filled(card.translate(vec2(0.0, 6.0)), radius(14), Color32::from_black_alpha(80));
        p.rect_filled(card, radius(13), theme::c(0x2c2c2e));
        let x = card.left() + 20.0;
        let mut y = card.top() + 20.0;
        p.text(pos2(x, y + 12.0), Align2::LEFT_CENTER, crate::i18n::tr_s(title), theme::strong(16.0), Tokens::text_strong());
        y += 34.0;
        let h = g_body.size().y;
        p.galley(pos2(x, y), g_body, Tokens::text_dim());
        y += h + 10.0;
        let h = g_alt.size().y;
        p.galley(pos2(x, y), g_alt, Tokens::text_dim());
        y += h + 16.0;
        let ok = Rect::from_min_size(pos2(card.right() - 20.0 - 90.0, y), vec2(90.0, 36.0));
        let mouse = Rect::from_min_size(pos2(ok.left() - 10.0 - 150.0, y), vec2(150.0, 36.0));
        let mouse_resp = ui.interact(mouse, ui.scope_id().with("pen-help-mouse"), Sense::click());
        let ok_resp = ui.interact(ok, ui.scope_id().with("pen-help-ok"), Sense::click());
        let p = ui.painter();
        p.rect_filled(mouse, radius(9), if mouse_resp.hovered() { theme::c(0x48484a) } else { theme::c(0x3a3a3c) });
        p.text(mouse.center(), Align2::CENTER_CENTER, crate::i18n::tr("Use a Mouse"), theme::strong(14.0), Tokens::text_strong());
        p.rect_filled(ok, radius(9), if ok_resp.hovered() { theme::c(0x2f7cf6) } else { Tokens::accent() });
        p.text(ok.center(), Align2::CENTER_CENTER, crate::i18n::tr("OK"), theme::strong(14.0), Color32::WHITE);
        if mouse_resp.clicked() {
            // As if the first run's answer had been the mouse (the app saves the workspace's settings).
            if ipad {
                ws.settings.tablet_device = "keyboard".into();
            } else {
                let (_, value, preset) = crate::onboarding::DEVICES[0];
                ws.settings.tablet_device = value.into();
                ws.settings.device = value.into();
                ws.settings.nav_preset = preset.into();
            }
            close = true;
        }
        if ok_resp.clicked() {
            close = true;
        }
    });
    if close {
        ws.pen_help = false;
        ws.pen_answered = true;
        ws.touch_opened = Some(ws.now - PEN_BUBBLE_SECS);
        ws.pen_nudged = None;
    }
}

/// What a row of the touch ⋯ menu does.
#[derive(Clone, Copy)]
enum Pick {
    App(crate::menubar::AppAction),
    Ui(UiAction),
    /// Opens submenu n beside the row.
    Sub(usize),
}

/// The touch ⋯ menu, as on iPad: Import › and Export to › open a submenu to the right, then Help and Settings.
/// (No Sync, Share, Take a Photo or Preview in AR: cloud and camera features we leave out.)
fn touch_more_menu(ws: &mut Workspace, ui: &mut Ui, more: Rect, more_c: Pos2, below: f32, ink: Color32) {
    use crate::menubar::AppAction;
    use crate::workspace::export::Format;
    const ROW: f32 = 44.0;
    const W: f32 = 252.0;
    let main: [(&str, Option<Icon>, Pick); 4] = [
        ("Import", Some(Icon::Import), Pick::Sub(0)),
        ("Export to", Some(Icon::Export), Pick::Sub(1)),
        ("Help", Some(Icon::Help), Pick::Ui(UiAction::Help)),
        ("Settings", Some(Icon::Gear), Pick::Ui(UiAction::Preferences)),
    ];
    let subs: [&[(&str, Option<Icon>, Pick)]; 2] = [
        &[
            ("File…", None, Pick::App(AppAction::Import)),
            ("Project…", None, Pick::App(AppAction::ImportProject)),
            ("Image…", None, Pick::Ui(UiAction::InsertImage)),
        ],
        &[
            ("Export…", None, Pick::App(AppAction::Export)),
            ("Export to Slicer…", None, Pick::App(AppAction::ExportAs(Format::Slicer))),
            ("Quick Export", None, Pick::App(AppAction::QuickExport)),
            ("Open Screenshot Tool…", None, Pick::Ui(UiAction::Screenshot)),
            ("Open Visualization…", None, Pick::Ui(UiAction::Visualization)),
        ],
    ];
    let menu = Rect::from_min_size(pos2(more_c.x - 236.0 + 23.0, below), vec2(W, 16.0 + main.len() as f32 * ROW + 9.0));
    let row_top = |i: usize| menu.top() + 8.0 + i as f32 * ROW + if i >= 2 { 9.0 } else { 0.0 };
    let mut picked: Option<Pick> = None;
    let mut rects = vec![menu];
    let paint_rows =
        |ui: &mut Ui, frame: Rect, rows: &[(&str, Option<Icon>, Pick)], sep_at: Option<usize>, key: &str, open: Option<usize>| {
            let p = ui.painter().clone();
            p.rect_filled(frame.translate(vec2(0.0, 6.0)), radius(16), Color32::from_black_alpha(80));
            p.rect_filled(frame, radius(16), theme::c(0x2c2c2e));
            let mut out = None;
            for (i, (label, icon, pick)) in rows.iter().enumerate() {
                let extra = if sep_at.is_some_and(|s| i >= s) { 9.0 } else { 0.0 };
                let row = Rect::from_min_size(frame.left_top() + vec2(8.0, 8.0 + i as f32 * ROW + extra), vec2(frame.width() - 16.0, ROW));
                if sep_at == Some(i) {
                    p.line_segment(
                        [pos2(row.left() + 12.0, row.top() - 5.0), pos2(row.right() - 12.0, row.top() - 5.0)],
                        Stroke::new(1.0, theme::c(0x48484a)),
                    );
                }
                let resp = ui.interact(row, ui.scope_id().with((key, i)), Sense::click());
                let lit = resp.hovered() || matches!(pick, Pick::Sub(n) if open == Some(*n));
                if lit {
                    p.rect_filled(row, radius(9), Tokens::button_hover());
                }
                let text_x = match icon {
                    Some(ic) => {
                        icons::paint(&p, Rect::from_center_size(row.left_center() + vec2(26.0, 0.0), vec2(20.0, 20.0)), *ic, ink);
                        50.0
                    }
                    None => 14.0,
                };
                p.text(
                    row.left_center() + vec2(text_x, 0.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(*label),
                    theme::body(16.0),
                    Tokens::text_strong(),
                );
                if let Pick::Sub(_) = pick {
                    icons::paint(
                        &p,
                        Rect::from_center_size(row.right_center() - vec2(18.0, 0.0), vec2(12.0, 12.0)),
                        Icon::ChevronRight,
                        Tokens::text_dim(),
                    );
                }
                if resp.clicked() {
                    out = Some(*pick);
                }
            }
            out
        };
    let open = ws.title_sub;
    egui::Area::new(ui.scope_id().with("title-menu")).fixed_pos(menu.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        if let Some(p) = paint_rows(ui, menu, &main, Some(2), "title-row", open) {
            picked = Some(p);
        }
    });
    if let Some(n) = open
        && let Some(rows) = subs.get(n)
    {
        // Measured on iPad: the submenu opens to the right, overlapping the menu by 10 pt, top aligned with its row.
        let frame = Rect::from_min_size(pos2(menu.right() - 10.0, row_top(n) - 8.0), vec2(236.0, 16.0 + rows.len() as f32 * ROW));
        rects.push(frame);
        egui::Area::new(ui.scope_id().with("title-sub")).fixed_pos(frame.min).order(egui::Order::Tooltip).show(ui.ctx(), |ui| {
            if let Some(p) = paint_rows(ui, frame, rows, None, "title-sub-row", None) {
                picked = Some(p);
            }
        });
    }
    let outside = ui.input(|i| {
        i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|at| !rects.iter().any(|r| r.contains(at)) && !more.contains(at))
    });
    match picked {
        Some(Pick::Sub(n)) => ws.title_sub = if ws.title_sub == Some(n) { None } else { Some(n) },
        Some(Pick::App(a)) => {
            ws.title_menu = false;
            ws.app_request = Some(a);
        }
        Some(Pick::Ui(u)) => {
            ws.title_menu = false;
            ws.ui_action(u);
        }
        None if outside => ws.title_menu = false,
        None => {}
    }
}
