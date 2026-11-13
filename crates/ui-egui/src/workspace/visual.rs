//! The Visualization sidebar, as observed: Material / Environment / Camera tabs, and a Properties view with a
//! color picker while something is selected. Material balls are shaded here per vertex (nothing bundled).

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use shapecraft_doc::{Appearance, ItemId, Pick};

use super::Workspace;
use crate::theme::{self, Tokens, radius};

/// Studio backdrops for Visualization (name, color, shadow strength), in the order observed (s13-018). Transparent
/// shows the default backdrop in the view (a capture keeps it see-through).
pub const ENVIRONMENTS: [(&str, u32, f32); 9] = [
    ("Default", 0x101010, 0.6),
    ("Transparent", 0x101010, 0.6),
    ("Colored Mood", 0xd9d5ce, 0.35),
    ("Gradient Mood", 0xf2f2f2, 0.3),
    ("Black and White Stage", 0xffffff, 0.3),
    ("Bright Exhibition Space", 0xe9e7e2, 0.3),
    ("Dark Exhibition Space", 0x1b1b1d, 0.55),
    ("Carousel 1", 0x232325, 0.55),
    ("Carousel 2", 0x2a2a2c, 0.55),
];

/// The environments whose backdrop color can be picked (s13-027).
const COLORED: [usize; 2] = [2, 3];

/// The backdrop color: the environment's, or the one picked for it.
pub fn backdrop(v: &shapecraft_doc::VisualSettings) -> u32 {
    // A custom environment: its panorama's average color, as shown (the floor's tone and the depth of field's
    // backdrop come from it).
    if let Some(p) = v.panorama() {
        let (avg, e) = (p.pixels.average(), p.pixels.exposure());
        let b = |x: f32| {
            let x = (x * e).clamp(0.0, 1.0);
            let s = if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
            (s * 255.0).round() as u32
        };
        return (b(avg[0]) << 16) | (b(avg[1]) << 8) | b(avg[2]);
    }
    let env = v.environment.min(ENVIRONMENTS.len() - 1);
    match v.background(env) {
        Some(c) if COLORED.contains(&env) => {
            let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
            (b(c[0]) << 16) | (b(c[1]) << 8) | b(c[2])
        }
        _ => ENVIRONMENTS[env].1,
    }
}

/// How dark the shadow is on the environment's floor.
pub fn shadow(v: &shapecraft_doc::VisualSettings) -> f32 {
    if v.panorama().is_some() {
        return 0.5;
    }
    ENVIRONMENTS[v.environment.min(ENVIRONMENTS.len() - 1)].2
}

/// A labelled slider as the reference draws them (s13-010, s13-019): bold label, a thin track filled blue up to a
/// white knob, the value in a small box on the right (click it to type one).
pub(super) struct Slider<'a, K: std::hash::Hash + std::fmt::Debug> {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub label: &'a str,
    pub value: f64,
    pub range: (f64, f64),
    /// Spaced by ratio (the ƒ-number), not evenly.
    pub log: bool,
    pub text: &'a str,
    pub id: K,
}

#[derive(Default)]
pub(super) struct SliderOut {
    /// A new value (dragged, clicked or typed).
    pub value: Option<f64>,
    pub started: bool,
    pub stopped: bool,
}

pub(super) fn slider<K: std::hash::Hash + std::fmt::Debug>(ui: &mut Ui, s: Slider<K>) -> SliderOut {
    let p = ui.painter().clone();
    let mut out = SliderOut::default();
    p.text(pos2(s.left, s.top), Align2::LEFT_CENTER, crate::i18n::tr_s(s.label), theme::strong(12.5), Tokens::text_strong());
    let (lo, hi) = s.range;
    let to_t = |v: f64| if s.log { (v.max(lo) / lo).ln() / (hi / lo).ln() } else { (v - lo) / (hi - lo) };
    let from_t = |t: f64| if s.log { lo * (hi / lo).powf(t) } else { lo + (hi - lo) * t };
    let track = Rect::from_min_max(pos2(s.left + 2.0, s.top + 16.0), pos2(s.left + s.width - 70.0, s.top + 26.0));
    let t = to_t(s.value).clamp(0.0, 1.0) as f32;
    let knob = pos2(track.left() + track.width() * t, track.center().y);
    let line = |a: f32, b: f32, c: Color32| p.line_segment([pos2(a, track.center().y), pos2(b, track.center().y)], Stroke::new(3.0, c));
    line(track.left(), track.right(), theme::c(0x48484c));
    line(track.left(), knob.x, theme::c(0x3b5bdb));
    p.circle_filled(knob + vec2(0.0, 1.0), 8.5, Color32::from_black_alpha(70));
    p.circle_filled(knob, 8.0, Color32::WHITE);
    let resp = ui.interact(track.expand2(vec2(8.0, 6.0)), ui.scope_id().with(("slider", &s.id)), Sense::click_and_drag());
    out.started = resp.drag_started();
    out.stopped = resp.drag_stopped();
    if (resp.dragged() || resp.clicked())
        && let Some(q) = resp.interact_pointer_pos()
    {
        let t = ((q.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
        out.value = Some(from_t(t));
    }
    // The value box: shows the value, takes a typed one.
    let bx = Rect::from_min_max(pos2(s.left + s.width - 48.0, s.top + 12.0), pos2(s.left + s.width, s.top + 30.0));
    p.rect_filled(bx, radius(4), theme::c(0x1e1e20));
    p.rect_stroke(bx, radius(4), Stroke::new(1.0, theme::c(0x3a3a3c)), StrokeKind::Inside);
    let edit_id = ui.scope_id().with(("slider-text", &s.id));
    let mut text = ui.data(|d| d.get_temp::<String>(edit_id)).unwrap_or_else(|| s.text.to_string());
    let te = ui.put(
        bx.shrink2(vec2(3.0, 1.0)),
        egui::TextEdit::singleline(&mut text)
            .frame(egui::Frame::NONE)
            .align(egui::Align2::CENTER_CENTER)
            .font(theme::body(11.0))
            .text_color(Tokens::text()),
    );
    if te.has_focus() {
        ui.data_mut(|d| d.insert_temp(edit_id, text.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(edit_id));
    }
    if te.lost_focus() {
        let num: String = text.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | ',')).collect();
        if let Ok(v) = num.replace(',', ".").trim_start_matches('.').parse::<f64>()
            && v.is_finite()
        {
            out.value = Some(v.clamp(lo, hi));
            out.started = true;
            out.stopped = true;
        }
    }
    out
}

/// A small blue check box with its label (s13-013). Returns true when clicked.
pub(super) fn checkbox<K: std::hash::Hash + std::fmt::Debug>(ui: &mut Ui, at: Pos2, label: &str, on: bool, id: K) -> bool {
    let p = ui.painter().clone();
    let bx = Rect::from_center_size(pos2(at.x + 6.0, at.y), vec2(12.0, 12.0));
    if on {
        p.rect_filled(bx, radius(3), theme::c(0x3b5bdb));
        p.line_segment([bx.left_center() + vec2(2.5, 0.5), bx.center_bottom() + vec2(-1.0, -3.0)], Stroke::new(1.6, Color32::WHITE));
        p.line_segment([bx.center_bottom() + vec2(-1.0, -3.0), bx.right_top() + vec2(-2.5, 3.0)], Stroke::new(1.6, Color32::WHITE));
    } else {
        p.rect_filled(bx, radius(3), theme::c(0x3a3a3c));
        p.rect_stroke(bx, radius(3), Stroke::new(1.0, theme::c(0x5a5a5e)), StrokeKind::Inside);
    }
    let galley = p.layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.0), Tokens::text());
    let hit = Rect::from_min_size(pos2(at.x, at.y - 9.0), vec2(20.0 + galley.size().x, 18.0));
    p.galley(pos2(at.x + 17.0, at.y - galley.size().y / 2.0), galley, Tokens::text());
    ui.interact(hit, ui.scope_id().with(("check", id)), Sense::click()).clicked()
}

