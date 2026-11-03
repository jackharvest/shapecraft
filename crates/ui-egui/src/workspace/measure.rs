//! Measure, as observed (L24): a card at the foot of the right-hand column listing what the selection measures
//! (each can be pinned), pinned measurements drawn in the view with a leader, and the Point-to-Point Distance and
//! 3-Point Angle modes picking points on the model.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::Pick;

use super::Workspace;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

/// One measurement: its kind's icon, the value, what it is of, and where its label hangs in the view.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub icon: Icon,
    pub value: String,
    pub sub: String,
    pub anchor: DVec3,
}

/// The current mode: 0 objects, 1 point-to-point, 2 three-point angle.
fn mode(ws: &Workspace) -> usize {
    ws.variant("Measure mode")
}

fn vec3(v: &Value) -> Option<DVec3> {
    let a = v.as_array()?;
    Some(DVec3::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?))
}

fn plural(n: usize, one: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") }
}

/// What the selection (or the picked points) measures.
pub fn rows(ws: &mut Workspace) -> Vec<Measured> {
    let units = ws.session.doc.units;
    let k = units.scale();
    let suffix = units.suffix();
    let mut out = Vec::new();
    match mode(ws) {
        1 => {
            if let [a, b] = ws.measure_points[..] {
                out.push(Measured {
                    icon: Icon::MDistance,
                    value: shapecraft_doc::format_length(a.distance(b), units),
                    sub: "Distance - 2 points".into(),
                    anchor: (a + b) / 2.0,
                });
                deltas(ws, a, b, &mut out);
            }
            return out;
        }
        2 => {
            if let [a, o, b] = ws.measure_points[..] {
                let (u, v) = ((a - o).normalize_or_zero(), (b - o).normalize_or_zero());
                let deg = u.dot(v).clamp(-1.0, 1.0).acos().to_degrees();
                out.push(Measured { icon: Icon::MAngle, value: format!("{deg:.2}°"), sub: "Angle - 3 points".into(), anchor: o });
            }
            return out;
        }
        _ => {}
    }
    let key = (ws.session.selection.clone(), ws.session.revision);
    let fresh = ws.measure_cache.as_ref().is_some_and(|(sel, h, _)| *sel == key.0 && *h == key.1);
    if !fresh {
        let result = ws.session.execute("measure", &json!({})).map(|o| o.value).map_err(|e| e.to_string());
        ws.measure_cache = Some((key.0, key.1, result));
    }
    let Some((_, _, Ok(value))) = ws.measure_cache.clone() else { return out };
    let items = value["items"].as_array().cloned().unwrap_or_default();
    // Faces and edges add up; bodies each give their surface area and volume.
    let (mut area, mut faces, mut length, mut edges) = (0.0, 0usize, 0.0, 0usize);
    let (mut face_at, mut edge_at) = (None, None);
    for item in &items {
        let center = vec3(&item["center"]).unwrap_or(DVec3::ZERO);
        let pick: Option<Pick> = serde_json::from_value(item["pick"].clone()).ok();
        match pick {
            Some(Pick::Body { body }) => {
                let name = ws.session.doc.item_name(body).unwrap_or("Body").to_string();
                if let Some(a) = item["area_mm2"].as_f64() {
                    out.push(Measured {
                        icon: Icon::MArea,
                        value: format!("{:.4} {suffix}²", a / (k * k)),
                        sub: format!("Surface Area - {name}"),
                        anchor: center,
                    });
                }
                if let Some(v) = item["volume_mm3"].as_f64() {
                    out.push(Measured {
                        icon: Icon::MVolume,
                        value: format!("{:.4} {suffix}³", v / (k * k * k)),
                        sub: format!("Volume - {name}"),
                        anchor: center,
                    });
                }
            }
            Some(Pick::Face { .. }) => {
                area += item["area_mm2"].as_f64().unwrap_or(0.0);
                faces += 1;
                face_at.get_or_insert(center);
            }
            Some(Pick::Edge { .. }) => {
                length += item["length_mm"].as_f64().unwrap_or(0.0);
                edges += 1;
                edge_at.get_or_insert(center);
            }
            _ => {}
        }
    }
    // Ours: where picked bodies overlap.
    if let Some(clashes) = value["interference"].as_array() {
        if clashes.is_empty() {
            let at = items.first().and_then(|i| vec3(&i["center"])).unwrap_or(DVec3::ZERO);
            out.push(Measured { icon: Icon::MVolume, value: "None".into(), sub: "Interference".into(), anchor: at });
        }
        for c in clashes {
            let name =
                |k: &str| c[k].as_u64().and_then(|i| ws.session.doc.item_name(shapecraft_doc::ItemId(i))).unwrap_or("Body").to_string();
            out.push(Measured {
                icon: Icon::MVolume,
                value: format!("{:.4} {suffix}³", c["volume_mm3"].as_f64().unwrap_or(0.0) / (k * k * k)),
                sub: format!("Interference - {} / {}", name("a"), name("b")),
                anchor: vec3(&c["center"]).unwrap_or(DVec3::ZERO),
            });
        }
    }
    if let Some(at) = face_at {
        out.push(Measured {
            icon: Icon::MArea,
            value: format!("{:.4} {suffix}²", area / (k * k)),
            sub: format!("Surface Area - {}", plural(faces, "face")),
            anchor: at,
        });
    }
    if let Some(at) = edge_at {
        out.push(Measured {
            icon: Icon::MLength,
            value: shapecraft_doc::format_length(length, units),
            sub: format!("Length - {}", plural(edges, "edge")),
            anchor: at,
        });
    }
    if let Some(d) = value["distance"].as_object()
        && let (Some(mm), Some(a), Some(b)) =
            (d.get("mm").and_then(Value::as_f64), d.get("from").and_then(vec3), d.get("to").and_then(vec3))
    {
        out.push(Measured {
            icon: Icon::MDistance,
            value: shapecraft_doc::format_length(mm, units),
            sub: "Minimum Distance".into(),
            anchor: (a + b) / 2.0,
        });
        deltas(ws, a, b, &mut out);
    }
    if let Some(d) = value["max_distance"].as_object()
        && let (Some(mm), Some(a), Some(b)) =
            (d.get("mm").and_then(Value::as_f64), d.get("from").and_then(vec3), d.get("to").and_then(vec3))
    {
        out.push(Measured {
            icon: Icon::MDistance,
            value: shapecraft_doc::format_length(mm, units),
            sub: "Maximum Distance".into(),
            anchor: (a + b) / 2.0,
        });
    }
    if let Some(deg) = value["angle_deg"].as_f64() {
        let at = face_at.or(edge_at).unwrap_or(DVec3::ZERO);
        out.push(Measured { icon: Icon::MAngle, value: format!("{deg:.2}°"), sub: "Angle".into(), anchor: at });
    }
    out
}

