//! The floating numpad on a touch screen with no keyboard. It opens when the pencil taps a
//! value pill, beside the pill, and types into the value field exactly as keys would.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::Workspace;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

const KEY: egui::Vec2 = egui::Vec2::new(52.0, 44.0);
const GAP: f32 = 6.0;
const PAD: f32 = 8.0;

#[derive(Clone, Copy)]
enum Pad {
    Char(char),
    Back,
    Close,
    Done,
}

/// The keys, row by row, with their width in key cells.
const ROWS: [&[(Pad, &str, f32)]; 4] = [
    &[(Pad::Char('7'), "7", 1.0), (Pad::Char('8'), "8", 1.0), (Pad::Char('9'), "9", 1.0), (Pad::Back, "", 1.0)],
    &[(Pad::Char('4'), "4", 1.0), (Pad::Char('5'), "5", 1.0), (Pad::Char('6'), "6", 1.0), (Pad::Char('-'), "−", 1.0)],
    &[(Pad::Char('1'), "1", 1.0), (Pad::Char('2'), "2", 1.0), (Pad::Char('3'), "3", 1.0), (Pad::Close, "", 1.0)],
    &[(Pad::Char('0'), "0", 2.0), (Pad::Char('.'), ".", 1.0), (Pad::Done, "", 1.0)],
];

impl Workspace {
    /// Values may have to be tapped in: a touch screen with no keyboard, or a pen or finger in use on a desktop whose
    /// keyboard hasn't been touched for a while (a 2-in-1 folded over).
    fn no_keyboard(&self) -> bool {
        const WHILE_S: f64 = 120.0;
        !crate::tablet::shows_shortcuts()
            || !crate::tablet::touch_ui()
                && (self.pen_input || self.finger_nav)
                && self.typed_at.is_none_or(|t| self.now - t > WHILE_S)
    }

    /// Whether the numpad shows: no keyboard to type with, a value being entered.
    pub(crate) fn numpad_open(&self) -> bool {
        self.no_keyboard()
            && self.numeric.is_some()
            && self.numpad_at.is_some()
            && (self.tool.takes_numbers() || self.dim_edit.is_some())
    }

