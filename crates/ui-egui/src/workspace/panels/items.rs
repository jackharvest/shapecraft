//! The Items sidebar: bodies, sketches, planes, axes, images and folders, with its filter and more menus.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use shapecraft_doc::Pick;

use super::left_toolbar::tool_mode;
use super::{panel, select_all};
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};
use crate::workspace::Workspace;
use crate::workspace::tools::Tool;

fn icon_button(ui: &mut Ui, r: Rect, icon: Icon, id: &str) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, radius(6), Tokens::button_hover());
    }
    icons::paint(ui.painter(), r.shrink(r.width() * 0.18), icon, Tokens::icon());
    resp.clicked()
}

pub(in crate::workspace) fn items_sidebar(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    panel(ui, r, ws.glass());
    let p = ui.painter().clone();
    // Measured (L02): grey title 17 px in; the filter bar 10 px in, 44 down, 24 tall.
    p.text(pos2(r.left() + 17.0, r.top() + 20.5), Align2::LEFT_CENTER, crate::i18n::tr("Items"), theme::strong(14.5), theme::c(0x9f9e9f));
    let filter = Rect::from_min_size(pos2(r.left() + 10.0, r.top() + 44.0), vec2(r.width() - 20.0, 24.0));
    let filter_resp = ui.interact(filter, ui.scope_id().with("items-filter"), Sense::click());
    p.rect_filled(filter, radius(5), if filter_resp.hovered() { Tokens::button_hover() } else { theme::c(0x302f2f) });
    let (filter_label, filter_icon) = ITEM_FILTERS[ws.items_filter.min(ITEM_FILTERS.len() - 1)];
    // Measured (s22-002): the icon centred 13 px in, the label from 26 px.
    icons::paint(
        &p,
        Rect::from_center_size(pos2(filter.left() + 13.5, filter.center().y), vec2(12.0, 12.0)),
        filter_icon,
        Tokens::text_dim(),
    );
    p.text(pos2(filter.left() + 26.5, filter.center().y), Align2::LEFT_CENTER, filter_label, theme::strong(11.0), theme::c(0xa2a1a1));
    let chev = pos2(filter.right() - 10.0, filter.center().y);
    icons::paint(&p, Rect::from_center_size(chev - vec2(0.0, 3.0), vec2(8.0, 8.0)), Icon::ChevronUp, Tokens::text_dim());
    icons::paint(&p, Rect::from_center_size(chev + vec2(0.0, 3.0), vec2(8.0, 8.0)), Icon::ChevronDown, Tokens::text_dim());

    let footer_y = r.bottom() - 30.0;
    p.line_segment([pos2(r.left(), footer_y - 26.0 + 0.5), pos2(r.right(), footer_y - 26.0 + 0.5)], Stroke::new(1.0, Tokens::divider()));
    let new_folder = Rect::from_center_size(pos2(r.left() + 20.0, footer_y), vec2(20.0, 20.0));
    if icon_button(ui, new_folder, Icon::NewFolder, "items-folder") {
        // With a folder picked (its row), the new one goes inside it (s24-021).
        let parent = picked_folder(ws);
        let _ = ws.run("new_folder", json!({ "parent": parent.map(|f| f.0) }));
    }
    let more = Rect::from_center_size(pos2(r.right() - 20.0, footer_y), vec2(20.0, 20.0));
    let more_clicked = icon_button(ui, more, Icon::Ellipsis, "items-more");
    items_menus(ws, ui, filter, filter_resp.clicked(), more, more_clicked);

    // As observed, a sketch with nothing drawn in it isn't listed yet.
    let doc = &ws.session.doc;
    let listed_sketches = doc.sketches.iter().any(|s| !s.entities.is_empty());
    if doc.bodies.is_empty()
        && !listed_sketches
        && doc.planes.is_empty()
        && doc.axes.is_empty()
        && doc.images.is_empty()
        && doc.folders.is_empty()
    {
        let c = pos2(r.center().x, r.center().y - 10.0);
        p.text(c, Align2::CENTER_CENTER, crate::i18n::tr("Workspace is empty"), theme::strong(12.5), Tokens::text_strong());
        p.text(c + vec2(0.0, 20.0), Align2::CENTER_CENTER, "Start modeling to see your items here", theme::body(11.0), Tokens::text_dim());
        return;
    }

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let mut rows: Vec<(shapecraft_doc::ItemId, String, bool, ItemKind)> = Vec::new();
    let shown = |k: ItemKind| match ws.items_filter {
        0 => true,
        1 => k == ItemKind::Body,
        2 => k == ItemKind::Sketch,
        3 => matches!(k, ItemKind::Plane | ItemKind::Axis),
        4 => k == ItemKind::Image,
        _ => false,
    };
    for b in ws.session.doc.bodies.iter().rev() {
        rows.push((b.id, b.name.clone(), b.visible, ItemKind::Body));
    }
    for s in ws.session.doc.sketches.iter().rev().filter(|s| !s.entities.is_empty()) {
        let active = ws.session.active_sketch == Some(s.id);
        rows.push((s.id, s.name.clone(), s.visible || active, ItemKind::Sketch));
    }
    for pl in ws.session.doc.planes.iter().rev() {
        rows.push((pl.id, pl.name.clone(), pl.visible, ItemKind::Plane));
    }
    for ax in ws.session.doc.axes.iter().rev() {
        rows.push((ax.id, ax.name.clone(), ax.visible, ItemKind::Axis));
    }
    for im in ws.session.doc.images.iter().rev() {
        rows.push((im.id, im.name.clone(), im.visible, ItemKind::Image));
    }
    rows.retain(|row| shown(row.3));
    // Newest first whatever the kind, as observed (Plane 01 above Body 01 above Sketch 01), unless dragged elsewhere.
    let doc = &ws.session.doc;
    rows.sort_by(|a, b| doc.item_key(a.0).total_cmp(&doc.item_key(b.0)));
    // Folders (with All Items): their items under them when open, 13 px further in per level (as observed, L23).
    let rows: Vec<Row> = if ws.items_filter == 0 && !doc.folders.is_empty() {
        let mut all: Vec<Row> = rows.iter().map(|r| (r.0, r.1.clone(), r.2, r.3, 0)).collect();
        for f in &doc.folders {
            let inside = doc.folder_items_deep(f.id);
            let shows = inside.is_empty() || inside.iter().any(|i| rows.iter().any(|r| r.0 == *i && r.2));
            all.push((f.id, f.name.clone(), shows, ItemKind::Folder, 0));
        }
        all.sort_by(|a, b| doc.item_key(a.0).total_cmp(&doc.item_key(b.0)));
        let mut out = Vec::new();
        tree(doc, &all, &ws.items_expanded, None, 0, &mut out);
        out
    } else {
        rows.into_iter().map(|r| (r.0, r.1, r.2, r.3, 0)).collect()
    };
    // Where a dragged row would land, from the pointer over the (evenly spaced) rows.
    let top = r.top() + 74.0;
    let drop =
        ws.items_dragging.zip(pointer.filter(|q| r.contains(*q))).and_then(|(item, q)| drop_at(&ws.session.doc, &rows, item, top, q.y));
    let mut y = r.top() + 74.0;
    let mut row_hover = None;
    let waiting = tool_mode(ws).is_some();
    let mut dragged_row: Option<(String, Icon)> = None;
    for (id, name, visible, kind, depth) in rows {
        if y > footer_y - 40.0 {
            break;
        }
        let indent = 13.0 * depth as f32;
        let row = Rect::from_min_size(pos2(r.left() + 9.0, y), vec2(r.width() - 18.0, Tokens::ROW_H - 2.0));
        let resp = ui.interact(row, ui.scope_id().with(("item", id.0)), Sense::click_and_drag());
        // Dragging a row onto another lights it up blue (a folder takes it in, an item makes a folder with it);
        // between rows a blue line shows where it goes (s24-022, s24-025).
        if resp.drag_started() {
            ws.items_dragging = Some(id);
        }
        let dragging_this = ws.items_dragging == Some(id);
        let drop_on_this = matches!(drop, Some(Drop::Onto(t)) if t == id);
        if drop_on_this {
            p.rect_filled(row, radius(5), theme::c(0x2e68ff));
            p.rect_stroke(row, radius(5), Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
        }
        let selected = ws.item_selected(id);
        // The whole item selected (not just a face or edge of it) fills the row: blue while the list has focus,
        // grey once a tool starts or the view is clicked, as observed.
        let whole = ws.session.selection.iter().any(|p| {
            matches!(*p, Pick::Body { body } if body == id)
                || matches!(*p, Pick::Sketch { sketch } if sketch == id)
                || *p == Pick::Plane { plane: id }
                || *p == Pick::Axis { axis: id }
                || *p == Pick::Image { image: id }
        });
        // Hovering a row lights its item up in the view.
        if resp.hovered() {
            row_hover = match kind {
                ItemKind::Body => Some(Pick::Body { body: id }),
                ItemKind::Sketch => Some(Pick::Sketch { sketch: id }),
                ItemKind::Plane => Some(Pick::Plane { plane: id }),
                ItemKind::Axis => Some(Pick::Axis { axis: id }),
                ItemKind::Image => Some(Pick::Image { image: id }),
                ItemKind::Folder => None,
            };
        }
        if whole {
            p.rect_filled(row, radius(5), if ws.items_focus { theme::c(0x2e68ff) } else { theme::c(0x454548) });
        } else if selected {
            p.rect_filled(row, radius(5), Tokens::row_selected());
        } else if resp.hovered() {
            p.rect_filled(row, radius(5), Tokens::row_hover());
        }
        // While a tool waits for picks in the view, the list goes dim except what it can still take: Mirror and
        // Split Body take more bodies (as observed, 324–334).
        let takes = kind == ItemKind::Body && matches!(ws.tool, Tool::Mirror(_) | Tool::Boolean(_));
        // Measured (s22-002): names #DFDFDF, hidden ones #818181; icons a grey hairline, #9D9D9F or #5F5F5F hidden.
        let lit = visible && (!waiting || takes) && !dragging_this;
        let color = if whole && ws.items_focus || drop_on_this {
            Color32::WHITE
        } else if lit {
            Tokens::text()
        } else {
            theme::c(0x818181)
        };
        let icon_color = if color == Color32::WHITE {
            color
        } else if lit {
            Tokens::text_dim()
        } else {
            theme::c(0x5f5f5f)
        };
        let icon = match kind {
            ItemKind::Body => Icon::Body,
            ItemKind::Sketch => Icon::SketchItem,
            ItemKind::Plane => Icon::Plane,
            ItemKind::Axis => Icon::Axis,
            ItemKind::Image => Icon::Insert,
            ItemKind::Folder => Icon::Folder,
        };
        if dragging_this {
            dragged_row = Some((name.clone(), icon));
        }
        // A folder's chevron, left of its icon (as observed, L23).
        let open = kind == ItemKind::Folder && ws.items_expanded.contains(&id);
        if kind == ItemKind::Folder {
            icons::paint(
                &p,
                Rect::from_center_size(pos2(r.left() + 25.0 + indent, row.center().y), vec2(10.0, 10.0)),
                if open { Icon::ChevronDown } else { Icon::ChevronRight },
                color,
            );
        }
        // Measured: 14 px icons centred 49 px in, names from 66 px.
        icons::paint(&p, Rect::from_center_size(pos2(r.left() + 49.0 + indent, row.center().y), vec2(19.0, 19.0)), icon, icon_color);
        let editing = matches!(&ws.renaming, Some((Some(rid), ..)) if *rid == id);
        if editing {
            if let Some((_, text, focus)) = &mut ws.renaming {
                let field = Rect::from_min_max(pos2(row.left() + 48.0, row.top() + 1.0), pos2(row.right() - 30.0, row.bottom() - 1.0));
                let edit_id = ui.scope_id().with(("rename-item", id.0));
                let resp = ui.put(field, egui::TextEdit::singleline(text).id(edit_id).font(theme::body(12.0)));
                if *focus {
                    resp.request_focus();
                    select_all(ui.ctx(), edit_id, text.chars().count());
                    *focus = false;
                }
                let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                if enter || resp.lost_focus() && !esc {
                    let new = text.clone();
                    ws.renaming = None;
                    let _ = ws.run("rename", json!({ "item": id.0, "name": new }));
                } else if esc {
                    ws.renaming = None;
                }
            }
        } else {
            // Long names end in "…" before the eye, as in the reference (s36), instead of running into the viewport.
            let x = r.left() + 66.0 + indent;
            let shown = theme::elide(&p, &name, theme::body(14.0), row.right() - 32.0 - x);
            p.text(pos2(x, row.center().y), Align2::LEFT_CENTER, shown, theme::body(14.0), color);
        }
        if resp.secondary_clicked() {
            ws.item_menu = resp.interact_pointer_pos().map(|pos| (id, pos));
            ws.popup_opened_at = ws.now;
        }
        let eye = Rect::from_center_size(pos2(row.right() - 17.0, row.center().y), vec2(20.0, 20.0));
        let eye_resp = ui.interact(eye, ui.scope_id().with(("eye", id.0)), Sense::click());
        if !visible || resp.hovered() || eye_resp.hovered() {
            // A hidden row's eye stays dim, as in the reference (s20-030).
            let tint = if visible || eye_resp.hovered() { Tokens::text_dim() } else { egui::Color32::from_rgb(0x3e, 0x3d, 0x3d) };
            icons::paint(&p, eye, if visible { Icon::Eye } else { Icon::EyeOff }, tint);
        }
        let on_chevron = kind == ItemKind::Folder && resp.interact_pointer_pos().is_some_and(|q| q.x < r.left() + 36.0 + indent);
        if eye_resp.clicked() {
            let _ = ws.run("show_hide", json!({ "items": [id.0] }));
        } else if resp.clicked() && on_chevron {
            if !ws.items_expanded.remove(&id) {
                ws.items_expanded.insert(id);
            }
        } else if resp.clicked() {
            let pick_of = |doc: &shapecraft_doc::Document, item: shapecraft_doc::ItemId| -> Option<Pick> {
                if doc.body(item).is_some() {
                    Some(Pick::Body { body: item })
                } else if doc.sketch(item).is_some() {
                    Some(Pick::Sketch { sketch: item })
                } else if doc.plane(item).is_some() {
                    Some(Pick::Plane { plane: item })
                } else if doc.axes.iter().any(|a| a.id == item) {
                    Some(Pick::Axis { axis: item })
                } else {
                    doc.image(item).map(|_| Pick::Image { image: item })
                }
            };
            // A folder picks everything in it.
            let picks: Vec<Pick> = match ws.session.doc.folder(id) {
                Some(_) => ws.session.doc.folder_items_deep(id).iter().filter_map(|i| pick_of(&ws.session.doc, *i)).collect(),
                None => pick_of(&ws.session.doc, id).into_iter().collect(),
            };
            let shift = ui.input(|i| i.modifiers.shift);
            // A sketch's row opens it in place with all its curves picked, ready for Move/Rotate and the rest.
            let sketch = ws.session.doc.sketch(id).filter(|_| !shift && !ws.visualizing && ws.drawings.is_none()).map(|k| k.entities.len());
            let opened = sketch.is_some_and(|n| {
                ws.end_model_tool();
                if ws.session.active_sketch != Some(id) && ws.session.active_sketch.is_some() {
                    let _ = ws.run("exit_sketching", json!({}));
                }
                let open = ws.session.active_sketch == Some(id) || ws.open_sketch_in_place(id);
                if open {
                    let all: Vec<Pick> = (0..n as u32).map(|entity| Pick::SketchEntity { sketch: id, entity }).collect();
                    let _ = ws.run("select", json!({ "picks": all }));
                }
                open
            });
            if !opened {
                let _ = ws.run("select", json!({ "picks": picks, "add": shift }));
                ws.predict_tool();
            }
            ws.items_focus = true;
        }
        if resp.double_clicked() {
            ws.renaming = Some((Some(id), name.clone(), true));
        }
        y += Tokens::ROW_H;
    }
    // The dragged row follows the pointer, see-through; letting go puts it in the folder under it, or takes it out of
    // its folder anywhere else in the list.
    if let Some(item) = ws.items_dragging {
        if let (Some((name, icon)), Some(q)) = (dragged_row, pointer) {
            let ghost =
                Rect::from_min_size(pos2(r.left() + 9.0, q.y - (Tokens::ROW_H - 2.0) / 2.0), vec2(r.width() - 18.0, Tokens::ROW_H - 2.0));
            let fg = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("items-ghost")));
            icons::paint(
                &fg,
                Rect::from_center_size(pos2(r.left() + 47.5, ghost.center().y), vec2(19.0, 19.0)),
                icon,
                Color32::from_white_alpha(150),
            );
            fg.text(pos2(r.left() + 66.0, ghost.center().y), Align2::LEFT_CENTER, name, theme::body(14.0), Color32::from_white_alpha(150));
        }
        if let Some(Drop::Between { y, depth, .. }) = drop {
            let a = pos2(r.left() + 9.0 + 13.0 * depth as f32, y);
            p.line_segment([a, pos2(r.right() - 9.0, y)], Stroke::new(2.0, theme::c(0x2e68ff)));
        }
        if ui.input(|i| i.pointer.any_released()) {
            ws.items_dragging = None;
            let doc = &ws.session.doc;
            match drop {
                // The folder stays as it was (closed, as observed).
                Some(Drop::Onto(t)) if doc.folder(t).is_some() => {
                    let _ = ws.run("move_to_folder", json!({ "items": [item.0], "folder": t.0 }));
                }
                // Onto another item: a new folder holding both, where that item was (s24-023/024).
                Some(Drop::Onto(t)) => {
                    let parent = doc.folder_of(t);
                    let _ = ws.run("new_folder", json!({ "items": [item.0, t.0], "parent": parent.map(|f| f.0), "at": t.0 }));
                }
                Some(Drop::Between { folder, after, before, .. }) => {
                    let id = |i: Option<shapecraft_doc::ItemId>| i.map(|i| i.0);
                    let _ = ws.run("place_item", json!({ "item": item.0, "folder": id(folder), "after": id(after), "before": id(before) }));
                }
                None => {}
            }
        }
    }
    if ws.items_hover != row_hover {
        ws.items_hover = row_hover;
        ui.ctx().request_repaint();
    }
}