/// Include X, Y, Z Deltas: the gap along each axis too.
fn deltas(ws: &Workspace, a: DVec3, b: DVec3, out: &mut Vec<Measured>) {
    if !ws.measure_deltas {
        return;
    }
    let units = ws.session.doc.units;
    let d = b - a;
    for (axis, v) in [("X", d.x), ("Y", d.y), ("Z", d.z)] {
        out.push(Measured {
            icon: Icon::MDistance,
            value: shapecraft_doc::format_length(v.abs(), units),
            sub: format!("{axis} Delta"),
            anchor: (a + b) / 2.0,
        });
    }
}

/// The Measure card at the foot of the right-hand column.
pub fn card(ws: &mut Workspace, ui: &mut Ui) {
    let view = ws.view_rect;
    let h = (view.height() - 120.0).clamp(240.0, 540.0);
    let r = Rect::from_min_max(pos2(view.right() - 8.0 - 300.0, view.bottom() - 8.0 - h), pos2(view.right() - 8.0, view.bottom() - 8.0));
    super::panels::panel(ui, r, ws.glass());
    let p = ui.painter().clone();
    // Close (x) at the top left.
    let close = pos2(r.left() + 14.0, r.top() + 13.0);
    let close_resp = ui.interact(Rect::from_center_size(close, vec2(16.0, 16.0)), ui.scope_id().with("measure-close"), Sense::click());
    p.circle_filled(close, 6.0, if close_resp.hovered() { Tokens::text() } else { Tokens::text_dim() });
    for d in [vec2(2.3, 2.3), vec2(2.3, -2.3)] {
        p.line_segment([close - d, close + d], Stroke::new(1.3, Tokens::panel()));
    }
    if close_resp.clicked() {
        ws.measuring = false;
    }
    p.text(pos2(r.left() + 16.0, r.top() + 46.0), Align2::LEFT_CENTER, "Measure", theme::strong(13.0), Tokens::text_strong());
    let line = |y: f32| p.line_segment([pos2(r.left() + 1.0, y), pos2(r.right() - 1.0, y)], Stroke::new(1.0, theme::c(0x3e3d3e)));
    line(r.top() + 64.5);
    p.text(pos2(r.left() + 16.0, r.top() + 95.0), Align2::LEFT_CENTER, "Pin to Always Show", theme::body(10.5), Tokens::text_faint());
    p.text(pos2(r.left() + 16.0, r.top() + 126.0), Align2::LEFT_CENTER, "Include X, Y, Z Deltas", theme::body(13.0), Tokens::text());
    let track = Rect::from_center_size(pos2(r.right() - 34.0, r.top() + 126.0), vec2(38.0, 22.0));
    if ui.interact(track, ui.scope_id().with("measure-deltas"), Sense::click()).clicked() {
        ws.measure_deltas = !ws.measure_deltas;
    }
    p.rect_filled(track, radius(11), if ws.measure_deltas { theme::c(0x2f5bd8) } else { theme::c(0x5a5a5e) });
    p.circle_filled(
        pos2(if ws.measure_deltas { track.right() - 11.0 } else { track.left() + 11.0 }, track.center().y),
        9.0,
        theme::c(0xd8d8da),
    );
    let found = rows(ws);
    let mut y = r.top() + 160.0;
    let unpinned: Vec<Measured> = found.iter().filter(|m| !ws.pinned.contains(m)).cloned().collect();
    if found.is_empty() {
        let hint = match mode(ws) {
            1 => "Pick two points on the model.",
            2 => "Pick three points: an end, the corner, the other end.",
            _ => "Select any item to measure.",
        };
        p.text(pos2(r.left() + 16.0, y), Align2::LEFT_CENTER, hint, theme::body(11.5), Tokens::text_dim());
        y += 30.0;
    } else if unpinned.is_empty() {
        let g =
            p.layout("Measurements for the selection are already pinned.".into(), theme::body(12.5), Tokens::text_dim(), r.width() - 40.0);
        let gh = g.size().y;
        p.galley(pos2(r.left() + 16.0, y - 8.0), g, Tokens::text_dim());
        y += gh + 14.0;
    }
    for (i, m) in unpinned.iter().enumerate() {
        measured_row(&p, r, y, m);
        let pin = Rect::from_center_size(pos2(r.right() - 26.0, y), vec2(26.0, 26.0));
        let resp = ui.interact(pin, ui.scope_id().with(("measure-pin", i)), Sense::click());
        icons::paint(&p, pin.shrink(3.0), Icon::Pin, if resp.hovered() { Tokens::text_strong() } else { Tokens::text_dim() });
        if resp.clicked() {
            ws.pinned.push(m.clone());
        }
        y += 46.0;
    }
    line(y + 2.5);
    p.text(pos2(r.left() + 16.0, y + 34.0), Align2::LEFT_CENTER, "Pinned", theme::body(10.5), Tokens::text_faint());
    y += 66.0;
    if ws.pinned.is_empty() {
        let g = p.layout("Pin measurements to have them always visible.".into(), theme::body(12.5), Tokens::text_dim(), r.width() - 80.0);
        p.galley(pos2(r.left() + 16.0, y - 8.0), g, Tokens::text_dim());
    }
    let mut remove = None;
    for (i, m) in ws.pinned.iter().enumerate() {
        if y > r.bottom() - 20.0 {
            break;
        }
        let row = Rect::from_min_max(pos2(r.left() + 8.0, y - 22.0), pos2(r.right() - 8.0, y + 22.0));
        let resp = ui.interact(row, ui.scope_id().with(("measure-pinned", i)), Sense::hover());
        if resp.hovered() {
            p.rect_filled(row, radius(6), theme::c(0x343336));
            let bin = Rect::from_center_size(pos2(r.right() - 26.0, y), vec2(22.0, 22.0));
            let bin_resp = ui.interact(bin, ui.scope_id().with(("measure-unpin", i)), Sense::click());
            icons::paint(&p, bin.shrink(3.0), Icon::Delete, if bin_resp.hovered() { Tokens::text_strong() } else { Tokens::text_dim() });
            if bin_resp.clicked() {
                remove = Some(i);
            }
        }
        measured_row(&p, r, y, m);
        y += 46.0;
    }
    if let Some(i) = remove {
        ws.pinned.remove(i);
    }
}

