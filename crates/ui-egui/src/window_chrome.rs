//! The window's frame where we draw all of it (Linux): a desktop's own title bar would stack on top of ours, so the
//! window has no decorations and these are the minimize, maximize and close buttons, the resize edges and the drag area.

use egui::viewport::ResizeDirection;
use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, ViewportCommand, pos2, vec2};

use crate::theme::{self, Tokens, radius};

pub const OWN_FRAME: bool = cfg!(target_os = "linux");

const BUTTON_STEP: f32 = 38.0;
const EDGE: f32 = 5.0;
const CORNER: f32 = 14.0;

/// How much of the title row's right end the window buttons take.
pub const BUTTONS_W: f32 = if OWN_FRAME { 3.0 * BUTTON_STEP } else { 0.0 };

/// This window draws its own frame (Linux; never the tablet's copy, which has no window).
pub fn own_frame() -> bool {
    OWN_FRAME && !crate::tablet::on_tablet()
}

/// [`BUTTONS_W`] where there are window buttons to make room for.
pub fn buttons_w() -> f32 {
    if own_frame() { BUTTONS_W } else { 0.0 }
}

#[derive(Clone, Copy)]
enum Button {
    Minimize,
    Maximize,
    Close,
}

fn glyph(ui: &Ui, button: Button, c: Pos2, maximized: bool, ink: Color32) {
    let p = ui.painter();
    let stroke = Stroke::new(1.5, ink);
    match button {
        Button::Minimize => {
            p.line_segment([c + vec2(-6.0, 0.5), c + vec2(6.0, 0.5)], stroke);
        }
        Button::Maximize if maximized => {
            p.rect_stroke(Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(8.0, 8.0)), 1.0, stroke, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-2.5, -4.5), c + vec2(4.5, -4.5)], stroke);
            p.line_segment([c + vec2(4.5, -4.5), c + vec2(4.5, 2.5)], stroke);
        }
        Button::Maximize => {
            p.rect_stroke(Rect::from_center_size(c, vec2(11.0, 11.0)), 1.0, stroke, egui::StrokeKind::Middle);
        }
        Button::Close => {
            p.line_segment([c + vec2(-5.5, -5.5), c + vec2(5.5, 5.5)], stroke);
            p.line_segment([c + vec2(-5.5, 5.5), c + vec2(5.5, -5.5)], stroke);
        }
    }
}

fn button(ui: &mut Ui, button: Button, center: Pos2, maximized: bool, ink: Color32) -> bool {
    let hit = Rect::from_center_size(center, vec2(30.0, 28.0));
    let resp = ui.interact(hit, ui.scope_id().with(("window-button", button as u8)), Sense::click());
    let closing = matches!(button, Button::Close) && resp.hovered();
    if closing {
        ui.painter().rect_filled(hit, radius(6), Color32::from_rgb(0xe8, 0x11, 0x23));
    } else if resp.hovered() {
        ui.painter().rect_filled(hit, radius(6), Tokens::button_hover());
    }
    glyph(ui, button, center, maximized, if closing { Color32::WHITE } else { ink });
    resp.clicked()
}

fn resize_edges(ui: &mut Ui, full: Rect) {
    let (l, r, t, b) = (full.left(), full.right(), full.top(), full.bottom());
    let zones = [
        (Rect::from_min_max(pos2(l + CORNER, t), pos2(r - CORNER, t + EDGE)), ResizeDirection::North, CursorIcon::ResizeNorth),
        (Rect::from_min_max(pos2(l + CORNER, b - EDGE), pos2(r - CORNER, b)), ResizeDirection::South, CursorIcon::ResizeSouth),
        (Rect::from_min_max(pos2(l, t + CORNER), pos2(l + EDGE, b - CORNER)), ResizeDirection::West, CursorIcon::ResizeWest),
        (Rect::from_min_max(pos2(r - EDGE, t + CORNER), pos2(r, b - CORNER)), ResizeDirection::East, CursorIcon::ResizeEast),
        (Rect::from_min_size(pos2(l, t), vec2(CORNER, CORNER)), ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest),
        (Rect::from_min_size(pos2(r - CORNER, t), vec2(CORNER, CORNER)), ResizeDirection::NorthEast, CursorIcon::ResizeNorthEast),
        (Rect::from_min_size(pos2(l, b - CORNER), vec2(CORNER, CORNER)), ResizeDirection::SouthWest, CursorIcon::ResizeSouthWest),
        (Rect::from_min_size(pos2(r - CORNER, b - CORNER), vec2(CORNER, CORNER)), ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast),
    ];
    for (i, (rect, direction, cursor)) in zones.into_iter().enumerate() {
        let resp = ui.interact(rect, ui.scope_id().with(("window-edge", i)), Sense::click_and_drag());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(cursor);
            if ui.input(|input| input.pointer.primary_pressed()) {
                ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(direction));
            }
        }
    }
}

/// Drag the window from `rect`; a double click maximizes it.
pub fn drag_area(ui: &mut Ui, rect: Rect, id: &str) {
    let drag = ui.interact(rect, ui.scope_id().with(id), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!max));
    }
}

/// Draws the frame over whichever screen is showing. Goes last so that it wins the window's edges.
pub fn show(ui: &mut Ui) {
    if !own_frame() {
        return;
    }
    let ctx = ui.ctx().clone();
    let full = ctx.content_rect();
    let (maximized, fullscreen, focused) = ctx.input(|i| {
        let v = i.viewport();
        (v.maximized.unwrap_or(false), v.fullscreen.unwrap_or(false), v.focused.unwrap_or(true))
    });
    if fullscreen {
        return;
    }
    if !maximized {
        resize_edges(ui, full);
    }
    let ink = if focused { theme::c(0x9d9c9d) } else { theme::c(0x686566) };
    let at = |slot: f32| pos2(full.right() - (slot + 0.5) * BUTTON_STEP, full.top() + 25.0);
    if button(ui, Button::Close, at(0.0), maximized, ink) {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
    if button(ui, Button::Maximize, at(1.0), maximized, ink) {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
    if button(ui, Button::Minimize, at(2.0), maximized, ink) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}