/// A grey pill with a blue chevron that opens a small menu of choices; returns the one picked.
pub(super) fn pill_menu<K: std::hash::Hash + std::fmt::Debug>(
    ui: &mut Ui,
    pill: Rect,
    label: &str,
    choices: &[&'static str],
    id: K,
) -> Option<&'static str> {
    let p = ui.painter().clone();
    let open_id = ui.scope_id().with(("pill-open", &id));
    let resp = ui.interact(pill, ui.scope_id().with(("pill", &id)), Sense::click());
    p.rect_filled(pill, radius(5), if resp.hovered() { theme::c(0x5a5a5e) } else { theme::c(0x4a4a4e) });
    let chev = Rect::from_min_max(pos2(pill.right() - 14.0, pill.top() + 2.0), pos2(pill.right() - 2.0, pill.bottom() - 2.0));
    p.rect_filled(chev, radius(4), theme::c(0x3b5bdb));
    crate::icons::paint(&p, chev.shrink(2.5), crate::icons::Icon::ChevronDown, Color32::WHITE);
    p.text(pos2(chev.left() - 5.0, pill.center().y), Align2::RIGHT_CENTER, crate::i18n::tr_s(label), theme::body(11.5), Tokens::text());
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }
    let mut picked = None;
    if open {
        let menu = egui::Area::new(open_id.with("menu"))
            .fixed_pos(pill.right_bottom() + vec2(-130.0, 4.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(130.0, 8.0 + 22.0 * choices.len() as f32), Sense::hover());
                let mp = ui.painter();
                mp.rect_filled(rect, radius(8), theme::themed(Color32::from_rgba_unmultiplied(30, 30, 33, 245)));
                mp.rect_stroke(rect, radius(8), Stroke::new(1.0, theme::c(0x48484c)), StrokeKind::Inside);
                for (k, c) in choices.iter().enumerate() {
                    let row = Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 4.0 + 22.0 * k as f32), vec2(122.0, 22.0));
                    let rr = ui.interact(row, open_id.with(k), Sense::click());
                    if rr.hovered() {
                        mp.rect_filled(row, radius(5), theme::c(0x213cb0));
                    }
                    if *c == label {
                        mp.text(pos2(row.left() + 6.0, row.center().y), Align2::LEFT_CENTER, "✓", theme::body(12.0), Tokens::text());
                    }
                    mp.text(pos2(row.left() + 20.0, row.center().y), Align2::LEFT_CENTER, *c, theme::body(12.5), Tokens::text());
                    if rr.clicked() {
                        picked = Some(*c);
                    }
                }
            });
        let outside = ui.input(|i| {
            i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|q| !menu.response.rect.contains(q) && !pill.contains(q))
        });
        if picked.is_some() || outside {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    picked
}

/// The blue Done across the bottom of a Properties sidebar. Returns true when clicked.
pub(super) fn done_button<K: std::hash::Hash + std::fmt::Debug>(ui: &mut Ui, r: Rect, id: K) -> bool {
    let done = Rect::from_min_size(pos2(r.left() + 12.0, r.bottom() - 36.0), vec2(r.width() - 24.0, 26.0));
    let dr = ui.interact(done, ui.scope_id().with(("done", id)), Sense::click());
    let p = ui.painter();
    p.rect_filled(done, radius(6), if dr.hovered() { theme::c(0x3a62e8) } else { theme::c(0x2f55d4) });
    p.text(done.center(), Align2::CENTER_CENTER, crate::i18n::tr("Done"), theme::body(12.5), Color32::WHITE);
    dr.clicked()
}

