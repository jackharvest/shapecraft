//! The viewport's right-click menu, Select Through, and the tool variant and distance-type menus.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use shapecraft_doc::Pick;

use crate::icons::Icon;
use crate::theme::{self, Tokens, radius};
use crate::workspace::Workspace;
use crate::workspace::toolbar::{self, Action, ToolDef, UiAction};
use crate::workspace::tools::Tool;

/// Help ▸ Keyboard Shortcuts…: every shortcut, from the same tables the app uses.
/// What a pick is called in lists ("Body 01 · Face 3").
fn pick_label(ws: &Workspace, p: Pick) -> String {
    let name = |id| ws.session.doc.item_name(id).unwrap_or("?").to_string();
    match p {
        Pick::Body { body } => name(body),
        Pick::Face { body, face } => format!("{} · Face {}", name(body), face + 1),
        Pick::Edge { body, edge } => format!("{} · Edge {}", name(body), edge + 1),
        Pick::Sketch { sketch } => name(sketch),
        Pick::Region { sketch, region } => format!("{} · Region {}", name(sketch), region + 1),
        Pick::SketchEntity { sketch, entity } => format!("{} · Curve {}", name(sketch), entity + 1),
        Pick::SketchPoint { sketch, entity, index } => format!("{} · Curve {} · Point {}", name(sketch), entity + 1, index + 1),
        Pick::Plane { plane } => name(plane),
        Pick::Axis { axis } => name(axis),
        Pick::Image { image } => name(image),
    }
}

/// "Select Through This Point…": everything under the point, nearest first; hover to see it, click to select.
pub(super) fn select_through(ws: &mut Workspace, ui: &mut Ui) {
    let Some((at, picks)) = ws.through.clone() else {
        ws.menu_hover = None;
        return;
    };
    let labels: Vec<String> = picks.iter().map(|p| pick_label(ws, *p)).collect();
    let rows: Vec<(&str, Option<Icon>, bool)> = labels
        .iter()
        .zip(&picks)
        .map(|(l, p)| {
            let icon = match p {
                Pick::Face { .. } => Icon::Body,
                Pick::Edge { .. } => Icon::Line,
                Pick::Region { .. } | Pick::SketchEntity { .. } | Pick::SketchPoint { .. } | Pick::Sketch { .. } => Icon::SketchItem,
                Pick::Plane { .. } => Icon::Plane,
                Pick::Axis { .. } => Icon::Axis,
                Pick::Body { .. } => Icon::Body,
                Pick::Image { .. } => Icon::Insert,
            };
            (l.as_str(), Some(icon), ws.session.selection.contains(p))
        })
        .collect();
    let (clicked, rect) = crate::workspace::popover::pull_down(ui, "select-through", at, 240.0, &rows);
    // Highlight the hovered entry in the view.
    let hovered =
        ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p)).map(|p| ((p.y - rect.top() - 5.0) / 24.0).floor() as isize);
    ws.menu_hover = hovered.and_then(|i| usize::try_from(i).ok()).and_then(|i| picks.get(i).copied());
    let outside = !ws.just_opened() && ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)));
    if let Some(p) = clicked.and_then(|i| picks.get(i).copied()) {
        let _ = ws.run("select", json!({ "picks": [p] }));
        ws.through = None;
        ws.menu_hover = None;
    } else if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.through = None;
        ws.menu_hover = None;
    }
}

/// The viewport's right-click menu, as observed: native-looking rows with separators and check marks.
/// A tool's variants in a second column beside it, the picked one highlighted (the main column drops its labels
/// meanwhile). Picking one keeps the tool running in that variant.
pub(super) fn variant_menu(ws: &mut Workspace, ui: &mut Ui, label: &'static str, at: Pos2) {
    let (names, _) = toolbar::variants(label);
    let icon = toolbar::SKETCHING.iter().chain(toolbar::FACE).find(|d| d.label == label).map_or(Icon::Pattern, |d| d.icon);
    let defs: Vec<ToolDef> = names
        .iter()
        .enumerate()
        .map(|(i, n)| ToolDef {
            icon: toolbar::variant_icon(label, i).unwrap_or(icon),
            label: n,
            sub: None,
            keys: "",
            action: Action::Soon,
            flyout: false,
        })
        .collect();
    let current = ws.variant(label);
    let active: Vec<bool> = (0..defs.len()).map(|i| i == current).collect();
    let vp = ws.viewport_rect;
    let top = pos2(at.x, at.y.min(vp.bottom() - 12.0 - toolbar::group_height(defs.len())));
    let hit = Rect::from_min_size(top, vec2(Tokens::TOOL_BUTTON + 120.0, toolbar::group_height(defs.len())));
    if let Some(i) = toolbar::group(ui, top, &defs, &active, true, "variants") {
        ws.variants.insert(label.into(), i);
        ws.variant_menu = None;
        // Restart the tool in its new variant (Pattern switches between its linear and circular tools).
        match label {
            "Rectangle" => ws.tool = ws.sketch_tool(toolbar::ToolKind::Rectangle),
            "Pattern" => ws.start_tool(if i == 1 { toolbar::ToolKind::CircularPattern } else { toolbar::ToolKind::Pattern }),
            "Offset Edge" => ws.start_tool(toolbar::ToolKind::OffsetEdge),
            "Scale" => ws.start_tool(if i == 0 { toolbar::ToolKind::Scale } else { toolbar::ToolKind::ScaleNonUniform }),
            "Spline" => ws.tool = ws.sketch_tool(toolbar::ToolKind::Spline),
            _ => {}
        }
        ws.preview = None;
        return;
    }
    let outside = ws.now - ws.popup_opened_at > 0.15
        && ui.input(|i| {
            i.pointer.any_click()
                && i.pointer.interact_pos().is_some_and(|q| !hit.contains(q) && q.x > vp.left() + 8.0 + Tokens::TOOL_BUTTON)
        });
    if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.variant_menu = None;
    }
}