    pub(super) fn numpad(&mut self, ui: &Ui) {
        // A value field open on a touch screen with no keyboard always has the numpad, whichever way it opened (a
        // dimension's or Thread's size tapped, a Distance Type picked): there is no other way to type into it (#84).
        if self.numpad_at.is_none()
            && self.numeric.is_some()
            && self.no_keyboard()
            && let Some(f) = crate::workspace::viewport::paint::field_rect(ui.ctx())
        {
            self.numpad_at = Some(f.center());
        }
        if !self.numpad_open() {
            if self.numeric.is_none() {
                self.numpad_at = None;
            }
            return;
        }
        let Some(at) = self.numpad_at else { return };
        let size = vec2(PAD * 2.0 + KEY.x * 4.0 + GAP * 3.0, PAD * 2.0 + 40.0 + GAP + KEY.y * 4.0 + GAP * 3.0);
        // Beside the pill, to its right where there's room, else its left; kept inside the view. Clear of the value
        // field when one is drawn, so its text and "123" key stay in sight.
        let vp = self.viewport_rect;
        let (left, right) = match crate::workspace::viewport::paint::field_rect(ui.ctx()) {
            Some(f) => (f.left() - 12.0, f.right() + 12.0),
            None => (at.x - 24.0, at.x + 24.0),
        };
        let mut x = right;
        if x + size.x > vp.right() - 8.0 {
            x = left - size.x;
        }
        let x = x.clamp(vp.left() + 8.0, (vp.right() - 8.0 - size.x).max(vp.left() + 8.0));
        let y = (at.y - size.y / 2.0).clamp(vp.top() + 8.0, (vp.bottom() - 8.0 - size.y).max(vp.top() + 8.0));
        let frame = Rect::from_min_size(pos2(x, y), size);
        let mut pressed: Option<Pad> = None;
        let shown = self.numeric.clone().unwrap_or_default();
        let fresh = self.numeric_fresh;
        egui::Area::new(ui.scope_id().with("touch-numpad")).fixed_pos(frame.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            let (_, _) = ui.allocate_exact_size(size, Sense::click());
            let p = ui.painter();
            p.rect_filled(frame.translate(vec2(0.0, 6.0)), radius(16), Color32::from_black_alpha(90));
            p.rect_filled(frame, radius(16), theme::c(0x2c2c2e));
            p.rect_stroke(frame, radius(16), Stroke::new(1.0, theme::c(0x3c3c3e)), egui::StrokeKind::Inside);
            // The value as typed (selected-looking while it is the pill's own value).
            let field = Rect::from_min_size(frame.min + vec2(PAD, PAD), vec2(size.x - PAD * 2.0, 40.0));
            p.rect_filled(field, radius(9), theme::c(0x1c1c1e));
            let g = p.layout_no_wrap(
                crate::i18n::tr_s(if shown.is_empty() { "0".into() } else { shown.clone() }),
                theme::strong(20.0),
                Tokens::text_strong(),
            );
            let text_at = pos2(field.right() - 12.0 - g.size().x, field.center().y - g.size().y / 2.0);
            if fresh && !shown.is_empty() {
                p.rect_filled(Rect::from_min_size(text_at, g.size()).expand(2.0), radius(4), Tokens::accent().gamma_multiply(0.6));
            }
            let dim = shown.is_empty();
            p.galley(text_at, g, if dim { Tokens::text_dim() } else { Tokens::text_strong() });
            let mut y = field.bottom() + GAP;
            for row in ROWS {
                let mut x = frame.left() + PAD;
                for (key, label, cells) in row.iter() {
                    let w = KEY.x * cells + GAP * (cells - 1.0);
                    let r = Rect::from_min_size(pos2(x, y), vec2(w, KEY.y));
                    let resp = ui.interact(r, ui.scope_id().with(("numpad", x as i32, y as i32)), Sense::click());
                    let fill = match key {
                        Pad::Done => Tokens::accent(),
                        _ if resp.is_pointer_button_down_on() => Tokens::button_active(),
                        _ => theme::c(0x3a3a3c),
                    };
                    p.rect_filled(r, radius(10), fill);
                    match key {
                        Pad::Back => backspace(p, r.center(), Tokens::icon()),
                        Pad::Close => icons::paint(p, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), Icon::Close, Tokens::icon()),
                        Pad::Done => {
                            p.text(r.center(), Align2::CENTER_CENTER, "✓", theme::strong(20.0), Color32::WHITE);
                        }
                        Pad::Char(_) => {
                            p.text(r.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(*label), theme::body(21.0), Tokens::text_strong());
                        }
                    }
                    if resp.clicked() {
                        pressed = Some(*key);
                    }
                    x += w + GAP;
                }
                y += KEY.y + GAP;
            }
        });
        match pressed {
            Some(Pad::Char(c)) => self.type_numeric(c),
            Some(Pad::Back) => self.key(egui::Key::Backspace, egui::Modifiers::NONE),
            Some(Pad::Close) => {
                self.numeric = None;
                self.numeric_fresh = false;
                self.numpad_at = None;
            }
            Some(Pad::Done) => {
                self.key(egui::Key::Enter, egui::Modifiers::NONE);
                self.numpad_at = None;
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Pos2, Rect, pos2, vec2};
    use serde_json::json;

    use super::Workspace;

    #[test]
    fn the_pad_has_digits_point_minus_back_close_and_done() {
        assert_eq!(super::ROWS.iter().map(|r| r.len()).sum::<usize>(), 15);
        assert!(super::ROWS.iter().all(|r| r.iter().map(|k| k.2).sum::<f32>() == 4.0), "four cells a row");
    }

    fn frame(ctx: &egui::Context, ws: &mut Workspace, events: Vec<egui::Event>) {
        let input =
            egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 900.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| ws.ui(ui, None));
        out.textures_delta.clear();
    }

    fn tap(ctx: &egui::Context, ws: &mut Workspace, at: Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, ws, vec![egui::Event::PointerMoved(at)]);
        frame(ctx, ws, vec![button(true)]);
        frame(ctx, ws, vec![button(false)]);
        frame(ctx, ws, Vec::new());
    }

    /// A tap on a picked circle's diameter opens the value field and, on a touch screen, the numpad beside it, as a
    /// tool's value pill does: with no keyboard there was no way to type the number (#84).
    #[test]
    fn a_dimension_tapped_opens_the_numpad() {
        let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "N");
        let mut ws = Workspace::new(session, crate::settings::Settings::default());
        ws.session.execute("sketch", &json!({"plane": "xy"})).unwrap();
        ws.session.execute("circle", &json!({"center": [0, 0], "radius": 20})).unwrap();
        let sk = ws.session.active_sketch.unwrap().0;
        ws.session.execute("select", &json!({"picks": [{"kind": "sketch_entity", "sketch": sk, "entity": 0}]})).unwrap();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&ctx, &mut ws, Vec::new());
        }
        let label = ws.entity_dims().first().map(|d| d.label).expect("the circle's diameter");
        tap(&ctx, &mut ws, label);
        assert!(ws.dim_edit.is_some(), "the dimension is being edited");
        assert_eq!(ws.numeric.as_deref(), Some(""));
        assert!(ws.numpad_at.is_some(), "the numpad has a place beside the value");
    }
}

/// The backspace key drawn: a tag pointing left with a cross in it.
fn backspace(p: &egui::Painter, c: Pos2, ink: Color32) {
    let s = Stroke::new(1.6, ink);
    let pts = vec![
        c + vec2(-11.0, 0.0),
        c + vec2(-5.0, -7.0),
        c + vec2(10.0, -7.0),
        c + vec2(10.0, 7.0),
        c + vec2(-5.0, 7.0),
        c + vec2(-11.0, 0.0),
    ];
    p.add(egui::Shape::line(pts, s));
    p.line_segment([c + vec2(-1.0, -3.5), c + vec2(6.0, 3.5)], s);
    p.line_segment([c + vec2(-1.0, 3.5), c + vec2(6.0, -3.5)], s);
}
