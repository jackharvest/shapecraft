//! The right column's popovers (Display Modes, Snap, Units & Grid, View), drawn like the reference's: a dark
//! rounded panel beside its button, section headers, check-marked choices and blue switches.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::theme::{self, Tokens, radius};

#[derive(Clone)]
pub enum Row<'a> {
    Header(&'a str),
    /// A choice with a check mark when it's the current one.
    Choice {
        label: &'a str,
        checked: bool,
        enabled: bool,
    },
    /// A label with a switch on the right.
    Switch {
        label: &'a str,
        on: bool,
    },
    /// A full-width grey button.
    Button(&'a str),
    /// Small grey help text.
    Tip(&'a str),
    Separator,
    /// A segmented control; the second value is the selected segment.
    Segments(&'a [&'a str], usize),
    /// A Saved Views slot: bright when it holds a view (click to go there); hovering an empty one offers Save.
    Saved {
        label: &'a str,
        saved: bool,
    },
    /// A slider between two labels, the value shown in blue between them and the range's ends under the track.
    Slider {
        left: &'a str,
        right: &'a str,
        value: &'a std::cell::Cell<f64>,
        min: f64,
        max: f64,
    },
    /// A label with a pop-up button on the right (as Zebra Analysis ▸ Direction); the segment is the option picked.
    Select {
        label: &'a str,
        options: &'a [&'a str],
        current: usize,
    },
    /// A label over a slider with its value ("1.0x") at the right end (Surface Analysis ▸ Scale).
    Scale {
        label: &'a str,
        value: &'a std::cell::Cell<f64>,
        min: f64,
        max: f64,
    },
    /// A bold title.
    Title(&'a str),
    /// Grey wrapped text over the given number of lines.
    Paragraph(&'a str, u8),
}

/// Where a popover sits: left of a button (arrow pointing right at it) or under a header (arrow pointing up).
#[derive(Clone, Copy)]
pub enum Place {
    Left(Pos2),
    Below(Pos2),
    /// Right of a left-column button (its right edge, at its middle), the arrow pointing back at it.
    Right(Pos2),
}

const W: f32 = 300.0;
/// How far the arrow sticks out towards the button.
const ARROW: f32 = 9.0;

fn height(r: &Row) -> f32 {
    match r {
        Row::Header(_) => 19.0,
        Row::Choice { .. } | Row::Switch { .. } | Row::Saved { .. } => 28.0,
        Row::Button(_) => 30.0,
        Row::Tip(_) => 20.0,
        Row::Separator => 12.0,
        Row::Segments(..) => 34.0,
        Row::Slider { .. } => 66.0,
        Row::Select { .. } => 30.0,
        Row::Scale { .. } => 56.0,
        Row::Title(_) => 24.0,
        Row::Paragraph(_, lines) => 15.0 * *lines as f32 + 6.0,
    }
}

/// Draws the popover left of `anchor` (its button's left edge, at the button's middle), centred on it with an
/// arrow pointing at it, kept inside the window `bounds` (as observed, L10: the macOS popover even overlaps the title
/// bar, 13 px from the window top). Returns the clicked row (and segment) and the popover's rect.
pub fn show(ui: &mut Ui, id: &str, anchor: Pos2, bounds: Rect, rows: &[Row]) -> (Option<(usize, usize)>, Rect) {
    show_at(ui, id, Place::Left(anchor), bounds, rows)
}

/// [`show`], placed left of a button or under a header (as History's Merge Modeling Steps, L16).
pub fn show_at(ui: &mut Ui, id: &str, place: Place, bounds: Rect, rows: &[Row]) -> (Option<(usize, usize)>, Rect) {
    // Measured (L10): 5 px above the first row, 4 below the last.
    let h: f32 = rows.iter().map(height).sum::<f32>() + 9.0;
    let (left, top) = match place {
        Place::Left(anchor) => {
            ((anchor.x - ARROW - W).max(bounds.left() + 4.0), (anchor.y - h / 2.0).min(bounds.bottom() - h - 4.0).max(bounds.top() + 13.0))
        }
        Place::Below(anchor) => ((anchor.x - W / 2.0).clamp(bounds.left() + 4.0, bounds.right() - W - 4.0), anchor.y + ARROW),
        Place::Right(anchor) => {
            ((anchor.x + ARROW).min(bounds.right() - W - 4.0), (anchor.y - h / 2.0).min(bounds.bottom() - h - 4.0).max(bounds.top() + 13.0))
        }
    };
    let mut clicked = None;
    // With nothing checked the names sit at the left edge; a check mark makes room for itself (as observed, 402/404).
    let any_checked = rows.iter().any(|r| matches!(r, Row::Choice { checked: true, .. }));
    let indent = if any_checked { 24.0 } else { 6.0 };
    let shown = egui::Area::new(ui.scope_id().with(id)).fixed_pos(pos2(left, top)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(W, h), Sense::hover());
        let p = ui.painter().clone();
        p.rect_filled(rect.translate(vec2(0.0, 4.0)), radius(11), Color32::from_black_alpha(70));
        // Measured (s23 display): #252525 with a black outer and a lighter inner edge.
        p.rect_filled(rect, radius(10), theme::c(0x252525));
        p.rect_stroke(rect, radius(10), Stroke::new(1.0, theme::c(0x070707)), StrokeKind::Outside);
        p.rect_stroke(rect, radius(10), Stroke::new(1.0, theme::c(0x555555)), StrokeKind::Inside);
        // The arrow at the button's height, or under the header (kept off the rounded corners).
        let tip = match place {
            Place::Left(anchor) => {
                let ay = anchor.y.clamp(rect.top() + 18.0, rect.bottom() - 18.0);
                [pos2(rect.right() - 1.0, ay - 10.0), pos2(rect.right() + ARROW - 1.0, ay), pos2(rect.right() - 1.0, ay + 10.0)]
            }
            Place::Right(anchor) => {
                let ay = anchor.y.clamp(rect.top() + 18.0, rect.bottom() - 18.0);
                [pos2(rect.left() + 1.0, ay - 10.0), pos2(rect.left() - ARROW + 1.0, ay), pos2(rect.left() + 1.0, ay + 10.0)]
            }
            Place::Below(anchor) => {
                let ax = anchor.x.clamp(rect.left() + 18.0, rect.right() - 18.0);
                [pos2(ax - 10.0, rect.top() + 1.0), pos2(ax, rect.top() - ARROW + 1.0), pos2(ax + 10.0, rect.top() + 1.0)]
            }
        };
        let pa = p.with_clip_rect(rect.expand(ARROW + 2.0));
        pa.add(egui::Shape::convex_polygon(tip.to_vec(), theme::c(0x252525), Stroke::NONE));
        pa.add(egui::Shape::line(tip.to_vec(), Stroke::new(1.0, theme::c(0x555555))));
        let mut y = rect.top() + 5.0;
        for (i, row) in rows.iter().enumerate() {
            let rh = height(row);
            let r = Rect::from_min_size(pos2(rect.left() + 8.0, y), vec2(W - 16.0, rh));
            match row {
                Row::Header(t) => {
                    p.text(
                        pos2(r.left() + 6.0, r.center().y),
                        Align2::LEFT_CENTER,
                        crate::i18n::tr_s(*t),
                        theme::strong(11.0),
                        Tokens::text_dim(),
                    );
                }
                Row::Choice { label, checked, enabled } => {
                    let resp = ui.interact(r, ui.scope_id().with((id, i)), Sense::click());
                    let hot = *enabled && resp.hovered();
                    // A grey highlight under the pointer (s23 display: #3A3A3A, light #D3D2D3).
                    if hot {
                        p.rect_filled(r, radius(5), theme::c(0x3a3a3a));
                    }
                    let col = if *enabled { Tokens::text() } else { Tokens::text_faint() };
                    if *checked {
                        p.text(pos2(r.left() + 6.0, r.center().y), Align2::LEFT_CENTER, "✓", theme::body(13.0), col);
                    }
                    p.text(pos2(r.left() + indent, r.center().y), Align2::LEFT_CENTER, crate::i18n::tr_s(*label), theme::body(13.5), col);
                    if *enabled && resp.clicked() {
                        clicked = Some((i, 0));
                    }
                }
                Row::Saved { label, saved } => {
                    let resp = ui.interact(r, ui.scope_id().with((id, i)), Sense::click());
                    if resp.hovered() {
                        p.rect_filled(r, radius(5), theme::c(0x3a3a3c));
                    }
                    let col = if *saved { Tokens::text_strong() } else { Tokens::text_faint() };
                    p.text(pos2(r.left() + 6.0, r.center().y), Align2::LEFT_CENTER, crate::i18n::tr_s(*label), theme::body(13.5), col);
                    if !saved && resp.hovered() {
                        let b = Rect::from_center_size(pos2(r.right() - 27.0, r.center().y), vec2(48.0, 20.0));
                        p.rect_filled(b, radius(5), theme::c(0x636366));
                        p.text(b.center(), Align2::CENTER_CENTER, crate::i18n::tr("Save"), theme::body(12.0), Color32::WHITE);
                        if resp.clicked() && resp.interact_pointer_pos().is_some_and(|q| b.contains(q)) {
                            clicked = Some((i, 1));
                        }
                    } else if *saved && resp.clicked() {
                        clicked = Some((i, 0));
                    }
                }
                Row::Switch { label, on } => {
                    let resp = ui.interact(r, ui.scope_id().with((id, i)), Sense::click());
                    p.text(
                        pos2(r.left() + 6.0, r.center().y),
                        Align2::LEFT_CENTER,
                        crate::i18n::tr_s(*label),
                        theme::body(13.5),
                        Tokens::text(),
                    );
                    let track = Rect::from_center_size(pos2(r.right() - 22.0, r.center().y), vec2(38.0, 22.0));
                    p.rect_filled(track, radius(11), if *on { theme::c(0x2f5bd8) } else { theme::c(0x5a5a5e) });
                    let knob = if *on { track.right() - 11.0 } else { track.left() + 11.0 };
                    p.circle_filled(pos2(knob, track.center().y), 9.0, theme::c(0xd8d8da));
                    if resp.clicked() {
                        clicked = Some((i, 0));
                    }
                }
                Row::Button(t) => {
                    let b = r.shrink2(vec2(0.0, 3.0));
                    let resp = ui.interact(b, ui.scope_id().with((id, i)), Sense::click());
                    p.rect_filled(b, radius(6), if resp.hovered() { theme::c(0x6a6a6e) } else { theme::c(0x58585c) });
                    p.text(b.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(*t), theme::body(13.0), Color32::WHITE);
                    if resp.clicked() {
                        clicked = Some((i, 0));
                    }
                }
                Row::Tip(t) => {
                    p.text(
                        pos2(r.left() + 6.0, r.center().y),
                        Align2::LEFT_CENTER,
                        crate::i18n::tr_s(*t),
                        theme::body(10.5),
                        Tokens::text_dim(),
                    );
                }
                Row::Separator => {
                    p.line_segment(
                        [pos2(r.left() + 4.0, r.center().y), pos2(r.right() - 4.0, r.center().y)],
                        Stroke::new(1.0, theme::c(0x434343)),
                    );
                }
                Row::Segments(labels, sel) => {
                    let b = r.shrink2(vec2(0.0, 4.0));
                    p.rect_filled(b, radius(7), theme::c(0x1f1f21));
                    let w = b.width() / labels.len().max(1) as f32;
                    for (k, l) in labels.iter().enumerate() {
                        let seg = Rect::from_min_size(pos2(b.left() + w * k as f32, b.top()), vec2(w, b.height())).shrink(2.0);
                        if k == *sel {
                            p.rect_filled(seg, radius(6), theme::c(0x5a5a5e));
                        }
                        p.text(seg.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(*l), theme::body(13.0), Tokens::text());
                        if ui.interact(seg, ui.scope_id().with((id, i, k)), Sense::click()).clicked() {
                            clicked = Some((i, k));
                        }
                    }
                }
                Row::Title(t) => {
                    p.text(
                        pos2(r.left() + 6.0, r.center().y),
                        Align2::LEFT_CENTER,
                        crate::i18n::tr_s(*t),
                        theme::strong(12.0),
                        Tokens::text_strong(),
                    );
                }
                Row::Paragraph(t, _) => {
                    let g = p.layout(crate::i18n::tr_s(t), theme::body(10.5), Tokens::text_dim(), r.width() - 10.0);
                    p.galley(pos2(r.left() + 6.0, r.top() + 2.0), g, Tokens::text_dim());
                }
                Row::Select { label, options, current } => {
                    p.text(
                        pos2(r.left() + 6.0, r.center().y),
                        Align2::LEFT_CENTER,
                        crate::i18n::tr_s(*label),
                        theme::body(13.5),
                        Tokens::text(),
                    );
                    let opts: Vec<String> = options.iter().map(|o| o.to_string()).collect();
                    if let Some(k) = pill_select(ui, &format!("{id}-{i}"), pos2(r.right() - 4.0, r.center().y), 92.0, &opts, Some(*current))
                    {
                        clicked = Some((i, k));
                    }
                }
                Row::Scale { label, value, min, max } => {
                    p.text(
                        pos2(r.left() + 6.0, r.top() + 12.0),
                        Align2::LEFT_CENTER,
                        crate::i18n::tr_s(*label),
                        theme::body(13.5),
                        Tokens::text(),
                    );
                    let v = value.get();
                    let ty = r.top() + 40.0;
                    p.text(pos2(r.right() - 4.0, ty), Align2::RIGHT_CENTER, format!("{v:.1}x"), theme::body(12.5), Tokens::text());
                    let track = Rect::from_min_max(pos2(r.left() + 6.0, ty - 2.0), pos2(r.right() - 48.0, ty + 2.0));
                    let t = ((v - min) / (max - min).max(1e-9)).clamp(0.0, 1.0) as f32;
                    let kx = track.left() + track.width() * t;
                    p.rect_filled(track, radius(2), theme::c(0x5a5a5e));
                    p.rect_filled(Rect::from_min_max(track.min, pos2(kx, track.bottom())), radius(2), theme::c(0x2f5bd8));
                    let resp = ui.interact(track.expand2(vec2(10.0, 12.0)), ui.scope_id().with((id, i)), Sense::click_and_drag());
                    p.circle_filled(pos2(kx, track.center().y), 9.0, theme::c(0xf2f2f4));
                    if (resp.dragged() || resp.clicked())
                        && let Some(q) = resp.interact_pointer_pos()
                    {
                        let f = ((q.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0) as f64;
                        value.set(((min + (max - min) * f) * 10.0).round() / 10.0);
                        clicked = Some((i, 0));
                    }
                }
                Row::Slider { left, right, value, min, max } => {
                    let v = value.get();
                    let ly = r.top() + 12.0;
                    p.text(pos2(r.left() + 6.0, ly), Align2::LEFT_CENTER, crate::i18n::tr_s(*left), theme::body(13.0), Tokens::text());
                    p.text(pos2(r.right() - 4.0, ly), Align2::RIGHT_CENTER, crate::i18n::tr_s(*right), theme::body(13.0), Tokens::text());
                    p.text(pos2(r.center().x - 10.0, ly), Align2::CENTER_CENTER, format!("{v:.0}"), theme::body(13.0), Tokens::accent());
                    let track = Rect::from_min_max(pos2(r.left() + 6.0, r.top() + 34.0), pos2(r.right() - 4.0, r.top() + 38.0));
                    let t = ((v - min) / (max - min).max(1e-9)).clamp(0.0, 1.0) as f32;
                    let kx = track.left() + track.width() * t;
                    p.rect_filled(track, radius(2), theme::c(0x5a5a5e));
                    p.rect_filled(Rect::from_min_max(track.min, pos2(kx, track.bottom())), radius(2), theme::c(0x2f5bd8));
                    let hit = track.expand2(vec2(10.0, 12.0));
                    let resp = ui.interact(hit, ui.scope_id().with((id, i)), Sense::click_and_drag());
                    p.circle_filled(pos2(kx, track.center().y), 9.0, theme::c(0xf2f2f4));
                    if (resp.dragged() || resp.clicked())
                        && let Some(q) = resp.interact_pointer_pos()
                    {
                        let f = ((q.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0) as f64;
                        value.set((min + (max - min) * f).round());
                        clicked = Some((i, 0));
                    }
                    let ty = r.top() + 54.0;
                    p.text(pos2(r.left() + 6.0, ty), Align2::LEFT_CENTER, format!("{min:.1}°"), theme::body(11.5), Tokens::text());
                    p.text(pos2(r.right() - 4.0, ty), Align2::RIGHT_CENTER, format!("{max:.1}°"), theme::body(11.5), Tokens::text());
                }
            }
            y += rh;
        }
    });
    (clicked, shown.response.rect)
}

/// A compact pop-up button like the reference's (grey pill, blue chevron square), right-aligned at `right_center`.
/// Clicking opens a check-marked list under it; returns the index picked.
pub fn pill_select(ui: &mut Ui, id: &str, right_center: Pos2, min_width: f32, options: &[String], current: Option<usize>) -> Option<usize> {
    let open_key = ui.scope_id().with((id, "pill-open"));
    let text = current.and_then(|i| options.get(i)).cloned().unwrap_or_default();
    let p = ui.painter().clone();
    let galley = p.layout_no_wrap(crate::i18n::tr_s(text), theme::body(12.0), Tokens::text_strong());
    let w = (galley.size().x + 32.0).max(min_width);
    let pill = Rect::from_min_max(pos2(right_center.x - w, right_center.y - 10.0), pos2(right_center.x, right_center.y + 10.0));
    let resp = ui.interact(pill, ui.scope_id().with((id, "pill")), Sense::click());
    p.rect_filled(pill, radius(5), if resp.hovered() { theme::c(0x636367) } else { theme::c(0x555559) });
    p.galley(pos2(pill.left() + 9.0, right_center.y - galley.size().y / 2.0), galley, Tokens::text_strong());
    let knob = Rect::from_center_size(pos2(pill.right() - 9.5, right_center.y), vec2(14.0, 15.0));
    p.rect_filled(knob, radius(4), Tokens::accent());
    crate::icons::paint(&p, Rect::from_center_size(knob.center(), vec2(8.0, 8.0)), crate::icons::Icon::ChevronDown, Color32::WHITE);
    let mut open: bool = ui.ctx().data(|d| d.get_temp(open_key)).unwrap_or(false);
    if resp.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(open_key, open));
        return None;
    }
    if !open {
        return None;
    }
    let row_h = 24.0;
    let list_w = options
        .iter()
        .map(|o| p.layout_no_wrap(crate::i18n::tr_s(o.clone()), theme::body(12.5), Tokens::text()).size().x)
        .fold(0.0, f32::max)
        + 44.0;
    let list_w = list_w.max(pill.width());
    let mut picked = None;
    let shown = egui::Area::new(ui.scope_id().with((id, "pill-list")))
        .fixed_pos(pos2(pill.right() - list_w, pill.bottom() + 2.0))
        .order(egui::Order::Tooltip)
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(list_w, row_h * options.len() as f32 + 10.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect.translate(vec2(0.0, 3.0)), radius(8), Color32::from_black_alpha(70));
            p.rect_filled(rect, radius(8), theme::c(0x2e2e30));
            p.rect_stroke(rect, radius(8), Stroke::new(1.0, theme::c(0x48484a)), StrokeKind::Inside);
            for (i, o) in options.iter().enumerate() {
                let r = Rect::from_min_size(pos2(rect.left() + 5.0, rect.top() + 5.0 + row_h * i as f32), vec2(list_w - 10.0, row_h));
                let resp = ui.interact(r, ui.scope_id().with((id, "pill-row", i)), Sense::click());
                if resp.hovered() {
                    p.rect_filled(r, radius(5), theme::c(0x2453c9));
                }
                if Some(i) == current {
                    p.text(pos2(r.left() + 7.0, r.center().y), Align2::LEFT_CENTER, "✓", theme::body(12.5), Tokens::text_strong());
                }
                p.text(
                    pos2(r.left() + 24.0, r.center().y),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(o),
                    theme::body(12.5),
                    Tokens::text_strong(),
                );
                if resp.clicked() {
                    picked = Some(i);
                }
            }
        });
    let outside = ui.input(|i| {
        i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|q| !shown.response.rect.contains(q) && !pill.contains(q))
    });
    if picked.is_some() || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.ctx().data_mut(|d| d.insert_temp(open_key, false));
    }
    picked.filter(|i| Some(*i) != current)
}

