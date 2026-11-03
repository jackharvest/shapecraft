//! Sketch constraints in the UI: which constraint chips apply to the selection, their ⇧ shortcuts, and the small
//! badges drawn at constrained elements (click one to select it, Delete removes it).

use std::collections::HashMap;

use egui::{Color32, Pos2, Rect, Shape, Stroke, pos2, vec2};
use glam::DVec2;
use serde_json::json;
use shapecraft_doc::constraints::{self, Constraint, Element};
use shapecraft_doc::{ItemId, Pick, SketchEntity};

use super::Workspace;
use crate::theme::{self, Tokens, radius};

/// The column's constraint chips: label, shortcut, and the `constrain` kind (or another command).
pub const CHIPS: [(&str, &str, &str); 12] = [
    ("Parallel", "⇧A", "parallel"),
    ("Perpendicular", "⇧P", "perpendicular"),
    ("Tangent", "⇧T", "tangent"),
    ("Coincident", "⇧N", "coincident"),
    ("Midpoint", "⇧M", "midpoint"),
    ("Concentric", "⇧C", "concentric"),
    ("Horizontal/Vertical", "⇧V", "horizontal_vertical"),
    ("Equal", "⇧E", "equal"),
    ("Symmetry", "⇧S", "symmetry"),
    ("Disconnect", "", "disconnect"),
    ("Lock", "⇧L", "lock"),
    ("Make Construction", "", "make_construction"),
];

impl Workspace {
    /// The selected elements of the active sketch, in selection order.
    fn picked_elements(&self) -> Vec<(u32, SketchEntity)> {
        let Some(sk) = self.session.active_sketch() else { return Vec::new() };
        self.session
            .selection
            .iter()
            .filter_map(|p| match *p {
                Pick::SketchEntity { sketch, entity } if sketch == sk.id => sk.entities.get(entity as usize).map(|e| (entity, e.clone())),
                _ => None,
            })
            .collect()
    }

    /// Every picked element is locked (Lock then reads Unlock).
    pub(super) fn picked_all_locked(&self) -> bool {
        let picked = self.picked_elements();
        let Some(sk) = self.session.active_sketch() else { return false };
        !picked.is_empty() && picked.iter().all(|(e, _)| sk.constraints.contains(&Constraint::Lock { entity: *e }))
    }

    /// Whether a chip applies to what's selected (the reference greys the rest out).
    pub(super) fn constraint_applies(&self, kind: &str) -> bool {
        let picked = self.picked_elements();
        let n = picked.len();
        let seg = |e: &SketchEntity| !constraints::segments(e).is_empty();
        let curve = |e: &SketchEntity| matches!(e, SketchEntity::Circle { .. } | SketchEntity::Arc { .. });
        match kind {
            "disconnect" | "lock" | "make_construction" => n >= 1,
            "horizontal_vertical" => n >= 1 && picked.iter().all(|(_, e)| matches!(e, SketchEntity::Line { .. })),
            "parallel" | "perpendicular" => n == 2 && picked.iter().all(|(_, e)| seg(e)),
            "equal" => n == 2 && (picked.iter().all(|(_, e)| seg(e)) || picked.iter().all(|(_, e)| curve(e))),
            "concentric" => n == 2 && picked.iter().all(|(_, e)| curve(e)),
            "tangent" => n == 2 && picked.iter().any(|(_, e)| curve(e)) && picked.iter().all(|(_, e)| curve(e) || seg(e)),
            "coincident" => n == 2,
            "midpoint" => n == 2 && seg(&picked[1].1),
            "symmetry" => n == 3 && matches!(picked[2].1, SketchEntity::Line { .. }),
            _ => false,
        }
    }

