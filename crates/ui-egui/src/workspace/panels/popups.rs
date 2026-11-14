//! What floats over the viewport each frame: hint banner, tool options, flyouts, the Items row menu,
//! toasts, and the Replace Face card.

use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use shapecraft_engine::SelectionKind;

use super::command_search::command_search;
use super::context_menu::{context_menu, distance_menu, select_through, variant_menu};
use super::left_toolbar::{is_active, tool_options};
use super::right_popover::right_popover;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};
use crate::workspace::Workspace;
use crate::workspace::toolbar::{self, Action, ToolDef, UiAction};
use crate::workspace::tools::Tool;

pub(in crate::workspace) fn popups(ws: &mut Workspace, ui: &mut Ui) {
    let vp = ws.viewport_rect;
    let p = ui.painter().clone();

    // Hint banner: the running tool, or the one the selection suggests.
    let predicted = match (&ws.tool, ws.session.selection_kind()) {
        _ if ws.creating_drawing => Some(("Create Drawing", "Select the bodies to include in the drawing, then Next")),
        _ if ws.section_pick.is_some() => Some(("Pick a plane or a flat face to cut along", "")),
        (Tool::Idle, _) if ws.section.is_some_and(|s| s.editing) => Some(("Adjust section with arrows", "")),
        (Tool::Idle, _) if ws.measuring => Some(match ws.variant("Measure mode") {
            1 => ("Point-to-Point Distance", "Pick two points to measure the distance between them"),
            2 => ("3-Point Angle", "Pick an end, the corner, then the other end"),
            _ => ("Object Measurements", "Select something to see what it measures"),
        }),
        (Tool::Idle, SelectionKind::Regions | SelectionKind::Faces) => {
            Some(("Extrude - One-Sided (E)", "Press E or pick Extrude to pull it into a solid"))
        }
        (Tool::Idle, SelectionKind::Edges) => Some(("Chamfer/Fillet (F)", "Press F to round the selected edges")),
        // Waiting for a plane, the banner already explains the tool that will start on it (26.200); after the
        // Sketch button no tool waits and there is no banner (s23 L-flyout).
        (Tool::PickPlane { .. }, _) => ws.pending_tool.and_then(|k| ws.sketch_tool(k).hint()),
        (t, _) => t.hint(),
    };
    // A tutorial's instruction pill takes the banner's place.
    let predicted = if ws.tutorial.is_some() { None } else { predicted };
    // A touch screen with no keyboard: the pencil's words, and no keys to press (as on iPad).
    let touch_words: Option<(String, String)> = (!crate::tablet::shows_shortcuts())
        .then(|| predicted.map(|(t, b)| (t.to_string(), touch_hint(&ws.tool, ws.pending_tool, b))))
        .flatten();
    let predicted: Option<(&str, &str)> = match &touch_words {
        Some((t, b)) => Some((t.as_str(), b.as_str())),
        None => predicted,
    };
    // The running tool's options (behind its ≡).
    if ws.options_menu.is_some() && matches!(ws.tool, Tool::Onto(_)) {
        replace_face_card(ws, ui);
    } else if ws.card_tool() {
        // Extrude's, Fillet's and Split Body's ≡ open their cards (step_card.rs).
    } else if let Some(at) = ws.options_menu {
        let opts = tool_options(ws);
        if opts.is_empty() {
            ws.options_menu = None;
        } else {
            let labels: Vec<String> = opts
                .iter()
                .map(|(d, on)| match d.sub {
                    Some(sub) if d.label == "Pattern Type" || d.label.contains("Type") => format!("{} — {sub}", d.label),
                    _ => format!("{}{}", d.label, if *on { " (On)" } else { "" }),
                })
                .collect();
            let rows: Vec<(&str, Option<Icon>, bool)> = labels.iter().zip(&opts).map(|(l, (_, on))| (l.as_str(), None, *on)).collect();
            let (hit, rect) = crate::workspace::popover::pull_down(ui, "tool-options", at, 190.0, &rows);
            let outside = ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)))
                && ws.now - ws.popup_opened_at > 0.2;
            if let Some(i) = hit
                && let Some((d, _)) = opts.get(i)
            {
                ws.options_menu = None;
                ws.act(d.action, d.label);
            } else if outside {
                ws.options_menu = None;
            }
        }
    }
    let mut banner_bottom = vp.top();
    ws.numpad(ui);
    ws.paint_area_banner(ui);
    // Under the Area Selection banner, not over it.
    if ws.area.is_some() && !crate::tablet::touch_ui() {
        banner_bottom = vp.top() + 8.0 + 102.0;
    }
    // A cycled option's bubble takes the banner's place for a moment.
    let bubble = crate::workspace::badges::cycle_bubble(ws, ui);
    let held = ws.pen_input && ws.pen_held.is_some() && ws.area_open();
    let predicted = if ws.area.is_some() || held || bubble { None } else { predicted };
    ws.seen_banner = match (&ws.notice, predicted) {
        (Some((text, _)), _) => text.clone(),
        (None, Some((title, body))) => if body.is_empty() { title } else { body }.to_string(),
        (None, None) => String::new(),
    };
    if let Some((text, _)) = &ws.notice {
        // A refusal takes the banner's place (s12-004): a dark chip, the message centred on up to two lines.
        let mut job = egui::text::LayoutJob::simple(text.clone(), theme::body(13.0), Tokens::text_strong(), 280.0);
        job.halign = egui::Align::Center;
        let g = p.layout_job(job);
        // A one-word notice ("Visualized") is a small pill; a refusal a wider chip.
        let short = !text.contains(' ') && text.len() <= 20;
        let size = if short { vec2(g.size().x + 20.0, g.size().y + 8.0) } else { vec2(g.size().x.max(200.0) + 28.0, g.size().y + 12.0) };
        let r = Rect::from_center_size(pos2(vp.center().x, ws.banner_y() - 8.0 + if short { 18.0 } else { 28.0 }), size);
        banner_bottom = r.bottom();
        p.rect_filled(r, radius(8), theme::c(0x171616));
        p.galley(pos2(r.center().x, r.top() + if short { 4.0 } else { 6.0 }), g, Tokens::text_strong());
    } else if let Some((title, body)) = predicted {
        let title_g = p.layout_no_wrap(title.to_string(), theme::strong(12.0), Tokens::text_strong());
        let body_g = p.layout_no_wrap(body.to_string(), theme::body(11.0), Tokens::text_dim());
        // 26.200: no title, the hint as a paragraph in white beside the picture, wrapped at about 250 px (s11-024).
        let para = p.layout(body.to_string(), theme::body(13.5), Tokens::text_strong(), 268.0);
        // Measured (L05): 62 px tall, no border, the picture 56×42 at (12, 10), text from x 77, 30 px for the
        // chevron after it.
        // Measure's banner has no picture (as observed, L24): 46 px, text 12 px in.
        let plain = ws.measuring && matches!(ws.tool, Tool::Idle)
            || matches!(ws.tool, Tool::Text { at: Some(_), .. })
            || ws.section_pick.is_some()
            || ws.section.is_some_and(|s| s.editing) && matches!(ws.tool, Tool::Idle);
        let w = if plain { title_g.size().x.max(body_g.size().x) + 44.0 } else { para.size().x + 112.0 };
        let collapsed = ws.hint_collapsed;
        let h = if collapsed || plain && body.is_empty() {
            30.0
        } else if plain {
            46.0
        } else {
            (para.size().y + 20.0).max(64.0)
        };
        let r = Rect::from_min_size(pos2(vp.center().x - w / 2.0, ws.banner_y()), vec2(w, h));
        banner_bottom = r.bottom();
        p.rect_filled(r, radius(8), Tokens::banner());
        if plain && !collapsed && body.is_empty() {
            p.text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, title, theme::body(13.0), Tokens::text_strong());
        } else if plain && !collapsed {
            p.text(pos2(r.left() + 12.0, r.top() + 15.0), Align2::LEFT_CENTER, title, theme::strong(12.0), Tokens::text_strong());
            p.text(pos2(r.left() + 12.0, r.top() + 31.0), Align2::LEFT_CENTER, body, theme::body(11.0), Tokens::text_dim());
        } else if !collapsed {
            // The picture: a dark 56×41 tile, 11 px in, centred (s11-024).
            let thumb = Rect::from_min_size(pos2(r.left() + 11.0, r.center().y - 20.5), vec2(56.0, 41.0));
            p.rect_filled(thumb, radius(6), theme::c(0x1a1a1a));
            let icon = match &ws.tool {
                Tool::Rectangle { .. } => Icon::Rectangle,
                Tool::Circle { .. } => Icon::Circle,
                Tool::Polygon { .. } => Icon::Polygon,
                Tool::Arc { .. } => Icon::Arc,
                Tool::Ellipse { .. } => Icon::Ellipse,
                Tool::Spline { control: true, .. } => Icon::SplineControl,
                Tool::Spline { .. } => Icon::Spline,
                Tool::Line { .. } => Icon::Line,
                Tool::PickPlane { .. } => match ws.pending_tool {
                    Some(crate::workspace::toolbar::ToolKind::Rectangle) => Icon::Rectangle,
                    Some(crate::workspace::toolbar::ToolKind::Circle) => Icon::Circle,
                    Some(crate::workspace::toolbar::ToolKind::Polygon) => Icon::Polygon,
                    Some(crate::workspace::toolbar::ToolKind::Arc) => Icon::Arc,
                    Some(crate::workspace::toolbar::ToolKind::Ellipse) => Icon::Ellipse,
                    Some(crate::workspace::toolbar::ToolKind::Spline) => Icon::Spline,
                    _ => Icon::Line,
                },
                Tool::Fillet(_) | Tool::SketchFillet { .. } => Icon::Fillet,
                Tool::Move(t) if t.items.is_empty() => Icon::SketchMove,
                Tool::Move(_) => Icon::MoveRotate,
                Tool::Mirror(t) if t.split => Icon::SplitBody,
                Tool::Mirror(_) => Icon::Mirror,
                Tool::OffsetFace(_) => Icon::OffsetFace,
                Tool::Scale(_) => Icon::Scale,
                Tool::Pattern(_) | Tool::CircularPattern(_) => Icon::Pattern,
                Tool::Loft(_) => Icon::Loft,
                Tool::Sweep(_) => Icon::Sweep,
                Tool::Trim { .. } => Icon::Trim,
                Tool::Project => Icon::Project,
                Tool::Text { .. } => Icon::Text,
                Tool::OffsetEdge(_) => Icon::OffsetEdge,
                Tool::RotateAxis(_) => Icon::RotateAxis,
                Tool::OffsetPlane(_) | Tool::Plane3 { .. } | Tool::EdgePlane(_) => Icon::Plane,
                Tool::BodyEdgeOffset(_) => Icon::OffsetEdge,
                Tool::Boolean(t) if t.op == "subtract" => Icon::Subtract,
                Tool::Boolean(t) if t.op == "intersect" => Icon::Intersect,
                Tool::Boolean(_) => Icon::Union,
                Tool::Axis2 { .. } => Icon::Axis,
                Tool::PointPlane { .. } => Icon::Plane,
                Tool::Align { .. } => Icon::Align,
                Tool::Onto(_) => Icon::ReplaceFace,
                Tool::Translate { .. } => Icon::Translate,
                Tool::Revolve(_) => Icon::Revolve,
                _ if title.starts_with("Chamfer") => Icon::Fillet,
                _ => Icon::Extrude,
            };
            icons::paint(&p, thumb.shrink(7.0), icon, Tokens::icon());
            p.galley(pos2(r.left() + 75.0, r.center().y - para.size().y / 2.0), para, Tokens::text_strong());
        } else {
            p.text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, title, theme::strong(12.0), Tokens::text_strong());
        }
        let chev = Rect::from_center_size(pos2(r.right() - 13.0, r.center().y), vec2(12.0, 12.0));
        icons::paint(&p, chev, if collapsed { Icon::ChevronDown } else { Icon::ChevronUp }, Tokens::text_dim());
        if ui.interact(chev.expand(6.0), ui.scope_id().with("hint-collapse"), Sense::click()).clicked() {
            ws.hint_collapsed = !collapsed;
        }
    }

    // As observed: while sketching on a plane the view isn't square to, a "Normal to Sketch" pill under the
    // banner turns the camera to face it.
    if let Some(plane) = ws.session.active_sketch().map(|s| s.plane)
        && ws.camera.back().dot(plane.normal()).abs() < 0.995
        && !ws.animating()
    {
        let ink = theme::c(0x1c1c1e);
        let g = p.layout_no_wrap("Normal to Sketch".to_string(), theme::strong(12.0), ink);
        let r = Rect::from_center_size(pos2(vp.center().x, banner_bottom + 10.0 + 14.0), vec2(g.size().x + 52.0, 28.0));
        let resp = ui.interact(r, ui.scope_id().with("normal-to-sketch"), Sense::click());
        p.rect_filled(r, radius(9), if resp.hovered() { theme::c(0xe6e6ea) } else { theme::c(0xffffff) });
        // A plane seen at an angle with an arrow coming down onto it.
        let c = pos2(r.left() + 20.0, r.center().y + 3.0);
        let grey = theme::c(0x8e8e93);
        p.add(egui::Shape::convex_polygon(
            vec![c + vec2(-7.0, 0.0), c + vec2(0.0, -3.5), c + vec2(7.0, 0.0), c + vec2(0.0, 3.5)],
            grey,
            Stroke::NONE,
        ));
        p.line_segment([c + vec2(0.0, -12.0), c + vec2(0.0, -3.0)], Stroke::new(1.6, ink));
        p.add(egui::Shape::convex_polygon(vec![c + vec2(0.0, -1.5), c + vec2(-3.0, -5.0), c + vec2(3.0, -5.0)], ink, Stroke::NONE));
        p.galley(pos2(r.left() + 36.0, r.center().y - g.size().y / 2.0), g, ink);
        if resp.clicked() {
            ws.focus_plane_public(plane);
        }
    }

    // As observed (L14): Zebra names itself at the top of the view when no banner is up.
    if banner_bottom == vp.top()
        && matches!(ws.display, shapecraft_render::DisplayMode::Zebra { .. })
        && !ws.visualizing
        && ws.drawings.is_none()
    {
        p.text(pos2(vp.center().x, vp.top() + 15.0), Align2::CENTER_CENTER, "Zebra Analysis", theme::strong(12.0), Tokens::text_strong());
    }

    // As observed (L09): with something selected that runs out of the view, a white "Zoom to Selection" pill under
    // the banner frames it, and goes away once it shows.
    if ws.session.active_sketch.is_none()
        && ws.drawings.is_none()
        && !ws.animating()
        && let Some((lo, hi)) = ws.selection_bounds()
        && !ws.box_in_view(lo, hi)
    {
        let ink = theme::c(0x1c1c1e);
        let g = p.layout_no_wrap("Zoom to Selection".to_string(), theme::strong(12.5), ink);
        let r = Rect::from_center_size(pos2(vp.center().x, banner_bottom + 13.0 + 15.0), vec2(g.size().x + 54.0, 30.0));
        let resp = ui.interact(r, ui.scope_id().with("zoom-to-selection"), Sense::click());
        p.rect_filled(r, radius(10), if resp.hovered() { theme::c(0xe6e6ea) } else { theme::c(0xffffff) });
        // A dashed frame with a magnifier over its corner.
        let c = pos2(r.left() + 17.0, r.center().y);
        let frame = Rect::from_center_size(c, vec2(14.0, 14.0));
        let corners = [frame.left_top(), frame.right_top(), frame.right_bottom(), frame.left_bottom(), frame.left_top()];
        p.add(egui::Shape::dashed_line(&corners, Stroke::new(1.3, ink), 3.0, 2.0));
        let lens = c + vec2(1.0, 1.0);
        p.circle_filled(lens, 4.2, theme::c(0xffffff));
        p.circle_stroke(lens, 4.2, Stroke::new(1.4, ink));
        p.line_segment([lens + vec2(-2.0, 0.0), lens + vec2(2.0, 0.0)], Stroke::new(1.2, ink));
        p.line_segment([lens + vec2(0.0, -2.0), lens + vec2(0.0, 2.0)], Stroke::new(1.2, ink));
        p.line_segment([lens + vec2(3.0, 3.0), lens + vec2(6.0, 6.0)], Stroke::new(1.6, ink));
        p.galley(pos2(r.left() + 38.0, r.center().y - g.size().y / 2.0), g, ink);
        if resp.clicked() {
            ws.zoom_to_selection();
        }
    }

    // Selection pill.
    // Tools with Cancel / Done down there take the pill's place.
    let sketch_move = ws.sketch_transform().is_some()
        || matches!(ws.tool, Tool::SketchFillet { .. })
        || ws.is_live() && !matches!(ws.tool, Tool::Idle)
        || ws.picked_modify_tool();
    if let Some(text) =
        ws.session.selection_summary().filter(|_| !ws.offsetting_region() && !sketch_move && !matches!(ws.tool, Tool::Text { .. }))
    {
        // Edges and sketch curves add their total length in an inner chip, and a round button that opens Measure
        // (as observed: "1 edge | ∿ 7.4 mm | ‹").
        // A single round face shows its diameter in the same chip instead ("1 face | ⌀ 10 mm | ‹" for a 10 mm
        // cylinder, 26.202 s37).
        let units = ws.session.doc.units;
        let (length, round) = match ws.session.selection_length() {
            Some(l) => (Some(shapecraft_doc::format_length(l, units)), false),
            None => (ws.session.selection_radius().map(|r| shapecraft_doc::format_length(2.0 * r, units)), true),
        };
        let g = p.layout_no_wrap(text.clone(), theme::body(11.0), Tokens::text());
        let lg = length.as_ref().map(|l| p.layout_no_wrap(l.clone(), theme::body(11.0), Tokens::text_strong()));
        // Two lines meeting at a corner add their angle before the length (s15-006: "2 edges | ∠ 120.9638° | ∿ …").
        let ag = ws
            .picked_angle()
            .map(|d| p.layout_no_wrap(crate::workspace::viewport::format_degrees(d), theme::body(11.0), Tokens::text_strong()));
        let angle_w = ag.as_ref().map_or(0.0, |a| a.size().x + 24.0 + 6.0);
        let extra = lg.as_ref().map_or(0.0, |l| l.size().x + 24.0 + 6.0) + angle_w + if ag.is_some() || lg.is_some() { 22.0 } else { 0.0 };
        let h = if lg.is_some() || ag.is_some() { 26.0 } else { 22.0 };
        let r = Rect::from_center_size(pos2(vp.center().x, vp.bottom() - 28.0), vec2(g.size().x + 16.0 + extra, h));
        p.rect_filled(r, radius(6), Tokens::pill());
        p.text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, text, theme::body(11.0), Tokens::text());
        let measured = ag.is_some() || lg.is_some();
        if let Some(ag) = ag {
            let chip = Rect::from_min_size(pos2(r.left() + g.size().x + 14.0, r.top() + 4.0), vec2(ag.size().x + 24.0, 18.0));
            p.rect_filled(chip, radius(9), theme::c(0x48484b));
            // An angle mark: two strokes from a corner with a small arc between them.
            let v = pos2(chip.left() + 5.0, chip.center().y + 4.0);
            let ink = Stroke::new(1.1, Tokens::text());
            p.line_segment([v, v + vec2(11.0, 0.0)], ink);
            p.line_segment([v, v + vec2(6.0, -9.0)], ink);
            let arc: Vec<Pos2> = (0..=6).map(|k| v + egui::Vec2::angled(-(k as f32) * 0.164) * 6.5).collect();
            p.add(Shape::line(arc, ink));
            p.galley(pos2(chip.left() + 19.0, chip.center().y - ag.size().y / 2.0), ag, Tokens::text_strong());
        }
        if let Some(lg) = lg {
            let chip = Rect::from_min_size(pos2(r.left() + g.size().x + 14.0 + angle_w, r.top() + 4.0), vec2(lg.size().x + 24.0, 18.0));
            p.rect_filled(chip, radius(9), theme::c(0x48484b));
            if round {
                // A diameter mark: a circle struck through corner to corner.
                let c = pos2(chip.left() + 10.0, chip.center().y);
                p.circle_stroke(c, 5.0, Stroke::new(1.1, Tokens::text()));
                p.line_segment([c + vec2(-6.0, 6.0), c + vec2(6.0, -6.0)], Stroke::new(1.1, Tokens::text()));
            } else {
                // A small wave for "length".
                let w0 = pos2(chip.left() + 5.0, chip.center().y);
                let wave: Vec<Pos2> = (0..=12).map(|k| w0 + vec2(k as f32 * 0.85, -(k as f32 * 0.52).sin() * 2.2)).collect();
                p.add(Shape::line(wave, Stroke::new(1.1, Tokens::text())));
            }
            p.galley(pos2(chip.left() + 19.0, chip.center().y - lg.size().y / 2.0), lg, Tokens::text_strong());
        }
        if measured {
            let knob = pos2(r.right() - 12.0, r.center().y);
            p.circle_stroke(knob, 6.5, Stroke::new(1.0, Tokens::text_dim()));
            p.text(knob + vec2(-0.5, -0.5), Align2::CENTER_CENTER, "‹", theme::body(11.0), Tokens::text());
            if ui.interact(Rect::from_center_size(knob, vec2(16.0, 16.0)), ui.scope_id().with("pill-measure"), Sense::click()).clicked() {
                ws.ui_action(UiAction::Measure);
            }
        }
    }

    // Flyout from a category button.
    if let Some((cat, at)) = ws.flyout {
        // A second toolbar column (and a third for a cascade), as observed; kept inside the viewport.
        let vp = ws.viewport_rect;
        let column = |defs: &[ToolDef], at: Pos2| {
            let h = toolbar::group_height(defs.len());
            pos2(at.x, at.y.min(vp.bottom() - 12.0 - h).max(vp.top() + 8.0))
        };
        let defs: Vec<ToolDef> = if cat == toolbar::Category::Overflow { ws.overflow.clone() } else { toolbar::flyout(cat).to_vec() };
        let defs = &defs[..];
        let top = column(defs, at);
        let active: Vec<bool> = defs.iter().map(|d| is_active(ws, d)).collect();
        let mut hit = Rect::from_min_size(top, vec2(Tokens::TOOL_BUTTON + 290.0, toolbar::group_height(defs.len())));
        if let Some(i) = toolbar::group(ui, top, defs, &active, ws.flyout_sub.is_none(), "flyout") {
            let d = defs[i];
            if let Action::Flyout(sub) = d.action {
                let sat = pos2(top.x + Tokens::TOOL_BUTTON + 5.0, top.y + i as f32 * Tokens::TOOL_PITCH);
                ws.flyout_sub = if ws.flyout_sub.is_some_and(|(c, _)| c == sub) { None } else { Some((sub, sat)) };
                ws.popup_opened_at = ws.now;
            } else {
                ws.act(d.action, d.label);
            }
        } else if let Some((sub, sat)) = ws.flyout_sub {
            let sdefs = toolbar::flyout(sub);
            let stop = column(sdefs, sat);
            hit = hit.union(Rect::from_min_size(stop, vec2(Tokens::TOOL_BUTTON + 330.0, toolbar::group_height(sdefs.len()))));
            let sactive: Vec<bool> = sdefs.iter().map(|d| is_active(ws, d)).collect();
            if let Some(i) = toolbar::group(ui, stop, sdefs, &sactive, true, "flyout-sub") {
                let d = sdefs[i];
                ws.act(d.action, d.label);
            }
        }
        let outside = !ws.just_opened()
            && ui.input(|i| {
                i.pointer.any_click()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !hit.contains(p) && p.x > ws.viewport_rect.left() + 8.0 + Tokens::TOOL_BUTTON)
            });
        if outside {
            ws.flyout = None;
            ws.flyout_sub = None;
        }
    }

    if let Some((at, under, opened)) = ws.context_menu {
        context_menu(ws, ui, at, under, opened);
    }
    if let Some(at) = ws.distance_menu {
        distance_menu(ws, ui, at);
    }
    if let Some(at) = ws.var_menu {
        super::variables::variable_menu(ws, ui, at);
    }
    if ws.var_create.is_some() {
        super::variables::create_variable(ws, ui);
    }
    if let Some((b, at)) = ws.badge_menu {
        crate::workspace::badges::menu(ws, ui, b, at);
    }
    if let Some((label, at)) = ws.variant_menu {
        variant_menu(ws, ui, label, at);
    }

    if let Some((pop, at, opened)) = ws.right_pop {
        right_popover(ws, ui, pop, at, opened);
    }

    crate::workspace::measure::pinned_labels(ws, ui);
    if ws.measuring {
        crate::workspace::measure::card(ws, ui);
    }

    if let Some((id, at)) = ws.item_menu {
        let is_sketch = ws.session.doc.sketch(id).is_some();
        let is_body = ws.session.doc.body(id).is_some();
        let is_image = ws.session.doc.image(id).is_some();
        let mut entries: Vec<&str> = vec!["Rename", "Show/Hide"];
        if is_body {
            entries.extend(["Zoom to", "Isolate", "Invert Visibility"]);
        }
        if is_sketch {
            entries.push("Edit Sketch");
        }
        // A reference picture's see-through-ness, in steps.
        if is_image {
            entries.extend(["Opacity 100%", "Opacity 75%", "Opacity 50%", "Opacity 25%"]);
        }
        entries.push("Delete");
        let mut chosen = None;
        let shown = egui::Area::new(ui.scope_id().with("item-menu")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(180.0, 28.0 * entries.len() as f32 + 8.0), Sense::hover());
            ui.painter().rect_filled(rect, radius(8), Tokens::banner());
            ui.painter().rect_stroke(rect, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
            for (i, label) in entries.iter().enumerate() {
                let row = Rect::from_min_size(rect.min + vec2(4.0, 4.0 + 28.0 * i as f32), vec2(172.0, 28.0));
                let r = ui.interact(row, ui.scope_id().with(("item-menu", i)), Sense::click());
                if r.hovered() {
                    ui.painter().rect_filled(row, radius(6), Tokens::accent());
                }
                let col = if *label == "Delete" && !r.hovered() { Tokens::danger() } else { Tokens::text() };
                ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, *label, theme::body(12.5), col);
                if r.clicked() {
                    chosen = Some(*label);
                }
            }
        });
        let outside = !ws.just_opened()
            && ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !shown.response.rect.contains(p)));
        match chosen {
            Some("Rename") => {
                let name = ws.session.doc.item_name(id).unwrap_or_default().to_string();
                ws.renaming = Some((Some(id), name, true));
                ws.item_menu = None;
            }
            Some("Show/Hide") => {
                let _ = ws.run("show_hide", json!({ "items": [id.0] }));
                ws.item_menu = None;
            }
            Some("Edit Sketch") => {
                ws.session.active_sketch = Some(id);
                if let Some(sk) = ws.session.doc.sketch_mut(id) {
                    sk.visible = true;
                    let plane = sk.plane;
                    ws.focus_plane_public(plane);
                }
                ws.item_menu = None;
            }
            Some("Delete") => {
                let _ = ws.run("delete", json!({ "items": [id.0] }));
                ws.item_menu = None;
            }
            Some("Zoom to") => {
                ws.zoom_to(Some(id));
                ws.item_menu = None;
            }
            Some("Isolate") => {
                ws.isolated = Some(vec![id]);
                ws.item_menu = None;
            }
            Some("Invert Visibility") => {
                // Everything else swaps: shown bodies hide, hidden ones show; this one stays shown.
                let others: Vec<u64> = ws.session.doc.bodies.iter().filter(|b| b.id != id).map(|b| b.id.0).collect();
                if !others.is_empty() {
                    let _ = ws.run("show_hide", json!({ "items": others }));
                }
                ws.item_menu = None;
            }
            Some(label) if label.starts_with("Opacity ") => {
                let pct: f64 = label.trim_start_matches("Opacity ").trim_end_matches('%').parse().unwrap_or(100.0);
                let _ = ws.run("image_settings", json!({ "image": id.0, "opacity": pct / 100.0 }));
                ws.item_menu = None;
            }
            _ if outside => ws.item_menu = None,
            _ => {}
        }
    }

    if ws.show_shortcuts {
        crate::workspace::shortcuts::window(ws, ui);
    }
    select_through(ws, ui);

    command_search(ws, ui);

    // Toasts, bottom center above the pill.
    let mut y = vp.bottom() - 64.0;
    for t in ws.toasts.iter().rev() {
        let g = p.layout_no_wrap(t.text.clone(), theme::body(12.0), Tokens::text());
        let r = Rect::from_center_size(pos2(vp.center().x, y), vec2(g.size().x + 24.0, 30.0));
        p.rect_filled(r, radius(8), Tokens::banner());
        p.rect_stroke(r, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, &t.text, theme::body(12.0), Tokens::text());
        y -= 36.0;
    }
}