/// A small pull-down menu like the reference's: a check column, an optional icon, and the hovered row in blue.
/// Returns the clicked row and the menu's rect.
pub fn pull_down(
    ui: &mut Ui,
    id: &str,
    top_left: Pos2,
    width: f32,
    rows: &[(&str, Option<crate::icons::Icon>, bool)],
) -> (Option<usize>, Rect) {
    const ROW: f32 = 24.0;
    let checks = rows.iter().any(|r| r.2);
    let mut clicked = None;
    let shown = egui::Area::new(ui.scope_id().with(id)).fixed_pos(top_left).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(width, ROW * rows.len() as f32 + 10.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, radius(8), Tokens::banner());
        p.rect_stroke(rect, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
        for (i, (label, icon, checked)) in rows.iter().enumerate() {
            let row = Rect::from_min_size(rect.min + vec2(5.0, 5.0 + ROW * i as f32), vec2(width - 10.0, ROW));
            let resp = ui.interact(row, ui.scope_id().with((id, i)), Sense::click());
            let hot = resp.hovered();
            if hot {
                p.rect_filled(row, radius(5), theme::c(0x2453c9));
            }
            let mut x = row.left() + 8.0;
            if checks {
                if *checked {
                    p.text(pos2(x, row.center().y), Align2::LEFT_CENTER, "✓", theme::body(12.5), Tokens::text_strong());
                }
                x += 16.0;
            }
            if let Some(icon) = icon {
                crate::icons::paint(p, Rect::from_center_size(pos2(x + 7.0, row.center().y), vec2(13.0, 13.0)), *icon, Tokens::text());
                x += 20.0;
            }
            p.text(pos2(x, row.center().y), Align2::LEFT_CENTER, crate::i18n::tr_s(*label), theme::body(12.5), Tokens::text_strong());
            if resp.clicked() {
                clicked = Some(i);
            }
        }
    });
    (clicked, shown.response.rect)
}