fn srgb(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

/// A lit sphere with the look's color, gloss and metalness, plus the groove the reference's swatches have.
pub fn ball(painter: &egui::Painter, center: Pos2, r: f32, look: Option<&Appearance>) {
    let (base, metal, gloss) = match look {
        Some(a) => (a.color, a.metal, a.gloss),
        None => ([0.63, 0.63, 0.64], 0.0, 0.35),
    };
    let light = glam::Vec3::new(-0.45, -0.6, 0.66).normalize();
    let shade = |x: f32, y: f32| -> Color32 {
        let z = (1.0 - x * x - y * y).max(0.0).sqrt();
        let n = glam::Vec3::new(x, y, z);
        let diffuse = n.dot(light).max(0.0);
        let h = (light + glam::Vec3::Z).normalize();
        let spec = n.dot(h).max(0.0).powf(8.0 + 200.0 * gloss * gloss) * (0.2 + 1.1 * gloss);
        // Studio reflection for metals: bright above, dark below.
        let env = (0.5 - 0.5 * y).clamp(0.0, 1.0);
        let mut c = [0.0f32; 3];
        for (k, ck) in c.iter_mut().enumerate() {
            let d = base[k] * (0.12 + 0.88 * diffuse) * (1.0 - 0.8 * metal);
            let m = base[k] * metal * (0.25 + 0.9 * env);
            *ck = (d + m + spec * if metal > 0.5 { base[k] } else { 1.0 }).clamp(0.0, 1.0);
        }
        srgb(c)
    };
    let mut mesh = egui::Mesh::default();
    let rings = 14;
    let segs = 40;
    let idx = |m: &mut egui::Mesh, x: f32, y: f32| {
        m.colored_vertex(center + vec2(x, y) * r, shade(x, y));
        m.vertices.len() as u32 - 1
    };
    let c0 = idx(&mut mesh, 0.0, 0.0);
    let mut prev: Vec<u32> = Vec::new();
    for ring in 1..=rings {
        let rr = ring as f32 / rings as f32;
        let cur: Vec<u32> = (0..segs)
            .map(|s| {
                let t = s as f32 / segs as f32 * std::f32::consts::TAU;
                idx(&mut mesh, rr * t.cos(), rr * t.sin())
            })
            .collect();
        for s in 0..segs {
            let (a, b) = (cur[s], cur[(s + 1) % segs]);
            if ring == 1 {
                mesh.add_triangle(c0, a, b);
            } else {
                let (pa, pb) = (prev[s], prev[(s + 1) % segs]);
                mesh.add_triangle(pa, a, b);
                mesh.add_triangle(pa, b, pb);
            }
        }
        prev = cur;
    }
    painter.add(egui::Shape::mesh(mesh));
    let d = vec2(0.7, -0.7) * r;
    painter.line_segment([center - d, center + d], Stroke::new(r * 0.07, Color32::from_black_alpha(90)));
}

fn tabs(ui: &mut Ui, r: Rect, labels: &[&str], sel: usize) -> Option<usize> {
    let p = ui.painter();
    p.rect_filled(r, radius(7), theme::c(0x1f1f21));
    let w = r.width() / labels.len() as f32;
    let mut out = None;
    for (k, l) in labels.iter().enumerate() {
        let seg = Rect::from_min_size(pos2(r.left() + w * k as f32, r.top()), vec2(w, r.height())).shrink(2.0);
        if k == sel {
            ui.painter().rect_filled(seg, radius(6), theme::c(0x5a5a5e));
        }
        ui.painter().text(seg.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(*l), theme::body(12.5), Tokens::text());
        if ui.interact(seg, ui.scope_id().with(("vis-tab", k)), Sense::click()).clicked() {
            out = Some(k);
        }
    }
    out
}

/// The bodies a look applies to: the selection's, or every body.
pub(super) fn targets(ws: &Workspace) -> Vec<ItemId> {
    let mut ids: Vec<ItemId> = Vec::new();
    for b in ws.session.selection.iter().filter_map(Pick::body) {
        if !ids.contains(&b) {
            ids.push(b);
        }
    }
    if ids.is_empty() {
        ids = ws.session.doc.bodies.iter().map(|b| b.id).collect();
    }
    ids
}

fn apply(ws: &mut Workspace, look: &Appearance) {
    let faces: Vec<(ItemId, u32)> = ws
        .session
        .selection
        .iter()
        .filter_map(|p| match *p {
            Pick::Face { body, face } => Some((body, face)),
            _ => None,
        })
        .collect();
    if !faces.is_empty() {
        // A face selection takes the color (faces carry a color, not a whole material).
        for (b, f) in faces {
            let _ = ws.run("set_face_color", json!({ "body": b.0, "faces": [f], "color": look.color }));
        }
        return;
    }
    let ids: Vec<u64> = targets(ws).iter().map(|i| i.0).collect();
    let _ = ws.run("set_appearance", json!({ "items": ids, "appearance": look }));
}

pub(super) fn sidebar(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    super::panels::panel(ui, r, ws.glass());
    super::decals::drag_frame(ws, ui);
    ws.paint_decal_handle(ui);
    if ws.vis_tab == 2 && ws.decal_sel.is_none() && !ws.env_settings && ws.session.selection.is_empty() {
        paint_focus(ws, ui);
    }
    if ws.decal_sel.is_some() {
        super::decals::properties(ws, ui, r);
        return;
    }
    if ws.env_settings {
        env_properties(ws, ui, r);
        return;
    }
    let selected = !ws.session.selection.is_empty();
    if selected && !ws.vis_changing {
        properties(ws, ui, r);
        return;
    }
    // Measured (L22): content 20 px in from either side.
    let tab_rect = Rect::from_min_size(r.min + vec2(20.0, 10.0), vec2(r.width() - 40.0, 26.0));
    if let Some(k) = tabs(ui, tab_rect, &["Material", "Environment", "Camera"], ws.vis_tab) {
        ws.vis_tab = k;
    }
    let body = Rect::from_min_max(pos2(r.left() + 18.0, tab_rect.bottom() + 8.0), pos2(r.right() - 6.0, r.bottom() - 6.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    egui::ScrollArea::vertical().id_salt("vis-scroll").show(&mut child, |ui| {
        ui.set_width(body.width() - 10.0);
        match ws.vis_tab {
            0 => materials(ws, ui),
            1 => environments(ws, ui),
            _ => camera(ws, ui),
        }
    });
}

fn materials(ws: &mut Workspace, ui: &mut Ui) {
    let w = ui.available_width();
    ui.add_space(2.0);
    // A grey field with a magnifier (as observed).
    let (field, _) = ui.allocate_exact_size(vec2(w - 8.0, 24.0), Sense::hover());
    ui.painter().rect_filled(field, radius(6), theme::c(0x353334));
    crate::icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(field.left() + 13.0, field.center().y), vec2(13.0, 13.0)),
        crate::icons::Icon::Search,
        Tokens::text_dim(),
    );
    let edit = egui::TextEdit::singleline(&mut ws.vis_search)
        .hint_text(crate::i18n::tr("Search Materials"))
        .frame(egui::Frame::NONE)
        .font(theme::body(12.5))
        .desired_width(field.width() - 34.0);
    ui.put(Rect::from_min_max(pos2(field.left() + 26.0, field.top() + 3.0), pos2(field.right() - 6.0, field.bottom() - 2.0)), edit);
    ui.add_space(14.0);
    let q = ws.vis_search.to_lowercase();
    // Used materials.
    let mut used: Vec<Option<Appearance>> = Vec::new();
    for b in &ws.session.doc.bodies {
        if !used.contains(&b.appearance) {
            used.push(b.appearance.clone());
        }
    }
    if q.is_empty() && !used.is_empty() {
        ui.label(
            egui::RichText::new(format!("{} ({})", crate::i18n::tr("Used Materials"), used.len()))
                .font(theme::strong(13.5))
                .color(Tokens::text_strong()),
        );
        ui.add_space(4.0);
        for look in &used {
            let (rect, resp) = ui.allocate_exact_size(vec2(w - 8.0, 52.0), Sense::click());
            // A click picks every body wearing it (s13-016), which shows its Properties.
            if resp.clicked() {
                let picks: Vec<Pick> =
                    ws.session.doc.bodies.iter().filter(|b| b.appearance == *look).map(|b| Pick::Body { body: b.id }).collect();
                ws.session.selection = picks;
                ws.note_selection();
            }
            let p = ui.painter();
            p.rect_filled(rect, radius(7), if resp.hovered() { theme::c(0x3a3a3c) } else { theme::c(0x303032) });
            ball(p, pos2(rect.left() + 26.0, rect.center().y), 19.0, look.as_ref());
            let (finish, name, col) = match look {
                Some(a) => (a.finish.clone(), a.name.clone(), a.color),
                None => ("Standard".into(), "Default Material".into(), [0.627, 0.627, 0.643]),
            };
            p.text(
                pos2(rect.left() + 54.0, rect.top() + 13.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(finish),
                theme::body(9.5),
                Tokens::text_dim(),
            );
            p.text(pos2(rect.left() + 54.0, rect.top() + 27.0), Align2::LEFT_CENTER, name, theme::strong(12.0), Tokens::text_strong());
            let hex = format!(
                "#{:02X}{:02X}{:02X}",
                (col[0] * 255.0).round() as u8,
                (col[1] * 255.0).round() as u8,
                (col[2] * 255.0).round() as u8
            );
            crate::icons::paint(
                p,
                Rect::from_center_size(pos2(rect.right() - 14.0, rect.center().y), vec2(10.0, 10.0)),
                crate::icons::Icon::ChevronRight,
                Tokens::text_dim(),
            );
            p.circle_filled(pos2(rect.left() + 58.0, rect.top() + 41.0), 4.0, srgb(col));
            p.text(pos2(rect.left() + 66.0, rect.top() + 41.0), Align2::LEFT_CENTER, hex, theme::body(9.5), Tokens::text_dim());
            ui.add_space(4.0);
        }
        ui.add_space(10.0);
    }
    if q.is_empty() {
        section_line(ui, w);
        decal_and_custom(ws, ui, w);
    }
    for (group, looks) in Appearance::library_groups() {
        // A search finds names, finishes and whole categories ("metal", "glossy", "walnut").
        let whole = !q.is_empty() && group.to_lowercase().contains(&q);
        let looks: Vec<Appearance> =
            looks.into_iter().filter(|a| q.is_empty() || whole || a.full_name().to_lowercase().contains(&q)).collect();
        if looks.is_empty() {
            continue;
        }
        section_line(ui, w);
        let (head, head_resp) = ui.allocate_exact_size(vec2(w - 8.0, 22.0), Sense::click());
        let open = !q.is_empty() || !ws.vis_collapsed.contains(group);
        ui.painter().text(
            pos2(head.left(), head.center().y),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(group),
            theme::strong(13.5),
            Tokens::text_strong(),
        );
        let chev = if open { crate::icons::Icon::ChevronDown } else { crate::icons::Icon::ChevronRight };
        crate::icons::paint(
            ui.painter(),
            Rect::from_center_size(pos2(head.right() - 8.0, head.center().y), vec2(9.0, 9.0)),
            chev,
            Tokens::text_dim(),
        );
        if head_resp.clicked() && q.is_empty() && !ws.vis_collapsed.remove(group) {
            ws.vis_collapsed.insert(group.to_string());
        }
        if !open {
            ui.add_space(4.0);
            continue;
        }
        ui.add_space(6.0);
        let cw = (w - 14.0) / 2.0;
        for pair in looks.chunks(2) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for look in pair {
                    let (rect, resp) = ui.allocate_exact_size(vec2(cw, cw + 30.0), Sense::click_and_drag());
                    let p = ui.painter();
                    p.rect_filled(rect, radius(6), if resp.hovered() { theme::c(0x48484b) } else { theme::c(0x3a3a3c) });
                    ball(p, pos2(rect.center().x, rect.top() + cw * 0.5), cw * 0.42, Some(look));
                    p.text(
                        pos2(rect.left() + 8.0, rect.bottom() - 22.0),
                        Align2::LEFT_CENTER,
                        &look.finish,
                        theme::body(9.5),
                        Tokens::text_dim(),
                    );
                    p.text(
                        pos2(rect.left() + 8.0, rect.bottom() - 9.0),
                        Align2::LEFT_CENTER,
                        &look.name,
                        theme::strong(11.5),
                        Tokens::text_strong(),
                    );
                    if resp.clicked() {
                        apply(ws, look);
                        ws.vis_changing = false;
                    }
                    // Dragged out onto a body: that body takes it (as the reference's tutorial teaches, s30-094). The
                    // ball follows the pointer meanwhile.
                    if resp.dragged()
                        && let Some(at) = resp.interact_pointer_pos()
                    {
                        let layer = egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("material-drag"));
                        ball(&ui.ctx().layer_painter(layer), at, 22.0, Some(look));
                    }
                    if resp.drag_stopped()
                        && let Some(at) = resp.interact_pointer_pos()
                        && ws.viewport_rect.contains(at)
                        && let Some(body) = ws.pick_at(at).and_then(|(p, _)| p.body())
                    {
                        let _ = ws.run("set_appearance", json!({ "items": [body.0], "appearance": look }));
                        ws.vis_changing = false;
                    }
                }
            });
            ui.add_space(6.0);
        }
        ui.add_space(8.0);
    }
}