/// Replace Face's options, a card over the foot of the Items sidebar (s11-124/126): the faces to replace as chips,
/// the replacing face, Flip Alignment, and a red bar when the pick was refused. It stays up while picking.
fn replace_face_card(ws: &mut Workspace, ui: &mut Ui) {
    let Tool::Onto(t) = &ws.tool else { return };
    let mut t = t.clone();
    let error = t.error.clone();
    let bounds = ui.ctx().content_rect();
    let h = if t.project { 195.0 } else { 237.0 } + if error.is_some() { 43.0 } else { 0.0 };
    let card = Rect::from_min_max(pos2(bounds.left() + 4.0, bounds.bottom() - 5.0 - h), pos2(bounds.left() + 324.0, bounds.bottom() - 5.0));
    let mut changed = false;
    egui::Area::new(ui.scope_id().with("replace-face-card")).fixed_pos(card.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(card.size(), Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, radius(10), theme::c(0x2a2727));
        p.rect_stroke(rect, radius(10), Stroke::new(1.0, theme::c(0x6f6d6d)), StrokeKind::Inside);
        let head = pos2(rect.left() + 16.0, rect.top() + 19.0);
        let title = p.text(head, Align2::LEFT_CENTER, t.title(), theme::strong(13.0), Tokens::text_strong());
        if error.is_some() {
            warning(p, pos2(title.right() + 12.0, head.y));
        }
        // ⋯ in a ring at the right.
        let more = pos2(rect.right() - 22.0, head.y);
        p.circle_stroke(more, 6.0, Stroke::new(1.2, theme::c(0x5b7cf9)));
        for dx in [-2.6, 0.0, 2.6] {
            p.circle_filled(more + vec2(dx, 0.0), 0.9, theme::c(0x5b7cf9));
        }
        p.hline(rect.x_range(), rect.top() + 37.0, Stroke::new(1.0, theme::c(0x4d4a4a)));
        let mut y = rect.top() + 42.0;
        if let Some(e) = &error {
            let bar = Rect::from_min_size(pos2(rect.left() + 5.0, y), vec2(rect.width() - 10.0, 37.0));
            p.rect_filled(bar, radius(6), theme::c(0x58302f));
            warning(p, pos2(bar.left() + 17.0, bar.center().y));
            p.text(pos2(bar.left() + 33.0, bar.center().y), Align2::LEFT_CENTER, e, theme::body(12.5), Tokens::text_strong());
            y = bar.bottom() + 6.0;
        }
        let inner = Rect::from_min_max(pos2(rect.left() + 5.0, y), pos2(rect.right() - 5.0, rect.bottom() - 5.0));
        p.rect_filled(inner, radius(7), theme::c(0x2d2a2a));
        p.rect_stroke(inner, radius(7), Stroke::new(1.0, theme::c(0x322f2f)), StrokeKind::Inside);
        let rules = inner.left() + 11.0..=inner.right() - 11.0;
        let rule = Stroke::new(1.0, theme::c(0x413f3f));
        let dot = |p: &egui::Painter, cy: f32| {
            let c = pos2(inner.right() - 18.0, cy);
            p.circle_filled(c, 8.0, theme::c(0x313c67));
            p.circle_filled(c, 3.5, theme::c(0x2e68ff));
        };
        // Faces to Replace: one chip per face, its ⓧ takes it out.
        let (first, second) = if t.project { ("Items to Project", "Project Onto") } else { ("Faces to Replace", "Replacing Face") };
        p.text(pos2(inner.left() + 11.0, inner.top() + 18.0), Align2::LEFT_CENTER, first, theme::body(13.0), Tokens::text());
        let mut x = inner.left() + 11.0;
        let cy = inner.top() + 49.0;
        let mut remove = None;
        for (i, (pick, _)) in t.items.iter().enumerate() {
            let g = p.layout_no_wrap(t.chip(pick).into(), theme::strong(10.5), Tokens::text_strong());
            let chip = Rect::from_min_size(pos2(x, cy - 9.0), vec2(g.size().x + 27.0, 18.0));
            p.rect_filled(chip, radius(9), theme::c(0x2e68ff));
            p.galley(pos2(chip.left() + 6.0, cy - g.size().y / 2.0), g, Tokens::text_strong());
            let xc = pos2(chip.right() - 9.0, cy);
            p.circle_filled(xc, 5.0, Color32::from_white_alpha(200));
            p.text(xc, Align2::CENTER_CENTER, "×", theme::strong(8.0), theme::c(0x2e68ff));
            if ui.interact(Rect::from_center_size(xc, vec2(14.0, 14.0)), ui.scope_id().with(("rf-chip", i)), Sense::click()).clicked() {
                remove = Some(i);
            }
            x = chip.right() + 4.0;
        }
        dot(p, cy);
        p.hline(rules.clone(), inner.top() + 73.0, rule);
        // Replacing Face.
        p.text(pos2(inner.left() + 11.0, inner.top() + 92.0), Align2::LEFT_CENTER, second, theme::body(13.0), Tokens::text());
        let cy2 = inner.top() + 125.0;
        if t.to.is_some() {
            let g = p.layout_no_wrap("1 face".into(), theme::strong(10.5), Tokens::text_strong());
            let chip = Rect::from_min_size(pos2(inner.left() + 11.0, cy2 - 9.0), vec2(g.size().x + 27.0, 18.0));
            p.rect_filled(chip, radius(9), theme::c(0x2e68ff));
            p.galley(pos2(chip.left() + 6.0, cy2 - g.size().y / 2.0), g, Tokens::text_strong());
            let xc = pos2(chip.right() - 9.0, cy2);
            p.circle_filled(xc, 5.0, Color32::from_white_alpha(200));
            p.text(xc, Align2::CENTER_CENTER, "×", theme::strong(8.0), theme::c(0x2e68ff));
            if ui.interact(Rect::from_center_size(xc, vec2(14.0, 14.0)), ui.scope_id().with("rf-to"), Sense::click()).clicked() {
                t.to = None;
                changed = true;
            }
        } else {
            let ask = if t.project { "Select a face..." } else { "Select a face or a plane..." };
            p.text(pos2(inner.left() + 11.0, cy2), Align2::LEFT_CENTER, ask, theme::body(12.5), Tokens::text_dim());
        }
        dot(p, cy2);
        if let Some(i) = remove {
            t.items.remove(i);
            changed = true;
        }
        if t.project {
            return;
        }
        p.hline(rules, inner.top() + 146.0, rule);
        // Flip Alignment switch.
        let cy3 = inner.top() + 168.0;
        p.text(pos2(inner.left() + 11.0, cy3), Align2::LEFT_CENTER, "Flip Alignment", theme::body(13.0), Tokens::text());
        let sw = Rect::from_center_size(pos2(inner.right() - 26.0, cy3), vec2(40.0, 24.0));
        p.rect_filled(sw, radius(12), if t.flip { theme::c(0x2443c4) } else { theme::c(0x4a4747) });
        let knob = if t.flip { sw.right() - 12.0 } else { sw.left() + 12.0 };
        p.circle_filled(pos2(knob, cy3), 10.0, theme::c(0xcacaca));
        if ui.interact(sw.expand(4.0), ui.scope_id().with("rf-flip"), Sense::click()).clicked() {
            t.flip = !t.flip;
            changed = true;
        }
    });
    if changed {
        ws.apply_onto(t);
    }
}