fn measured_row(p: &egui::Painter, r: Rect, y: f32, m: &Measured) {
    icons::paint(p, Rect::from_center_size(pos2(r.left() + 22.0, y), vec2(18.0, 18.0)), m.icon, Tokens::text_dim());
    p.text(pos2(r.left() + 40.0, y - 7.0), Align2::LEFT_CENTER, &m.value, theme::body(13.5), Tokens::text_strong());
    p.text(pos2(r.left() + 40.0, y + 9.0), Align2::LEFT_CENTER, &m.sub, theme::body(10.5), Tokens::text_dim());
}

/// Pinned measurements in the view: a dot on the model, a leader, the value in a dark box (as observed).
pub fn pinned_labels(ws: &Workspace, ui: &Ui) {
    if !ws.view_opts.pinned {
        return;
    }
    let p = ui.painter_at(ws.view_rect);
    for m in &ws.pinned {
        let Some(at) = ws.to_screen(m.anchor) else { continue };
        label(&p, at, m);
    }
    // The points picked in the point modes, joined.
    let pts: Vec<Pos2> = ws.measure_points.iter().filter_map(|q| ws.to_screen(*q)).collect();
    if ws.measuring && !pts.is_empty() {
        let col = Color32::WHITE;
        for w in pts.windows(2) {
            p.add(egui::Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.3, col), 6.0, 4.0));
        }
        for q in &pts {
            p.circle_filled(*q, 4.0, Tokens::accent());
            p.circle_stroke(*q, 4.0, Stroke::new(1.5, Color32::WHITE));
        }
    }
}

fn label(p: &egui::Painter, at: Pos2, m: &Measured) {
    let tip = at + vec2(45.0, -40.0);
    p.line_segment([at, tip], Stroke::new(1.2, Color32::WHITE));
    p.circle_filled(at, 3.0, Color32::WHITE);
    let g = p.layout_no_wrap(m.value.clone(), theme::mono(11.0), Color32::WHITE);
    let r = Rect::from_min_size(tip - vec2(0.0, 10.0), vec2(g.size().x + 26.0, 20.0));
    p.rect_filled(r, radius(3), theme::c(0x1c1c1e));
    p.rect_stroke(r, radius(3), Stroke::new(1.0, Color32::WHITE), StrokeKind::Inside);
    icons::paint(p, Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(11.0, 11.0)), m.icon, Color32::WHITE);
    p.galley(pos2(r.left() + 19.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
}