fn environments(ws: &mut Workspace, ui: &mut Ui) {
    let w = ui.available_width() - 8.0;
    ui.label(egui::RichText::new(crate::i18n::tr("Studio")).font(theme::strong(12.5)).color(Tokens::text_strong()));
    ui.add_space(6.0);
    let current = ws.session.doc.visual.environment;
    for (i, (name, _, _)) in ENVIRONMENTS.iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 102.0), Sense::click());
        let p = ui.painter().clone();
        let mut look = ws.session.doc.visual.clone();
        look.environment = i;
        let bg = backdrop(&look);
        if i == 1 {
            // Transparent: a checkerboard.
            p.rect_filled(rect, radius(6), theme::c(0x2a2a2a));
            let cell = 9.0;
            let (nx, ny) = ((rect.width() / cell) as i32 + 1, (rect.height() / cell) as i32 + 1);
            for gy in 0..ny {
                for gx in (gy % 2..nx).step_by(2) {
                    let c = Rect::from_min_size(rect.min + vec2(gx as f32 * cell, gy as f32 * cell), vec2(cell, cell)).intersect(rect);
                    p.rect_filled(c, 0.0, theme::c(0x161616));
                }
            }
        } else {
            p.rect_filled(rect, radius(6), theme::c(bg));
        }
        // A chrome ball on its own little shadow, the reference's preview.
        let c = pos2(rect.center().x, rect.center().y - 8.0);
        p.add(egui::Shape::ellipse_filled(c + vec2(0.0, 32.0), vec2(26.0, 5.0), Color32::from_black_alpha(70)));
        ball(
            &p,
            c,
            30.0,
            Some(&Appearance {
                name: String::new(),
                finish: String::new(),
                color: [0.85, 0.86, 0.88],
                metal: 1.0,
                gloss: 0.95,
                ..Default::default()
            }),
        );
        let dark = (bg >> 16) < 0x80 || i == 1;
        let ink = if dark { Color32::WHITE } else { theme::c(0x1c1c1e) };
        let title = p.layout_no_wrap(name.to_string(), theme::strong(11.0), ink);
        let tw = title.size().x;
        let ty = if COLORED.contains(&i) { rect.bottom() - 24.0 } else { rect.bottom() - 13.0 };
        p.galley(pos2(rect.left() + 10.0, ty - title.size().y / 2.0), title, ink);
        if COLORED.contains(&i) {
            p.text(
                pos2(rect.left() + 24.0, rect.bottom() - 12.0),
                Align2::LEFT_CENTER,
                format!("#{bg:06X}"),
                theme::body(9.0),
                if dark { Tokens::text_dim() } else { theme::c(0x6a6a6e) },
            );
        }
        if current == i {
            if selected_marks(ui, &p, rect, pos2(rect.left() + 18.0 + tw, ty), dark, bg, ink, i) {
                ws.env_settings = true;
                continue;
            }
        } else if resp.hovered() {
            p.rect_stroke(rect, radius(6), Stroke::new(1.0, Tokens::text_dim()), StrokeKind::Inside);
        }
        if resp.clicked() && current != i {
            let _ = ws.run("visual_settings", json!({ "environment": i }));
        }
        ui.add_space(6.0);
    }
    custom_environments(ws, ui, w);
}

/// The selected card's ✓ after its name and its settings gear (s13-018). True when the gear was clicked.
#[allow(clippy::too_many_arguments)]
fn selected_marks(ui: &Ui, p: &egui::Painter, rect: Rect, tick: Pos2, dark: bool, bg: u32, ink: Color32, i: usize) -> bool {
    p.rect_stroke(rect, radius(6), Stroke::new(2.0, Tokens::accent()), StrokeKind::Inside);
    p.circle_filled(tick, 5.0, if dark { theme::c(0xbfbfc4) } else { theme::c(0x6a6a6e) });
    p.line_segment([tick + vec2(-2.4, 0.2), tick + vec2(-0.6, 2.0)], Stroke::new(1.3, theme::c(bg)));
    p.line_segment([tick + vec2(-0.6, 2.0), tick + vec2(2.6, -1.8)], Stroke::new(1.3, theme::c(bg)));
    let gear = Rect::from_center_size(pos2(rect.right() - 13.0, rect.bottom() - 12.0), vec2(14.0, 14.0));
    let gr = ui.interact(gear.expand(3.0), ui.scope_id().with(("env-gear", i)), Sense::click());
    crate::icons::paint(p, gear, crate::icons::Icon::Gear, if gr.hovered() { Tokens::accent() } else { ink });
    gr.clicked()
}

/// A panorama's thumbnail (made once per panorama): shown as it lights, about 256 px wide.
fn env_thumb(ws: &mut Workspace, ctx: &egui::Context, pano: &shapecraft_doc::Panorama) -> Option<egui::TextureId> {
    let stale = ws.env_thumbs.get(&pano.id.0).is_none_or(|(px, _)| !std::sync::Arc::ptr_eq(px, &pano.pixels));
    if stale {
        let px = &pano.pixels;
        if px.width == 0 || px.height == 0 || px.rgb.len() != (px.width * px.height) as usize {
            return None;
        }
        let (tw, th) = (256.min(px.width), (128.min(px.height)).max(1));
        let e = px.exposure();
        let srgb = |x: f32| {
            let x = (x * e).clamp(0.0, 1.0);
            let s = if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
            (s * 255.0).round() as u8
        };
        let mut rgba = Vec::with_capacity((tw * th * 4) as usize);
        for y in 0..th {
            for x in 0..tw {
                let (sx, sy) = ((x * px.width / tw).min(px.width - 1), (y * px.height / th).min(px.height - 1));
                let c = px.rgb.get((sy * px.width + sx) as usize).copied().unwrap_or([0.0; 3]);
                rgba.extend([srgb(c[0]), srgb(c[1]), srgb(c[2]), 255]);
            }
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &rgba);
        let tex = ctx.load_texture(format!("environment-{}", pano.id.0), image, egui::TextureOptions::LINEAR_REPEAT);
        ws.env_thumbs.insert(pano.id.0, (pano.pixels.clone(), tex));
    }
    ws.env_thumbs.get(&pano.id.0).map(|(_, t)| t.id())
}

