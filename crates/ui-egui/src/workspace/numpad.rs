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
    /// Whether the numpad shows: a touch screen without a keyboard, a value being entered.
    pub(crate) fn numpad_open(&self) -> bool {
        !crate::tablet::shows_shortcuts()
            && self.numeric.is_some()
            && self.numpad_at.is_some()
            && (self.tool.takes_numbers() || self.dim_edit.is_some())
    }

    pub(super) fn numpad(&mut self, ui: &Ui) {
        if !self.numpad_open() {
            if self.numeric.is_none() {
                self.numpad_at = None;
            }
            return;
        }
        let Some(at) = self.numpad_at else { return };
        let size = vec2(PAD * 2.0 + KEY.x * 4.0 + GAP * 3.0, PAD * 2.0 + 40.0 + GAP + KEY.y * 4.0 + GAP * 3.0);
        // Beside the pill, to its right where there's room, else its left; kept inside the view.
        let vp = self.viewport_rect;
        let mut x = at.x + 24.0;
        if x + size.x > vp.right() - 8.0 {
            x = at.x - 24.0 - size.x;
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
            let g = p.layout_no_wrap(if shown.is_empty() { "0".into() } else { shown.clone() }, theme::strong(20.0), Tokens::text_strong());
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
                            p.text(r.center(), Align2::CENTER_CENTER, *label, theme::body(21.0), Tokens::text_strong());
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
    #[test]
    fn the_pad_has_digits_point_minus_back_close_and_done() {
        assert_eq!(super::ROWS.iter().map(|r| r.len()).sum::<usize>(), 15);
        assert!(super::ROWS.iter().all(|r| r.iter().map(|k| k.2).sum::<f32>() == 4.0), "four cells a row");
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