/// A macOS-style menu (as the viewport's right-click menu, measured L11): dark see-through, 24 px rows with an
/// optional icon, 11 px separators (an empty label), a blue highlight. Kept inside `bounds`. Returns the clicked row
/// and the menu's rect.
pub fn native_menu(
    ui: &mut Ui,
    id: &str,
    top_left: Pos2,
    width: f32,
    bounds: Rect,
    rows: &[(&str, Option<crate::icons::Icon>)],
) -> (Option<usize>, Rect) {
    const ROW: f32 = 24.0;
    const SEP: f32 = 11.0;
    let height: f32 = rows.iter().map(|r| if r.0.is_empty() { SEP } else { ROW }).sum::<f32>() + 10.0;
    let origin = pos2(top_left.x.min(bounds.right() - width - 4.0), top_left.y.min(bounds.bottom() - height - 4.0));
    let mut clicked = None;
    let shown = egui::Area::new(ui.scope_id().with(id)).fixed_pos(origin).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let p = ui.painter();
        let shadow = egui::epaint::Shadow {
            offset: [0, 5],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(if theme::is_light() { 34 } else { 60 }),
        };
        p.add(shadow.as_shape(rect, radius(10)));
        p.rect_filled(rect, radius(10), theme::themed(Color32::from_rgba_unmultiplied(24, 24, 27, 236)));
        p.rect_stroke(rect, radius(10), Stroke::new(1.0, theme::c(0x030303)), StrokeKind::Outside);
        p.rect_stroke(
            rect,
            radius(10),
            Stroke::new(1.0, theme::themed(Color32::from_rgba_unmultiplied(110, 110, 114, 200))),
            StrokeKind::Inside,
        );
        let mut y = rect.top() + 5.0;
        for (i, (label, icon)) in rows.iter().enumerate() {
            if label.is_empty() {
                p.line_segment(
                    [pos2(rect.left() + 15.0, y + SEP / 2.0), pos2(rect.right() - 15.0, y + SEP / 2.0)],
                    Stroke::new(1.0, theme::c(0x3a3b44)),
                );
                y += SEP;
                continue;
            }
            let row = Rect::from_min_size(pos2(rect.left() + 5.0, y), vec2(width - 9.0, ROW));
            let resp = ui.interact(row, ui.scope_id().with((id, i)), Sense::click());
            if resp.hovered() {
                p.rect_filled(row, radius(5), theme::c(0x213cb0));
            }
            let col = if resp.hovered() { Color32::WHITE } else { Tokens::text() };
            let mut x = rect.left() + 15.0;
            if let Some(icon) = icon {
                crate::icons::paint(p, Rect::from_center_size(pos2(x + 5.0, row.center().y), vec2(13.0, 13.0)), *icon, col);
                x += 18.0;
            }
            p.text(pos2(x, row.center().y), Align2::LEFT_CENTER, crate::i18n::tr_s(*label), theme::body(13.5), col);
            if resp.clicked() {
                clicked = Some(i);
            }
            y += ROW;
        }
    });
    (clicked, shown.response.rect)
}