/// The user's panoramas after the studio environments, each a card showing the picture, and an "Import…" row at
/// the end. Not observed in the reference (its import entry point was never seen): a guess, noted in
/// plan/shapr3d/01-observed-ui.md. Right-click a card to delete it.
fn custom_environments(ws: &mut Workspace, ui: &mut Ui, w: f32) {
    let panoramas = ws.session.doc.visual.panoramas.clone();
    let current = ws.session.doc.visual.environment;
    if !panoramas.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(crate::i18n::tr("Custom")).font(theme::strong(12.5)).color(Tokens::text_strong()));
        ui.add_space(6.0);
    }
    for (k, pano) in panoramas.iter().enumerate() {
        let i = shapecraft_doc::BUILTIN_ENVIRONMENTS + k;
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 102.0), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(rect, radius(6), theme::c(0x2a2a2a));
        if let Some(tex) = env_thumb(ws, ui.ctx(), pano) {
            // The panorama's middle band, cropped to the card.
            let band = (rect.height() / rect.width() * 0.5).min(1.0);
            let uv = Rect::from_min_max(pos2(0.0, 0.5 - band / 2.0), pos2(1.0, 0.5 + band / 2.0));
            let mut mesh = egui::Mesh::with_texture(tex);
            mesh.add_rect_with_uv(rect.shrink(1.0), uv, Color32::WHITE);
            p.add(mesh);
        }
        // A dark band under the name so it reads over any picture.
        let foot = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 26.0), rect.max);
        p.rect_filled(foot, egui::CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 }, Color32::from_black_alpha(120));
        let ink = Color32::WHITE;
        let title = p.layout_no_wrap(pano.name.clone(), theme::strong(11.0), ink);
        let tw = title.size().x.min(w - 60.0);
        let ty = rect.bottom() - 13.0;
        p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.right() - 40.0, rect.bottom()))).galley(
            pos2(rect.left() + 10.0, ty - title.size().y / 2.0),
            title,
            ink,
        );
        if current == i {
            if selected_marks(ui, &p, rect, pos2(rect.left() + 18.0 + tw, ty), true, 0x2a2a2a, ink, i) {
                ws.env_settings = true;
                continue;
            }
        } else if resp.hovered() {
            p.rect_stroke(rect, radius(6), Stroke::new(1.0, Tokens::text_dim()), StrokeKind::Inside);
        }
        if resp.clicked() && current != i {
            let _ = ws.run("visual_settings", json!({ "environment": i }));
        }
        resp.context_menu(|ui| {
            if ui.button(crate::i18n::tr("Delete")).clicked() {
                let _ = ws.run("delete_environment", json!({ "id": pano.id.0 }));
                ui.close();
            }
        });
        ui.add_space(6.0);
    }
    // Import…: a dashed-looking row with a ⊕.
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, radius(6), if resp.hovered() { theme::c(0x3a3a3c) } else { theme::c(0x2c2c2e) });
    p.rect_stroke(rect, radius(6), Stroke::new(1.0, theme::c(0x4a4a4e)), StrokeKind::Inside);
    let c = pos2(rect.left() + 20.0, rect.center().y);
    p.circle_stroke(c, 7.0, Stroke::new(1.3, Tokens::text_strong()));
    p.line_segment([c - vec2(3.5, 0.0), c + vec2(3.5, 0.0)], Stroke::new(1.3, Tokens::text_strong()));
    p.line_segment([c - vec2(0.0, 3.5), c + vec2(0.0, 3.5)], Stroke::new(1.3, Tokens::text_strong()));
    p.text(
        pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        crate::i18n::tr("Import Panorama…"),
        theme::strong(11.5),
        Tokens::text_strong(),
    );
    p.text(
        pos2(rect.right() - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        crate::i18n::tr("HDR · EXR · PNG · JPG"),
        theme::body(9.0),
        Tokens::text_dim(),
    );
    if resp.clicked() {
        ws.wants_environment = true;
    }
    ui.add_space(8.0);
}

/// Runs a slider's change to the Visualization settings as one undo step per drag.
fn visual_slider(ws: &mut Workspace, ui: &mut Ui, s: Slider<&str>, key: &str) {
    let out = slider(ui, s);
    if out.started {
        ws.live_step = None;
    }
    if let Some(v) = out.value {
        ws.live_run("visual_settings", json!({ key: v }));
    }
    if out.stopped {
        ws.live_step = None;
    }
}