    /// Runs a chip (or its shortcut). Horizontal/Vertical applies to each selected line.
    pub(super) fn apply_constraint(&mut self, kind: &str) {
        if !self.constraint_applies(kind) {
            self.toast("Select the sketch elements to constrain first");
            return;
        }
        match kind {
            "disconnect" => {
                let _ = self.run("disconnect", json!({}));
            }
            "make_construction" => {
                let _ = self.run("make_construction", json!({}));
            }
            "lock" if self.picked_all_locked() => {
                // Unlock: drop the picked elements' locks (from the end, so the indices hold).
                let picked: Vec<u32> = self.picked_elements().into_iter().map(|(e, _)| e).collect();
                let Some(sk) = self.session.active_sketch() else { return };
                let mut locks: Vec<usize> = sk
                    .constraints
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| matches!(c, Constraint::Lock { entity } if picked.contains(entity)))
                    .map(|(i, _)| i)
                    .collect();
                locks.reverse();
                for i in locks {
                    let _ = self.run("delete_constraint", json!({ "index": i }));
                }
            }
            "lock" => {
                let picked: Vec<u32> = self.picked_elements().into_iter().map(|(e, _)| e).collect();
                let _ = self.run("constrain", json!({ "kind": "lock", "entities": picked }));
            }
            "horizontal_vertical" => {
                for (e, _) in self.picked_elements() {
                    let _ = self.run("constrain", json!({ "kind": kind, "entities": [e] }));
                }
            }
            _ => {
                // Constraint Settings ▸ Anchored Sketch Entity: the anchor goes first (a symmetry's axis stays last).
                let mut order: Vec<u32> = self.picked_elements().into_iter().map(|(e, _)| e).collect();
                if self.settings.anchor_last {
                    let n = if kind == "symmetry" { order.len().saturating_sub(1) } else { order.len() };
                    order[..n].reverse();
                }
                let _ = self.run("constrain", json!({ "kind": kind, "entities": order }));
            }
        }
    }

    /// Where each constraint's badge sits on screen: (rect, sketch, index, constraint).
    pub(super) fn constraint_badges(&self) -> Vec<(Rect, ItemId, usize, Constraint)> {
        let Some(sk) = self.session.active_sketch() else { return Vec::new() };
        let to_screen = |p: DVec2| self.to_screen(sk.plane.to_world(p));
        let mut used: HashMap<(i32, i32), u32> = HashMap::new();
        let mut out = Vec::new();
        // Constraint Settings ▸ Always Show Constraints off (the default): only those of the selected elements.
        let picked: Vec<u32> = self.picked_elements().into_iter().map(|(e, _)| e).collect();
        let all = self.settings.always_show_constraints;
        // Driving dimensions show as dimensions, not badges.
        for (i, c) in sk.constraints.iter().enumerate().filter(|(_, c)| {
            !matches!(
                c,
                Constraint::Distance { .. } | Constraint::Radius { .. } | Constraint::Angle { .. } | Constraint::PointDistance { .. }
            )
        }) {
            if !all && self.selected_constraint != Some((sk.id, i)) && !c.elements().iter().any(|el| picked.contains(&el.entity())) {
                continue;
            }
            // The badge goes by the element that moved to meet the constraint (the last one; the origin never moves).
            let Some(el) = c.elements().into_iter().rev().find(|el| !matches!(el, Element::Origin { .. })) else { continue };
            let Some(e) = sk.entities.get(el.entity() as usize) else { continue };
            let anchor = match el {
                Element::Point { index, .. } => constraints::points(e).get(index as usize).copied(),
                Element::Segment { index, .. } => constraints::segments(e).get(index as usize).map(|s| (s.0 + s.1) / 2.0),
                Element::Curve { .. } => constraints::curve(e).map(|(c, r)| c + DVec2::new(0.0, r)),
                Element::Origin { .. } => None,
            };
            let Some(at) = anchor.and_then(to_screen) else { continue };
            let key = ((at.x / 12.0) as i32, (at.y / 12.0) as i32);
            let slot = used.entry(key).or_insert(0);
            let r = Rect::from_center_size(at + vec2(12.0 + *slot as f32 * 19.0, 16.0), vec2(17.0, 17.0));
            *slot += 1;
            out.push((r, sk.id, i, c.clone()));
        }
        out
    }

    pub(super) fn paint_constraint_badges(&self, painter: &egui::Painter, hover: Option<Pos2>) {
        for (r, sketch, i, c) in self.constraint_badges() {
            let selected = self.selected_constraint == Some((sketch, i));
            let hot = hover.is_some_and(|h| r.contains(h));
            let fill = if selected {
                Tokens::accent()
            } else if hot {
                theme::c(0x4a4a4e)
            } else {
                theme::c(0x2c2c2f)
            };
            painter.rect_filled(r, radius(4), fill);
            painter.rect_stroke(r, radius(4), Stroke::new(1.0, theme::c(0x5a5a5e)), egui::StrokeKind::Inside);
            glyph(painter, r.shrink(4.0), &c);
        }
    }
}

