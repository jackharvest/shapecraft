//! The title bar: home button, project name (renamed in place), the place for the menus and Preferences where there is
//! no system menu bar (Linux, Windows), and window dragging.

use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::select_all;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};
use crate::window_chrome;
use crate::workspace::Workspace;
use crate::workspace::toolbar::UiAction;

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
    let system_menu_bar = cfg!(target_os = "macos");
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
        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
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
        if ui.interact(name_rect, ui.scope_id().with("title-name"), Sense::click()).double_clicked() {
            ws.renaming = Some((None, ws.session.doc.name.clone(), true));
        }
    }
    ws.menus_at = pos2(name_end + 12.0, r.top() + 25.0);
    // A 17 px ring in the home button's grey (measured).
    let right = r.right() - window_chrome::BUTTONS_W;
    let help = pos2(right - 27.0, r.top() + 25.0);
    if bar_button(ui, help, "help", Icon::Help, ink).clicked() {
        ws.ui_action(UiAction::Help);
    }
    if !system_menu_bar && bar_button(ui, help - vec2(34.0, 0.0), "preferences", Icon::Gear, ink).on_hover_text("Preferences").clicked() {
        ws.ui_action(UiAction::Preferences);
    }
    // Drag the window from the empty title bar.
    let (drag_left, drag_right) = if system_menu_bar { (r.left() + 360.0, right - 60.0) } else { (ws.menus_at.x, right - 94.0) };
    window_chrome::drag_area(ui, Rect::from_min_max(pos2(drag_left, r.top()), pos2(drag_right, r.bottom())), "titledrag");
}