/// The Items sidebar's filter choices, as observed.
const ITEM_FILTERS: &[(&str, Icon)] = &[
    ("All Items", Icon::AllItems),
    ("Bodies", Icon::Body),
    ("Sketches", Icon::SketchItem),
    ("Planes and Axes", Icon::Plane),
    ("Images", Icon::Insert),
];

/// The Items filter and the footer's ⋯ menu.
fn items_menus(ws: &mut Workspace, ui: &mut Ui, filter: Rect, filter_clicked: bool, more: Rect, more_clicked: bool) {
    if filter_clicked {
        ws.items_menu = if ws.items_menu == Some("filter") { None } else { Some("filter") };
    }
    if more_clicked {
        ws.items_menu = if ws.items_menu == Some("more") { None } else { Some("more") };
    }
    let (clicked, rect) = match ws.items_menu {
        Some("filter") => {
            let rows: Vec<(&str, Option<Icon>, bool)> =
                ITEM_FILTERS.iter().enumerate().map(|(i, (l, icon))| (*l, Some(*icon), i == ws.items_filter)).collect();
            crate::workspace::popover::pull_down(ui, "items-filter-menu", pos2(filter.left(), filter.bottom() + 1.0), filter.width(), &rows)
        }
        Some("more") => crate::workspace::popover::pull_down(
            ui,
            "items-more-menu",
            pos2(more.left() - 4.0, more.center().y - 20.0),
            160.0,
            &[("Show Hidden Bodies", None, false), ("Invert Body Visibility", None, false)],
        ),
        _ => return,
    };
    let outside = ui.input(|i| {
        i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !filter.contains(p) && !more.contains(p))
    });
    match (ws.items_menu, clicked) {
        (Some("filter"), Some(i)) => {
            ws.items_filter = i;
            ws.items_menu = None;
        }
        (Some("more"), Some(0)) => {
            ws.show_hidden_bodies();
            ws.items_menu = None;
        }
        (Some("more"), Some(_)) => {
            ws.invert_body_visibility();
            ws.items_menu = None;
        }
        _ if outside => ws.items_menu = None,
        _ => {}
    }
}