/// An environment's settings (s13-019, s13-027): a preview of its light, Light Rotation, Sun Elevation and Light
/// Intensity, the ground plane and object shadow, and for the moods the backdrop color. Then Done.
fn env_properties(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    let v = ws.session.doc.visual.clone();
    let env = v.environment.min(ENVIRONMENTS.len() - 1);
    let p = ui.painter().clone();
    p.text(pos2(r.center().x, r.top() + 18.0), Align2::CENTER_CENTER, crate::i18n::tr("Properties"), theme::body(12.0), Tokens::text_dim());
    let left = r.left() + 14.0;
    let w = r.width() - 28.0;
    // Our own picture of the light (not the reference's photograph): sky over floor, the sun where it stands.
    let preview = Rect::from_min_size(pos2(left - 2.0, r.top() + 38.0), vec2(w + 4.0, 112.0));
    let mut mesh = egui::Mesh::default();
    let sky = |t: f32| Color32::from_gray((60.0 + 150.0 * (1.0 - t)) as u8);
    let horizon = preview.top() + preview.height() * 0.58;
    for (a, b, ca, cb) in
        [(preview.top(), horizon, sky(1.0), sky(0.0)), (horizon, preview.bottom(), Color32::from_gray(150), Color32::from_gray(70))]
    {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(pos2(preview.left(), a), ca);
        mesh.colored_vertex(pos2(preview.right(), a), ca);
        mesh.colored_vertex(pos2(preview.right(), b), cb);
        mesh.colored_vertex(pos2(preview.left(), b), cb);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    p.add(egui::Shape::mesh(mesh));
    let sx = preview.left() + preview.width() * (0.5 + v.light_rotation as f32 / 360.0);
    let sy = horizon - (horizon - preview.top() - 10.0) * (v.sun_elevation as f32 / 90.0);
    p.circle_filled(pos2(sx, sy), 11.0, Color32::from_white_alpha(70));
    p.circle_filled(pos2(sx, sy), 7.0, Color32::WHITE);
    // A custom environment previews its own panorama, turned with Light Rotation.
    if let Some(pano) = v.panorama().cloned()
        && let Some(tex) = env_thumb(ws, ui.ctx(), &pano)
    {
        let turn = (v.light_rotation / 360.0) as f32;
        let mut mesh = egui::Mesh::with_texture(tex);
        mesh.add_rect_with_uv(preview, Rect::from_min_max(pos2(-turn, 0.0), pos2(1.0 - turn, 1.0)), Color32::WHITE);
        p.add(mesh);
    }
    p.rect_stroke(preview, radius(6), Stroke::new(1.0, theme::c(0x3a3a3c)), StrokeKind::Inside);
    let mut y = preview.bottom() + 26.0;
    let rows = [
        ("Light Rotation", v.light_rotation, shapecraft_doc::LIGHT_ROTATION, format!("{:.0}°", v.light_rotation), "light_rotation"),
        ("Sun Elevation", v.sun_elevation, shapecraft_doc::SUN_ELEVATION, format!("{:.1}°", v.sun_elevation), "sun_elevation"),
        ("Light Intensity", v.light_intensity, shapecraft_doc::LIGHT_INTENSITY, format!("{:.1}", v.light_intensity), "light_intensity"),
    ];
    for (label, value, range, text, key) in rows {
        visual_slider(ws, ui, Slider { left, top: y, width: w, label, value, range, log: false, text: &text, id: key }, key);
        y += 50.0;
    }
    p.line_segment([pos2(left, y - 12.0), pos2(left + w, y - 12.0)], Stroke::new(1.0, theme::c(0x3e3d3e)));
    let ground_on = v.ground != shapecraft_doc::Ground::Off;
    if checkbox(ui, pos2(left, y), "Ground Plane", ground_on, "env-ground") {
        let _ = ws.run("visual_settings", json!({ "ground": if ground_on { "off" } else { "model" } }));
    }
    let label = if v.ground == shapecraft_doc::Ground::Xy { "XY Plane" } else { "Snap to Model" };
    let pill = Rect::from_min_size(pos2(left + w - 104.0, y - 9.0), vec2(104.0, 18.0));
    if let Some(k) = pill_menu(ui, pill, label, &["Snap to Model", "XY Plane"], "env-ground-kind") {
        let _ = ws.run("visual_settings", json!({ "ground": if k == "XY Plane" { "xy" } else { "model" } }));
    }
    y += 16.0;
    p.line_segment([pos2(left, y), pos2(left + w, y)], Stroke::new(1.0, theme::c(0x3e3d3e)));
    y += 17.0;
    if checkbox(ui, pos2(left, y), "Object Shadow", v.object_shadow, "env-shadow") {
        let _ = ws.run("visual_settings", json!({ "object_shadow": !v.object_shadow }));
    }
    y += 16.0;
    if COLORED.contains(&env) {
        p.line_segment([pos2(left, y), pos2(left + w, y)], Stroke::new(1.0, theme::c(0x3e3d3e)));
        y += 16.0;
        p.text(pos2(left, y), Align2::LEFT_CENTER, crate::i18n::tr("Background Color"), theme::strong(12.5), Tokens::text_strong());
        y += 12.0;
        let c = ws.vis_draft.unwrap_or(theme::c(backdrop(&v)));
        let (picked, used) = color_picker(ui, left, y, w, c, "backdrop");
        if let Some(c) = picked {
            ws.vis_draft = Some(c);
        }
        let set = |ws: &mut Workspace, c: Color32| {
            let _ = ws.run("visual_settings", json!({ "background": [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0] }));
        };
        if let Some(d) = ws.vis_draft
            && !ui.input(|i| i.pointer.any_down())
        {
            ws.vis_draft = None;
            set(ws, d);
        }
        let swatches = [0xffffff, 0xd0d0d2, 0x8e8e93, 0xf5d936, 0xf29b2c, 0xe5483d, 0xa660e0, 0x3a50e0, 0x3fc3f0, 0x3fbf6f];
        for (i, sw) in swatches.iter().enumerate() {
            let rr =
                Rect::from_center_size(pos2(left + 9.0 + 26.0 * (i % 8) as f32, used + 18.0 + 22.0 * (i / 8) as f32), vec2(16.0, 16.0));
            let resp = ui.interact(rr, ui.scope_id().with(("env-swatch", i)), Sense::click());
            p.rect_filled(rr, radius(3), theme::c(*sw));
            if resp.hovered() {
                p.rect_stroke(rr.expand(2.0), radius(4), Stroke::new(1.5, Tokens::accent()), StrokeKind::Inside);
            }
            if resp.clicked() {
                set(ws, theme::c(*sw));
            }
        }
    }
    if done_button(ui, r, "env-done") {
        ws.env_settings = false;
    }
}

/// The Camera tab (s13-022): Field of View, Aperture (ƒ1.4–ƒ32) and Blur Intensity, and how to set the focus.
fn camera(ws: &mut Workspace, ui: &mut Ui) {
    let w = ui.available_width() - 8.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, 170.0), Sense::hover());
    let left = rect.left();
    let mut y = rect.top() + 10.0;
    let fov = ws.camera.fov_y.to_degrees();
    let out = slider(
        ui,
        Slider {
            left,
            top: y,
            width: w,
            label: "Field of View",
            value: fov,
            range: (15.0, 90.0),
            log: false,
            text: &format!("{fov:.0}°"),
            id: "fov",
        },
    );
    if let Some(v) = out.value {
        ws.camera.fov_y = v.to_radians();
    }
    y += 54.0;
    let v = ws.session.doc.visual.clone();
    visual_slider(
        ws,
        ui,
        Slider {
            left,
            top: y,
            width: w,
            label: "Aperture",
            value: v.aperture,
            range: shapecraft_doc::APERTURE,
            log: true,
            text: &format!("ƒ{}", trim(v.aperture)),
            id: "aperture",
        },
        "aperture",
    );
    y += 50.0;
    visual_slider(
        ws,
        ui,
        Slider {
            left,
            top: y,
            width: w,
            label: "Blur Intensity",
            value: v.blur,
            range: shapecraft_doc::BLUR,
            log: false,
            text: &format!("{:.1}", v.blur),
            id: "blur",
        },
        "blur",
    );
    y += 40.0;
    let hint = ui.painter().layout(
        crate::i18n::tr_s("Right click anywhere on your model to move the focus point."),
        theme::body(10.0),
        Tokens::text_dim(),
        w,
    );
    ui.painter().galley(pos2(left, y), hint, Tokens::text_dim());
}

/// The focus point's marker while the Camera tab shows: yellow corner brackets (s13-025).
fn paint_focus(ws: &Workspace, ui: &Ui) {
    let Some(f) = ws.session.doc.visual.focus else { return };
    let Some((s, _)) = ws.camera.project(f, ws.size()) else { return };
    let c = ws.view_rect.min + vec2(s.x as f32, s.y as f32);
    if !ws.view_rect.contains(c) {
        return;
    }
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, ui.scope_id().with("focus-mark")));
    let (h, arm) = (14.0, 6.0);
    let stroke = Stroke::new(1.5, theme::c(0xd8b23a));
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let corner = c + vec2(sx * h, sy * h);
        p.line_segment([corner, corner - vec2(sx * arm, 0.0)], stroke);
        p.line_segment([corner, corner - vec2(0.0, sy * arm)], stroke);
    }
}

/// An ƒ-number as the reference shows it: 4, 1.4, 5.6.
fn trim(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or(s.clone(), str::to_string)
}