/// A red warning triangle with a "!".
fn warning(p: &egui::Painter, c: Pos2) {
    let red = theme::c(0xff453a);
    p.add(Shape::convex_polygon(vec![c + vec2(0.0, -6.5), c + vec2(7.0, 5.5), c + vec2(-7.0, 5.5)], red, Stroke::NONE));
    p.text(c + vec2(0.0, 1.0), Align2::CENTER_CENTER, "!", theme::strong(9.0), Color32::WHITE);
}

/// A hint reworded for a touch screen with no keyboard: taps instead of clicks, and the sentences about keys left out.
/// Line says what the pencil does (a round stroke draws a circle).
pub(crate) fn touch_hint(tool: &Tool, pending: Option<crate::workspace::toolbar::ToolKind>, body: &str) -> String {
    use crate::workspace::toolbar::ToolKind;
    let line = matches!(tool, Tool::Line { .. }) || matches!(tool, Tool::PickPlane { .. }) && pending == Some(ToolKind::Line);
    if line {
        return "Draw lines with the pen. A round stroke draws a circle.".to_string();
    }
    let keys = ["Return", "Escape", "Esc ", "Tab", "Shift", "⌘", "⌥", "Space", "Press ", "press ", "Delete key", "arrow keys"];
    let mut kept: Vec<String> = Vec::new();
    for sentence in body.split_inclusive(['.', ';']) {
        let s = sentence.trim();
        if s.is_empty() || keys.iter().any(|k| s.contains(k)) {
            continue;
        }
        let s = s.trim_end_matches(';').to_string();
        kept.push(if s.ends_with('.') || s.ends_with('!') || s.ends_with('?') { s } else { format!("{s}.") });
    }
    let out = if kept.is_empty() { body.to_string() } else { kept.join(" ") };
    out.replace("Click", "Tap").replace("click", "tap")
}

#[cfg(test)]
mod touch_hint_tests {
    use super::*;

    #[test]
    fn touch_hints_tap_and_leave_out_keys() {
        let line = Tool::Line { first: None, last: None };
        assert_eq!(touch_hint(&line, None, "Click to place the ends."), "Draw lines with the pen. A round stroke draws a circle.");
        let idle = Tool::Idle;
        assert_eq!(touch_hint(&idle, None, "Click a face to start. Return finishes; Escape cancels."), "Tap a face to start.");
        assert_eq!(touch_hint(&idle, None, "Press E to extrude"), "Press E to extrude", "nothing left: the original stays");
    }
}