/// A small drawing of the constraint's kind.
fn glyph(p: &egui::Painter, r: Rect, c: &Constraint) {
    let s = Stroke::new(1.3, Color32::WHITE);
    let (l, t, rt, b, cx, cy) = (r.left(), r.top(), r.right(), r.bottom(), r.center().x, r.center().y);
    match c {
        Constraint::Distance { .. } | Constraint::Radius { .. } | Constraint::Angle { .. } | Constraint::PointDistance { .. } => {}
        Constraint::Parallel { .. } => {
            p.line_segment([pos2(l + 2.0, b), pos2(cx, t)], s);
            p.line_segment([pos2(cx, b), pos2(rt - 2.0, t)], s);
        }
        Constraint::Perpendicular { .. } => {
            p.line_segment([pos2(l, b), pos2(rt, b)], s);
            p.line_segment([pos2(cx, b), pos2(cx, t)], s);
        }
        Constraint::Horizontal { .. } => {
            p.line_segment([pos2(l, cy), pos2(rt, cy)], s);
        }
        Constraint::Vertical { .. } => {
            p.line_segment([pos2(cx, t), pos2(cx, b)], s);
        }
        Constraint::Equal { .. } => {
            p.line_segment([pos2(l, cy - 2.5), pos2(rt, cy - 2.5)], s);
            p.line_segment([pos2(l, cy + 2.5), pos2(rt, cy + 2.5)], s);
        }
        Constraint::Coincident { .. } => {
            p.circle_filled(r.center(), 3.0, Color32::WHITE);
        }
        Constraint::Midpoint { .. } => {
            p.line_segment([pos2(l, cy), pos2(rt, cy)], s);
            p.add(Shape::convex_polygon(
                vec![pos2(cx, cy - 4.0), pos2(cx + 3.5, cy + 1.0), pos2(cx - 3.5, cy + 1.0)],
                Color32::WHITE,
                Stroke::NONE,
            ));
        }
        Constraint::Concentric { .. } => {
            p.circle_stroke(r.center(), 4.5, s);
            p.circle_stroke(r.center(), 2.0, s);
        }
        Constraint::Tangent { .. } => {
            p.circle_stroke(pos2(cx, cy + 1.5), 3.5, s);
            p.line_segment([pos2(l, cy - 2.0), pos2(rt, cy - 2.0)], s);
        }
        Constraint::Symmetric { .. } => {
            p.line_segment([pos2(cx, t), pos2(cx, b)], Stroke::new(1.0, Tokens::text_dim()));
            p.add(Shape::convex_polygon(
                vec![pos2(l, cy), pos2(cx - 1.5, cy - 3.5), pos2(cx - 1.5, cy + 3.5)],
                Color32::WHITE,
                Stroke::NONE,
            ));
            p.add(Shape::convex_polygon(
                vec![pos2(rt, cy), pos2(cx + 1.5, cy - 3.5), pos2(cx + 1.5, cy + 3.5)],
                Color32::WHITE,
                Stroke::NONE,
            ));
        }
        Constraint::Lock { .. } => {
            let body = Rect::from_min_max(pos2(l + 1.0, cy - 0.5), pos2(rt - 1.0, b));
            p.rect_filled(body, radius(1), Color32::WHITE);
            p.add(Shape::line(
                (0..=8)
                    .map(|k| {
                        let a = std::f32::consts::PI * k as f32 / 8.0;
                        pos2(cx + 2.8 * a.cos(), cy - 0.5 - 3.5 * a.sin())
                    })
                    .collect(),
                s,
            ));
        }
    }
}