/// With something selected: its material, a color picker, and Done.
fn properties(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    p.text(pos2(r.center().x, r.top() + 18.0), Align2::CENTER_CENTER, crate::i18n::tr("Properties"), theme::body(12.0), Tokens::text_dim());
    let first_body = ws.session.selection.iter().find_map(Pick::body);
    let look = first_body.and_then(|b| ws.session.doc.body(b)).and_then(|b| b.appearance.clone());
    let face_color = ws.session.selection.iter().find_map(|pk| match *pk {
        Pick::Face { body, face } => {
            ws.session.doc.body(body).and_then(|b| b.face_colors.iter().find(|(f, _)| *f == face).map(|(_, c)| *c))
        }
        _ => None,
    });
    let mut y = r.top() + 40.0;
    if let Some(b) = first_body {
        y += super::decals::body_rows(ws, ui, r, y, b);
    }
    p.text(pos2(r.left() + 14.0, y), Align2::LEFT_CENTER, crate::i18n::tr("Material"), theme::strong(12.5), Tokens::text_strong());
    y += 14.0;
    let card = Rect::from_min_size(pos2(r.left() + 12.0, y), vec2(r.width() - 24.0, 54.0));
    p.rect_filled(card, radius(7), theme::c(0x303032));
    ball(&p, pos2(card.left() + 27.0, card.center().y), 19.0, look.as_ref());
    let (finish, name) =
        look.as_ref().map_or(("Standard".to_string(), "Default Material".to_string()), |a| (a.finish.clone(), a.name.clone()));
    p.text(
        pos2(card.left() + 54.0, card.top() + 15.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr_s(finish),
        theme::body(9.5),
        Tokens::text_dim(),
    );
    p.text(pos2(card.left() + 54.0, card.top() + 30.0), Align2::LEFT_CENTER, name, theme::strong(12.0), Tokens::text_strong());
    let change = Rect::from_center_size(pos2(card.right() - 32.0, card.center().y), vec2(56.0, 22.0));
    let cr = ui.interact(change, ui.scope_id().with("vis-change"), Sense::click());
    p.text(
        change.center(),
        Align2::CENTER_CENTER,
        crate::i18n::tr("Change"),
        theme::body(12.0),
        if cr.hovered() { theme::c(0x6fa0ff) } else { theme::c(0x3f86ff) },
    );
    if cr.clicked() {
        ws.vis_changing = true;
        ws.vis_tab = 0;
    }
    y = card.bottom() + 22.0;
    // A custom material has no color to pick: how big its picture's tiles are instead.
    if let Some(a) = look.as_ref().filter(|a| a.texture.is_some()) {
        let text = format!("{}x", trim(a.scale as f64));
        let out = slider(
            ui,
            Slider {
                left: r.left() + 14.0,
                top: y,
                width: r.width() - 28.0,
                label: "Scale",
                value: a.scale as f64,
                range: (0.05, 4.0),
                log: true,
                text: &text,
                id: "material-scale",
            },
        );
        if out.started {
            ws.live_step = None;
        }
        if let Some(v) = out.value {
            let mut a = a.clone();
            a.scale = v as f32;
            let ids: Vec<u64> = targets(ws).iter().map(|i| i.0).collect();
            ws.live_run("set_appearance", json!({ "items": ids, "appearance": a }));
        }
        if out.stopped {
            ws.live_step = None;
        }
        if done_button(ui, Rect::from_min_size(pos2(r.left() + 12.0, r.bottom() - 36.0), vec2(r.width() - 24.0, 26.0)), "material-done") {
            ws.session.selection.clear();
        }
        return;
    }
    p.text(pos2(r.left() + 14.0, y), Align2::LEFT_CENTER, crate::i18n::tr("Color"), theme::strong(12.5), Tokens::text_strong());
    y += 12.0;
    let base = face_color.or(look.as_ref().map(|a| a.color)).unwrap_or([0.627, 0.627, 0.643]);
    let c = ws.vis_draft.unwrap_or(srgb(base));
    let (picked, used) = color_picker(ui, r.left() + 14.0, y, r.width() - 28.0, c, "body");
    if let Some(p) = picked {
        ws.vis_draft = Some(p);
    }
    let released = !ui.input(|i| i.pointer.any_down());
    let set_color = |ws: &mut Workspace, c: Color32| {
        let rgb = [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0];
        let has_faces = ws.session.selection.iter().any(|p| matches!(p, Pick::Face { .. }));
        if has_faces {
            let _ = ws.run("set_face_color", json!({ "color": rgb }));
        } else {
            let mut a = look_or_default(ws);
            a.color = rgb;
            let ids: Vec<u64> = targets(ws).iter().map(|i| i.0).collect();
            let _ = ws.run("set_appearance", json!({ "items": ids, "appearance": a }));
        }
    };
    if let Some(d) = ws.vis_draft
        && released
    {
        ws.vis_draft = None;
        set_color(ws, d);
    }
    // Swatches.
    let swatches = [0xffffff, 0xd0d0d2, 0x8e8e93, 0xf5d936, 0xf29b2c, 0xe5483d, 0xa660e0, 0x3a50e0, 0x3fc3f0, 0x3fbf6f];
    let mut x = r.left() + 14.0;
    let sy = used + 18.0;
    for (i, sw) in swatches.iter().enumerate() {
        let rr = Rect::from_center_size(pos2(x + 9.0, sy), vec2(16.0, 16.0));
        let resp = ui.interact(rr, ui.scope_id().with(("swatch", i)), Sense::click());
        p.rect_filled(rr, radius(3), theme::c(*sw));
        if resp.hovered() {
            p.rect_stroke(rr.expand(2.0), radius(4), Stroke::new(1.5, Tokens::accent()), StrokeKind::Inside);
        }
        if resp.clicked() {
            set_color(ws, theme::c(*sw));
        }
        x += 26.0;
    }
    let reset = Rect::from_min_size(pos2(r.left() + 12.0, sy + 24.0), vec2(r.width() - 24.0, 26.0));
    let rr = ui.interact(reset, ui.scope_id().with("vis-default"), Sense::click());
    p.rect_filled(reset, radius(6), if rr.hovered() { theme::c(0x58585c) } else { theme::c(0x48484b) });
    p.text(reset.center(), Align2::CENTER_CENTER, crate::i18n::tr("Default Body Color"), theme::body(12.0), Tokens::text());
    if rr.clicked() {
        let has_faces = ws.session.selection.iter().any(|p| matches!(p, Pick::Face { .. }));
        if has_faces {
            let _ = ws.run("set_face_color", json!({ "color": null }));
        } else {
            let ids: Vec<u64> = targets(ws).iter().map(|i| i.0).collect();
            let _ = ws.run("set_appearance", json!({ "items": ids, "appearance": null }));
        }
    }
    let done = Rect::from_min_size(pos2(r.left() + 12.0, r.bottom() - 36.0), vec2(r.width() - 24.0, 26.0));
    let dr = ui.interact(done, ui.scope_id().with("vis-done"), Sense::click());
    p.rect_filled(done, radius(6), if dr.hovered() { theme::c(0x3a62e8) } else { theme::c(0x2f55d4) });
    p.text(done.center(), Align2::CENTER_CENTER, crate::i18n::tr("Done"), theme::body(12.5), Color32::WHITE);
    if dr.clicked() {
        ws.session.selection.clear();
    }
}

fn look_or_default(ws: &Workspace) -> Appearance {
    ws.session
        .selection
        .iter()
        .find_map(Pick::body)
        .and_then(|b| ws.session.doc.body(b))
        .and_then(|b| b.appearance.clone())
        .unwrap_or_default()
}

/// The Decal and Custom sections (as observed, L22, s13-007): the decal library, or with none yet the add tile and
/// Import… (the reference's sample pictures are its own), both asking for a picture; then Custom.
fn decal_and_custom(ws: &mut Workspace, ui: &mut Ui, w: f32) {
    let header = |ui: &mut Ui, title: &str, open: bool| -> bool {
        let (rect, resp) = ui.allocate_exact_size(vec2(w - 8.0, 22.0), Sense::click());
        let p = ui.painter();
        p.text(
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(title),
            theme::strong(13.5),
            Tokens::text_strong(),
        );
        let plus = pos2(rect.right() - 26.0, rect.center().y);
        p.circle_stroke(plus, 5.5, Stroke::new(1.2, Tokens::text_dim()));
        p.line_segment([plus - vec2(2.8, 0.0), plus + vec2(2.8, 0.0)], Stroke::new(1.2, Tokens::text_dim()));
        p.line_segment([plus - vec2(0.0, 2.8), plus + vec2(0.0, 2.8)], Stroke::new(1.2, Tokens::text_dim()));
        let chev = Rect::from_center_size(pos2(rect.right() - 8.0, rect.center().y), vec2(9.0, 9.0));
        crate::icons::paint(
            p,
            chev,
            if open { crate::icons::Icon::ChevronDown } else { crate::icons::Icon::ChevronRight },
            Tokens::text_dim(),
        );
        resp.clicked()
    };
    if header(ui, "Decal", true) {
        ws.wants_decal = true;
    }
    ui.add_space(4.0);
    if super::decals::library(ws, ui, w - 8.0) {
        ui.add_space(8.0);
        custom(ws, ui, w, &header);
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(w - 8.0, 92.0), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, radius(7), theme::c(0x303032));
    let tile = Rect::from_center_size(pos2(rect.center().x, rect.top() + 34.0), vec2(34.0, 34.0));
    let tile_resp = ui.interact(tile, ui.scope_id().with("decal-add"), Sense::click());
    p.rect_filled(tile, radius(3), if tile_resp.hovered() { theme::c(0xe8f0ff) } else { theme::c(0xdfe9ff) });
    p.rect_stroke(tile, radius(3), Stroke::new(1.5, Tokens::accent()), egui::StrokeKind::Inside);
    p.line_segment([tile.center() - vec2(6.0, 0.0), tile.center() + vec2(6.0, 0.0)], Stroke::new(1.2, theme::c(0x6e6e72)));
    p.line_segment([tile.center() - vec2(0.0, 6.0), tile.center() + vec2(0.0, 6.0)], Stroke::new(1.2, theme::c(0x6e6e72)));
    let button = Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 16.0), vec2(64.0, 16.0));
    let button_resp = ui.interact(button, ui.scope_id().with("decal-import"), Sense::click());
    p.rect_filled(button, radius(4), if button_resp.hovered() { theme::c(0x6a6a6e) } else { theme::c(0x58585c) });
    p.text(button.center(), Align2::CENTER_CENTER, crate::i18n::tr("Import…"), theme::body(10.5), Color32::WHITE);
    if tile_resp.clicked() || button_resp.clicked() {
        ws.wants_decal = true;
    }
    ui.add_space(14.0);
    custom(ws, ui, w, &header);
}