/// A listed row: id, name, shown, kind, depth (folder levels in).
type Row = (shapecraft_doc::ItemId, String, bool, ItemKind, usize);

/// The rows of one folder (or the top level), each open folder followed by its own.
fn tree(
    doc: &shapecraft_doc::Document,
    all: &[Row],
    open: &std::collections::HashSet<shapecraft_doc::ItemId>,
    parent: Option<shapecraft_doc::ItemId>,
    depth: usize,
    out: &mut Vec<Row>,
) {
    if depth > 64 {
        return;
    }
    for row in all.iter().filter(|r| doc.folder_of(r.0) == parent) {
        out.push((row.0, row.1.clone(), row.2, row.3, depth));
        if row.3 == ItemKind::Folder && open.contains(&row.0) {
            tree(doc, all, open, Some(row.0), depth + 1, out);
        }
    }
}

/// Where a dragged Items row lands.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Drop {
    /// On a row: into a folder, or into a new folder with an item.
    Onto(shapecraft_doc::ItemId),
    /// Between two rows: in `folder`, after and before those items; the line at `y`, `depth` levels in.
    Between {
        folder: Option<shapecraft_doc::ItemId>,
        after: Option<shapecraft_doc::ItemId>,
        before: Option<shapecraft_doc::ItemId>,
        y: f32,
        depth: usize,
    },
}