/// Offset Face's Distance Type menu, as observed: a dim title, then Offset and Total (a round face: Offset, Radius,
/// Diameter; s37) with a check on the current one, in a popover pointing left at the pill.
pub(super) fn distance_menu(ws: &mut Workspace, ui: &mut Ui, at: Pos2) {
    // Offset Face: Offset / Total; Linear Pattern: Total / Spacing (327). (label, means "Total")
    // Chamfer/Fillet's kind pill (394): ✓ Fillet / Chamfer under a dim "Chamfer/Fillet". (true = Chamfer)
    use crate::workspace::tools::OffsetKind as K;
    let round = matches!(&ws.tool, Tool::OffsetFace(t) if t.radius.is_some());
    let options: &[(&str, u8)] = match ws.tool {
        Tool::OffsetFace(_) if round => &[("Offset", 0), ("Radius", 2), ("Diameter", 3)],
        Tool::OffsetFace(_) => &[("Offset", 0), ("Total", 1)],
        Tool::Pattern(_) => &[("Total", 1), ("Spacing", 0)],
        Tool::Fillet(_) => &[("Fillet", 0), ("Chamfer", 1)],
        _ => {
            ws.distance_menu = None;
            return;
        }
    };
    let current: u8 = match &ws.tool {
        Tool::Pattern(_) => ws.pattern_total as u8,
        Tool::Fillet(t) => t.chamfer as u8,
        Tool::OffsetFace(t) => match ws.offset_kind(t) {
            K::Offset => 0,
            K::Total => 1,
            K::Radius => 2,
            K::Diameter => 3,
        },
        _ => 0,
    };
    let title = if matches!(ws.tool, Tool::Fillet(_)) { "Chamfer/Fillet" } else { "Distance Type" };
    let (width, row_h) = (128.0, 24.0);
    let rect = Rect::from_min_size(pos2(at.x + 6.0, at.y - 38.0), vec2(width, 22.0 + row_h * options.len() as f32 + 8.0));
    let mut chosen = None;
    let shown = egui::Area::new(ui.scope_id().with("distance-menu"))
        .fixed_pos(rect.min - vec2(8.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            let (r, _) = ui.allocate_exact_size(rect.size() + vec2(8.0, 0.0), Sense::hover());
            let rect = Rect::from_min_max(pos2(r.left() + 8.0, r.top()), r.max);
            let p = ui.painter();
            p.rect_filled(rect.translate(vec2(0.0, 4.0)), radius(9), Color32::from_black_alpha(70));
            let fill = theme::c(0x2e2e30);
            p.rect_filled(rect, radius(9), fill);
            p.rect_stroke(rect, radius(9), Stroke::new(1.0, theme::c(0x48484a)), StrokeKind::Inside);
            let tip = pos2(rect.left() - 7.0, rect.top() + 38.0);
            p.add(egui::Shape::convex_polygon(
                vec![tip, pos2(rect.left() + 1.0, tip.y - 8.0), pos2(rect.left() + 1.0, tip.y + 8.0)],
                fill,
                Stroke::NONE,
            ));
            p.text(pos2(rect.left() + 14.0, rect.top() + 15.0), Align2::LEFT_CENTER, title, theme::body(12.5), Tokens::text_dim());
            for (i, &(label, total)) in options.iter().enumerate() {
                let row = Rect::from_min_size(pos2(rect.left() + 5.0, rect.top() + 26.0 + row_h * i as f32), vec2(width - 10.0, row_h));
                let resp = ui.interact(row, ui.scope_id().with(("distance-type", i)), Sense::click());
                if resp.hovered() {
                    p.rect_filled(row, radius(4), theme::c(0x2f6df5));
                }
                if current == total {
                    p.text(pos2(row.left() + 6.0, row.center().y), Align2::LEFT_CENTER, "✓", theme::body(13.0), Tokens::text_strong());
                }
                p.text(pos2(row.left() + 22.0, row.center().y), Align2::LEFT_CENTER, label, theme::body(13.0), Tokens::text_strong());
                if resp.clicked() {
                    chosen = Some(total);
                }
            }
        });
    let outside = ws.now - ws.popup_opened_at > 0.15
        && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !shown.response.rect.contains(q)));
    if let Some(v) = chosen {
        match &mut ws.tool {
            Tool::Pattern(t) => {
                ws.pattern_total = v == 1;
                t.total = v == 1;
            }
            Tool::Fillet(t) => t.chamfer = v == 1,
            _ if round => ws.offset_round = [K::Offset, K::Total, K::Radius, K::Diameter][v as usize],
            _ => ws.offset_total = v == 1,
        }
        ws.preview = None;
    }
    if chosen.is_some() || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.distance_menu = None;
    }
}