/// The Custom section: ⊕ imports a picture as a material (s13-034), its cards below.
fn custom(ws: &mut Workspace, ui: &mut Ui, w: f32, header: &dyn Fn(&mut Ui, &str, bool) -> bool) {
    section_line(ui, w);
    let any = ws.session.doc.decal_images.iter().any(|i| i.material);
    if header(ui, "Custom", any) {
        ws.wants_material = true;
    }
    ui.add_space(if any { 6.0 } else { 10.0 });
    if super::decals::custom_materials(ws, ui, w - 8.0) {
        ui.add_space(8.0);
    }
}

/// The reference's color picker (s13-027): a saturation/brightness square with a ring knob, a rainbow hue bar under
/// it, then "Hexadecimal" with the code in a small field (type one and press Return). Returns a new color while it's
/// dragged or typed, and where the picker ends.
pub(super) fn color_picker(ui: &mut Ui, left: f32, top: f32, width: f32, color: Color32, id: &str) -> (Option<Color32>, f32) {
    use egui::ecolor::Hsva;
    let p = ui.painter().clone();
    let key = ui.scope_id().with(("picker", id));
    // Hue and saturation are kept while dragging, so they don't jump at black or grey.
    let shown = Hsva::from(color);
    let mut hsv: Hsva = ui.data(|d| d.get_temp::<(Color32, Hsva)>(key)).filter(|(c, _)| *c == color).map_or(shown, |(_, h)| h);
    let mut out = None;
    let square = Rect::from_min_size(pos2(left, top), vec2(width, (width * 0.73).round()));
    let resp = ui.interact(square, key.with("sv"), Sense::click_and_drag());
    let n = 24;
    let mut mesh = egui::Mesh::default();
    for j in 0..=n {
        for i in 0..=n {
            let (sx, vy) = (i as f32 / n as f32, j as f32 / n as f32);
            let c: Color32 = Hsva::new(hsv.h, sx, 1.0 - vy, 1.0).into();
            mesh.colored_vertex(pos2(square.left() + sx * square.width(), square.top() + vy * square.height()), c);
        }
    }
    for j in 0..n {
        for i in 0..n {
            let a = j * (n + 1) + i;
            mesh.add_triangle(a, a + 1, a + n + 1);
            mesh.add_triangle(a + 1, a + n + 2, a + n + 1);
        }
    }
    p.add(mesh);
    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        hsv.s = ((pos.x - square.left()) / square.width()).clamp(0.0, 1.0);
        hsv.v = 1.0 - ((pos.y - square.top()) / square.height()).clamp(0.0, 1.0);
        out = Some(Color32::from(hsv));
    }
    let knob = pos2(square.left() + hsv.s * square.width(), square.top() + (1.0 - hsv.v) * square.height());
    p.circle_stroke(knob, 7.0, Stroke::new(1.5, Color32::WHITE));
    p.circle_stroke(knob, 8.5, Stroke::new(0.8, Color32::from_black_alpha(90)));
    // Hue bar.
    let bar = Rect::from_min_size(pos2(left, square.bottom() + 14.0), vec2(width, 20.0));
    let hr = ui.interact(bar, key.with("hue"), Sense::click_and_drag());
    let r = bar.height() / 2.0;
    // The rainbow between two round red ends.
    let mut mesh = egui::Mesh::default();
    let steps = 36;
    let span = bar.width() - 2.0 * r;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let c: Color32 = Hsva::new(t, 1.0, 1.0, 1.0).into();
        let x = bar.left() + r + t * span;
        mesh.colored_vertex(pos2(x, bar.top()), c);
        mesh.colored_vertex(pos2(x, bar.bottom()), c);
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k + 1, k);
        }
    }
    p.add(mesh);
    let red: Color32 = Hsva::new(0.0, 1.0, 1.0, 1.0).into();
    p.circle_filled(pos2(bar.left() + r, bar.center().y), r, red);
    p.circle_filled(pos2(bar.right() - r, bar.center().y), r, red);
    if let Some(pos) = hr.interact_pointer_pos().filter(|_| hr.dragged() || hr.clicked()) {
        hsv.h = ((pos.x - bar.left() - r) / (bar.width() - 2.0 * r)).clamp(0.0, 1.0);
        out = Some(Color32::from(hsv));
    }
    let hx = bar.left() + r + hsv.h * (bar.width() - 2.0 * r);
    let hc: Color32 = Hsva::new(hsv.h, 1.0, 1.0, 1.0).into();
    p.circle_filled(pos2(hx, bar.center().y), 7.5, hc);
    p.circle_stroke(pos2(hx, bar.center().y), 7.5, Stroke::new(2.0, Color32::WHITE));
    // Hexadecimal.
    let y = bar.bottom() + 24.0;
    p.text(pos2(left, y), Align2::LEFT_CENTER, crate::i18n::tr("Hexadecimal"), theme::body(11.0), Tokens::text_dim());
    let field = Rect::from_min_max(pos2(left + width - 66.0, y - 10.0), pos2(left + width, y + 10.0));
    p.rect_filled(field, radius(4), theme::c(0x1e1e1f));
    p.text(pos2(field.left() + 7.0, y), Align2::LEFT_CENTER, "#", theme::body(11.0), Tokens::text_dim());
    let current = out.unwrap_or(color);
    let code = format!("{:02X}{:02X}{:02X}", current.r(), current.g(), current.b());
    let tkey = key.with("hex-text");
    let mut text: String = ui.data(|d| d.get_temp::<String>(tkey)).unwrap_or_else(|| code.clone());
    let edit = ui.put(
        Rect::from_min_max(pos2(field.left() + 16.0, field.top() + 2.0), pos2(field.right() - 4.0, field.bottom() - 1.0)),
        egui::TextEdit::singleline(&mut text).frame(egui::Frame::NONE).font(theme::strong(11.0)).char_limit(6),
    );
    if edit.has_focus() {
        ui.data_mut(|d| d.insert_temp(tkey, text.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(tkey));
    }
    if edit.lost_focus()
        && let Ok(v) = u32::from_str_radix(text.trim().trim_start_matches('#'), 16)
        && text.trim().trim_start_matches('#').len() == 6
    {
        let c = theme::c(v);
        hsv = Hsva::from(c);
        out = Some(c);
    }
    let shown = out.unwrap_or(color);
    ui.data_mut(|d| d.insert_temp(key, (shown, hsv)));
    (out, y + 12.0)
}

/// The thin line between the material sections.
fn section_line(ui: &mut Ui, w: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(w - 8.0, 13.0), Sense::hover());
    ui.painter().line_segment([pos2(r.left(), r.top() + 0.5), pos2(r.right(), r.top() + 0.5)], Stroke::new(1.0, theme::c(0x3e3d3e)));
}