/// The middle half of a row is "onto" it, the edges "between"; between rows it goes in at the level of the row above
/// (inside that row when it's an open folder), as observed (s24-025/026).
fn drop_at(doc: &shapecraft_doc::Document, rows: &[Row], item: shapecraft_doc::ItemId, top: f32, y: f32) -> Option<Drop> {
    let h = Tokens::ROW_H;
    let f = (y - top) / h;
    if f < -0.5 || rows.is_empty() {
        return None;
    }
    let k = f.floor().max(0.0) as usize;
    let frac = f - f.floor();
    if k < rows.len() && (0.25..0.75).contains(&frac) {
        let t = rows[k].0;
        return (t != item && !doc.is_within(t, item)).then_some(Drop::Onto(t));
    }
    // The gap: rows[g - 1] above, rows[g] below.
    let g = if frac >= 0.75 { k + 1 } else { k }.min(rows.len());
    let above = g.checked_sub(1).and_then(|i| rows.get(i));
    let below = rows.get(g);
    let (folder, after, depth) = match above {
        Some(u) if u.3 == ItemKind::Folder && doc.folder(u.0).is_some() && below.is_some_and(|d| d.4 > u.4) => (Some(u.0), None, u.4 + 1),
        Some(u) => (doc.folder_of(u.0), Some(u.0), u.4),
        None => (None, None, 0),
    };
    let before = below.filter(|d| doc.folder_of(d.0) == folder).map(|d| d.0);
    if folder.is_some_and(|f| doc.is_within(f, item)) || after == Some(item) || before == Some(item) {
        return None;
    }
    Some(Drop::Between { folder, after, before, y: top + g as f32 * h - 1.0, depth })
}

/// The folder whose whole contents are what's picked (a click on its row picks them), if any.
fn picked_folder(ws: &Workspace) -> Option<shapecraft_doc::ItemId> {
    let picked: std::collections::BTreeSet<u64> = ws.session.selection.iter().filter_map(|p| p.item()).map(|i| i.0).collect();
    if picked.is_empty() {
        return None;
    }
    let doc = &ws.session.doc;
    doc.folders
        .iter()
        .map(|f| f.id)
        .find(|f| doc.folder_items_deep(*f).iter().map(|i| i.0).collect::<std::collections::BTreeSet<_>>() == picked)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Body,
    Sketch,
    Plane,
    Axis,
    Image,
    Folder,
}