pub(super) fn context_menu(ws: &mut Workspace, ui: &mut Ui, at: Pos2, under: Option<Pick>, opened: f64) {
    let body = under.and_then(|p| p.body());
    let face = match under {
        Some(Pick::Face { body, face }) => Some((body, face)),
        _ => None,
    };
    let v = ws.view_opts;
    let through = ws.picks_through(at);
    // What Show/Hide, Isolate and Reveal act on: the item under the pointer, else the selection.
    let item = under.and_then(|p| p.item());
    let on_sketch = under.is_some_and(|p| p.sketch().is_some());
    // (label, checked); "" is a separator. As observed (L11): everything shows enabled, and the Sketch on Face /
    // Section View pair is left out over a sketch.
    let mut rows: Vec<(&str, bool)> = vec![("Select All", false), ("Select Through This Point…", false), ("", false)];
    if !on_sketch {
        rows.extend([("Sketch on Face", false), ("Section View", false), ("", false)]);
    }
    let ground = ws.session.doc.visual.ground;
    if ws.visualizing {
        // Visualization's own menu (s13-024).
        use shapecraft_doc::Ground;
        rows = vec![
            ("Select All", false),
            ("", false),
            ("Set as Focus Point", false),
            ("", false),
            ("Zoom to", false),
            ("Reveal in Items Sidebar", false),
            ("", false),
            ("Search Commands", false),
            ("", false),
            ("Snap Ground Plane to Model", ground == Ground::Model),
            ("Snap Ground Plane to XY", ground == Ground::Xy),
            ("Turn Off Ground Plane", false),
        ];
    } else if under.is_none() {
        // Over empty space (s23 ctx): the view toggles and the body visibility pair.
        rows = vec![
            ("Select All", false),
            ("", false),
            ("Show Grid", v.grid),
            ("Show World Axes", v.axes),
            ("Show Edges", v.edges),
            ("Show Hidden Edges", v.hidden_edges),
            ("Show Hidden Bodies", false),
            ("Invert Body Visibility", false),
            ("", false),
            ("Search Commands", false),
        ];
    } else {
        rows.extend([
            ("Show/Hide", false),
            ("Isolate", false),
            ("", false),
            ("Zoom to", false),
            ("Show Grid", v.grid),
            ("Show World Axes", v.axes),
            ("Show Edges", v.edges),
            ("Show Hidden Edges", v.hidden_edges),
            ("Reveal in Items Sidebar", false),
            ("", false),
            ("Search Commands", false),
        ]);
    }
    // Measured: 206 wide, 24 px rows, 11 px separators, 5 px padding; the dark see-through macOS menu with a black
    // outer and a light inner edge, a blue highlight.
    let row_h = 24.0;
    let sep_h = 11.0;
    let height: f32 = rows.iter().map(|r| if r.0.is_empty() { sep_h } else { row_h }).sum::<f32>() + 10.0;
    // As wide as its longest label (23 px in, 20 px after; 206 with "Select Through This Point…", s23 165 without).
    let widest =
        rows.iter().map(|r| ui.painter().layout_no_wrap(r.0.to_string(), theme::body(13.5), Tokens::text()).size().x).fold(0.0, f32::max);
    let width = (widest + 43.0).round();
    let vp = ws.view_rect;
    // macOS opens a menu 6 px above the pointer (measured).
    let origin = pos2(at.x.min(vp.right() - width - 4.0), (at.y - 6.0).min(vp.bottom() - height - 4.0));
    let mut chosen = None;
    let shown = egui::Area::new(ui.scope_id().with("ctx-menu")).fixed_pos(origin).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
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
        for (i, (label, checked)) in rows.iter().enumerate() {
            if label.is_empty() {
                p.line_segment(
                    [pos2(rect.left() + 15.0, y + sep_h / 2.0), pos2(rect.right() - 15.0, y + sep_h / 2.0)],
                    Stroke::new(1.0, theme::c(0x3a3b44)),
                );
                y += sep_h;
                continue;
            }
            let row = Rect::from_min_size(pos2(rect.left() + 5.0, y), vec2(width - 9.0, row_h));
            let resp = ui.interact(row, ui.scope_id().with(("ctx", i)), Sense::click());
            if resp.hovered() {
                p.rect_filled(row, radius(5), theme::c(0x213cb0));
            }
            let col = if resp.hovered() { Color32::WHITE } else { Tokens::text() };
            if *checked {
                p.text(pos2(rect.left() + 11.0, row.center().y), Align2::LEFT_CENTER, "✓", theme::body(13.0), col);
            }
            p.text(pos2(rect.left() + 23.0, row.center().y), Align2::LEFT_CENTER, *label, theme::body(13.5), col);
            if resp.clicked() {
                chosen = Some(*label);
            }
            y += row_h;
        }
    });
    let clicked_outside = ws.now - opened > 0.15
        && ui.input(|i| {
            (i.pointer.primary_clicked() || i.pointer.secondary_clicked())
                && i.pointer.interact_pos().is_some_and(|q| !shown.response.rect.contains(q))
        });
    if chosen.is_some() || clicked_outside {
        ws.context_menu = None;
    }
    let Some(label) = chosen else { return };
    match label {
        "Select All" => {
            let _ = ws.run("select_all", json!({}));
        }
        "Set as Focus Point" => {
            if let Some((_, p)) = ws.pick_at(at) {
                let _ = ws.run("visual_settings", json!({ "focus": [p.x, p.y, p.z] }));
            }
        }
        "Snap Ground Plane to Model" => {
            let _ = ws.run("visual_settings", json!({ "ground": "model" }));
        }
        "Snap Ground Plane to XY" => {
            let _ = ws.run("visual_settings", json!({ "ground": "xy" }));
        }
        "Turn Off Ground Plane" => {
            let _ = ws.run("visual_settings", json!({ "ground": "off" }));
        }
        "Select Through This Point…" => {
            ws.through = Some((origin, through));
            ws.popup_opened_at = ws.now;
        }
        "Sketch on Face" => match face {
            Some((b, f)) => {
                ws.sketch_on_face(b, f);
            }
            None => ws.start_tool(crate::workspace::toolbar::ToolKind::Sketch),
        },
        "Section View" => ws.ui_action(UiAction::SectionView),
        "Show/Hide" => {
            let items: Vec<u64> = match item {
                Some(i) => vec![i.0],
                None => ws.session.selection.iter().filter_map(|p| p.item()).map(|i| i.0).collect(),
            };
            if !items.is_empty() {
                let _ = ws.run("show_hide", json!({ "items": items }));
            }
        }
        "Isolate" => {
            if let Some(b) = body {
                ws.session.selection = vec![Pick::Body { body: b }];
            }
            if !ws.session.selection.is_empty() {
                ws.ui_action(UiAction::Isolate);
            }
        }
        "Zoom to" => ws.zoom_to(body),
        "Show Grid" => ws.view_opts.grid = !v.grid,
        "Show World Axes" => ws.view_opts.axes = !v.axes,
        "Show Edges" => ws.view_opts.edges = !v.edges,
        "Show Hidden Edges" => ws.view_opts.hidden_edges = !v.hidden_edges,
        "Show Hidden Bodies" => ws.show_hidden_bodies(),
        "Invert Body Visibility" => ws.invert_body_visibility(),
        "Reveal in Items Sidebar" => {
            ws.show_items = true;
            // The folder holding it opens, so its row shows.
            if let Some(f) = under.and_then(|p| p.item()).and_then(|i| ws.session.doc.folder_of(i)) {
                ws.items_expanded.insert(f);
            }
            if let Some(p) = under.filter(|p| p.item().is_some()) {
                ws.session.selection = vec![match p.sketch() {
                    Some(sketch) => Pick::Sketch { sketch },
                    None => p.body().map_or(p, |body| Pick::Body { body }),
                }];
            }
        }
        "Search Commands" => ws.ui_action(UiAction::Search),
        _ => {}
    }
}
