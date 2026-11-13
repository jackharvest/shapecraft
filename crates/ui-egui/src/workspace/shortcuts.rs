//! The Keyboard Shortcuts window (Help ▸ Keyboard Shortcuts…), laid out like the reference's: categories on the left,
//! the category's commands on the right in grouped cards, each with its key. The list follows the reference's (read
//! from its window, 2026-10-22); commands we don't have yet are dimmed, the way the reference dims unavailable ones.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::Workspace;
use crate::theme::{self, Tokens, radius};

/// One category per block: a title line, then rows `label|keys`, a trailing `|-` marking a command we lack;
/// a blank line starts a new card.
const TABLE: &str = "\
File
New Project|⌘N
Import File…|⇧⌘I
Import Project…|⇧⌘P
Create Drawing|
Close Project|⌘W

Rename Project…|⌥⌘R

Export|⇧⌘E
Copy Screenshot to Clipboard|⇧⌘C
Open Screenshot Tool…|

Settings|⌘,
#
Edit
Undo|⌘Z
Redo|⇧⌘Z
Select All|⌘A
Deselect All|⇧⌘A
Select Through…|⇧⌘S
#
Item
Show/Hide|⇧⌘H
Isolate|
Exit Isolation|

Zoom to|
Zoom to All|
Reveal in Items Sidebar|⌥⇧I

Show Hidden Bodies|⌥H
Invert Body Visibility|⌥⇧H
Collapse All|⌥L
#
Sketch
Exit Sketching|Esc
Sketch on Face Under Pointer|Space

Line|L
Arc|A
Spline - Recent|I
Spline - Control|
Spline - Fit|
Rectangle - Recent|R
Rectangle - Center|
Rectangle - Diagonal|
Rectangle - 3 Points|
Circle|C
Ellipse|
Polygon - Recent|G
Polygon - Triangle|
Polygon - Pentagon|
Polygon - Hexagon|
Polygon - Octagon|
Offset Edge - Recent|
Offset Edge - Single|
Offset Edge - Chain|
Move/Rotate|
Mirror|
Pattern - Recent|
Pattern - Linear|
Pattern - Circular|
Text|

Trim|T

Parallel|⇧A
Perpendicular|⇧P
Tangent|⇧T
Coincident|⇧N
Midpoint|⇧M
Concentric|⇧C
Horizontal/Vertical|⇧V
Equal|⇧E
Symmetry|⇧S
Disconnect|
Lock|⇧L
Make Construction|
Make Regular||-
Construction Sketching|

Normal to Sketch|

Snap to Grid|
Snap to Sketch Guide Lines|
Snap to Sketch Guide Points|
Snap to 3D Guide Points|
Snap to Distant Edges|
Always Show Constraints|
Always Show Dimensions|
#
Insert
Variable|⇧⌘V
Image|
Import File…|⇧⌘I
Import Project…|⇧⌘P
Import Material||-
Import Image as Decal||-
#
Construct
Offset Plane|
Midplane|
Plane Through 3 Points|
Plane on Curve at Point|
Plane Perpendicular to Edge at Point||-
Plane Through Edge at Angle|
Plane Parallel to Face at Point||-
Axis Through 2 Points|
Axis Through 2 Planes||-
Axis of Cylinder/Cone|
Axis Along Edge|
Axis Perpendicular to Face at Point|
#
Transform
Move/Rotate|M
Scale - Recent|S
Scale - Uniform|
Scale - Non-uniform|
Translate|N
Pattern - Recent|
Pattern - Linear|
Pattern - Circular|
Rotate Around Axis|
Align|
Mirror|
#
Tools
Offset Face|
Chamfer/Fillet - Recent|F
Chamfer/Fillet - Auto|
Chamfer/Fillet - 2-Distance Chamfer||-
Chamfer/Fillet - Chord fillet||-
Extrude - Recent|
Extrude - One-Sided|E
Extrude - Symmetric|
Shell|H
Loft|

Union|⌘U
Subtract|⌘B
Intersect|⌘I

Split Body|
Revolve|V
Sweep|W
Replace Face||-
Offset Edge - Recent|O
Offset Edge - Chain|
Offset Edge - Single|
Project - Recent|P
Project - Sketch|
Project - Edge (Only on Faces)|
Wrap & Emboss||-

Visualize|

Section View|
Edit Section View|
2D Section View||-
Measure - Recent|
Measure - Object Measurements|
Measure - Point-to-Point Distance|
Measure - 3-Point Angle|

Sketch|
#
History
Merge Modeling Steps|⌥⇧M
#
Visualization
Snap Ground Plane to Model||-
Snap Ground Plane to XY||-
Turn Off Ground Plane||-
Capture|
Generative Render||-
Import Material||-
Import Image as Decal||-
Delete|
#
Drawings
Base View||-
Projection||-
Section View||-
Convert to Section View||-
Detail View||-
Select Custom Base View||-
Select Reference Bodies||-

Line Length|
Point-to-Point Distance|
Point-to-Line Distance|
Line-to-Line Distance|
Arc Angle|
3-Point Angle|
Line-to-Line Angle|
Radius|
Diameter|
Min-Max Distance||-

2-Point Centerline|
2-Point Circular Centerline|
3-Point Circular Centerline|
2-Line Centerline|
#
View
Drawings|⇧⌘\\
Items|⌥⌘S
History|⌥⌘H

Toggle Properties Sidebar|⌥⌘P

Default View|⌘1
Front View|⌘2
Back View|⌘3
Top View|⌘4
Bottom View|⌘5
Right View|⌘6
Left View|⌘7
Nearest Ortho View|⌘8

Rotate View Left by 15°|⌘Left
Rotate View Right by 15°|⌘Right
Rotate View Up by 15°|⌘Up
Rotate View Down by 15°|⌘Down
Rotate View Left by 45°|⇧⌘Left
Rotate View Right by 45°|⇧⌘Right
Rotate View Up by 45°|⇧⌘Up
Rotate View Down by 45°|⇧⌘Down

Wireframe|⌥W
Shaded|⌥S
Visualized|⌥V
Zebra|⌥Z
Curvature Map|⌥C
X-Ray|⌥X

Show Grid|
Show World Axes|

Show Edges|
Show Hidden Edges|
Show Decals|

Show Pinned Measurements|

Go to Modeling|

Show Guide Points|
Show Snapping Hints|
#
Help
Keyboard Shortcuts…|
Search Commands|⌘F
";

struct Row {
    label: &'static str,
    keys: &'static str,
    have: bool,
}

/// The categories, each a title and its cards of rows.
fn categories() -> Vec<(&'static str, Vec<Vec<Row>>)> {
    TABLE
        .split("#\n")
        .filter_map(|block| {
            let mut lines = block.lines();
            let title = lines.next()?.trim();
            let mut cards: Vec<Vec<Row>> = vec![Vec::new()];
            for line in lines {
                if line.trim().is_empty() {
                    cards.push(Vec::new());
                    continue;
                }
                let mut parts = line.split('|');
                let label = parts.next().unwrap_or("");
                let keys = parts.next().unwrap_or("");
                let have = parts.next() != Some("-");
                if let Some(card) = cards.last_mut() {
                    card.push(Row { label, keys, have });
                }
            }
            cards.retain(|c| !c.is_empty());
            Some((title, cards))
        })
        .collect()
}

/// Each category's square in the sidebar: its colour and the white glyph on it (as observed, 411).
fn badge(i: usize) -> (Color32, crate::icons::Icon) {
    use crate::icons::Icon as I;
    const T: [u32; 13] =
        [0x7a7a7f, 0x7a7a7f, 0xa64fd6, 0x2f5bf0, 0x2f5bf0, 0x2f5bf0, 0x2f5bf0, 0x2f5bf0, 0x30b94a, 0x7a7a7f, 0x7a7a7f, 0x7a7a7f, 0x7a7a7f];
    const G: [I; 13] = [
        I::Merged,
        I::Ellipsis,
        I::Body,
        I::Sketch,
        I::Plus,
        I::Construct,
        I::Transform,
        I::Tools,
        I::History,
        I::Visualize,
        I::DDimensions,
        I::Eye,
        I::Help,
    ];
    (theme::c(T[i % T.len()]), G[i % G.len()])
}

pub(super) fn window(ws: &mut Workspace, ui: &mut Ui) {
    let cats = categories();
    let size = vec2(800.0, 650.0);
    // Centred on the whole window, title bar included (as observed, 411).
    let at = ui.ctx().content_rect().center() - size / 2.0;
    let key = ui.scope_id().with("shortcuts-cat");
    let mut sel: usize = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0);
    let mut close = false;
    // A sheet: the window behind dims, as observed.
    egui::Area::new(ui.scope_id().with("shortcuts-dim")).fixed_pos(egui::Pos2::ZERO).order(egui::Order::Middle).show(ui.ctx(), |ui| {
        ui.painter().rect_filled(ui.ctx().content_rect(), 0.0, Color32::from_black_alpha(110));
    });
    let shown = egui::Area::new(ui.scope_id().with("shortcuts")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let p = ui.painter().clone();
        p.rect_filled(rect.translate(vec2(0.0, 6.0)), radius(12), Color32::from_black_alpha(90));
        p.rect_filled(rect, radius(11), theme::c(0x232323));
        p.rect_stroke(rect, radius(11), Stroke::new(1.0, theme::c(0x48484a)), StrokeKind::Inside);
        // Categories.
        let side = Rect::from_min_size(rect.min, vec2(236.0, rect.height()));
        p.line_segment([pos2(side.right(), side.top()), pos2(side.right(), side.bottom())], Stroke::new(1.0, theme::c(0x3a3a3c)));
        for (i, (title, _)) in cats.iter().enumerate() {
            let r = Rect::from_min_size(pos2(side.left() + 9.0, side.top() + 10.0 + 32.0 * i as f32), vec2(side.width() - 18.0, 30.0));
            let resp = ui.interact(r, ui.scope_id().with(("shortcut-cat", i)), Sense::click());
            if i == sel {
                p.rect_filled(r, radius(5), theme::c(0x2f6cf6));
            } else if resp.hovered() {
                p.rect_filled(r, radius(5), Tokens::row_hover());
            }
            let dot = Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(18.0, 18.0));
            let (fill, glyph) = badge(i);
            p.rect_filled(dot, radius(4), fill);
            crate::icons::paint(&p, dot.shrink(1.5), glyph, Color32::WHITE);
            p.text(
                pos2(r.left() + 33.0, r.center().y),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(*title),
                theme::body(13.0),
                if i == sel { Color32::WHITE } else { Tokens::text_strong() },
            );
            if resp.clicked() {
                sel = i;
            }
        }
        // The chosen category's commands.
        let body = Rect::from_min_max(pos2(side.right() + 1.0, rect.top()), pos2(rect.right(), rect.bottom() - 52.0));
        let Some((title, cards)) = cats.get(sel) else { return };
        p.text(
            pos2(body.left() + 30.0, body.top() + 30.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(*title),
            theme::strong(15.0),
            Tokens::text_strong(),
        );
        let list = Rect::from_min_max(pos2(body.left() + 22.0, body.top() + 56.0), pos2(body.right() - 18.0, body.bottom()));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        let (edits, focus) = ws.key_edits.clone().unwrap_or_default();
        let saved = ws.settings.custom_keys.clone();
        let mut new_focus: Option<Option<String>> = None;
        egui::ScrollArea::vertical().id_salt(("shortcut-list", sel)).auto_shrink([false, false]).show(&mut child, |ui| {
            let w = ui.available_width() - 6.0;
            for card in cards {
                let h = card.len() as f32 * 41.0;
                let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
                let p = ui.painter();
                p.rect_filled(r, radius(7), theme::c(0x2a2a2c));
                for (k, row) in card.iter().enumerate() {
                    let y = r.top() + 41.0 * k as f32;
                    if k > 0 {
                        p.line_segment([pos2(r.left() + 10.0, y), pos2(r.right() - 10.0, y)], Stroke::new(1.0, theme::c(0x38383a)));
                    }
                    let cy = y + 20.5;
                    let col = if row.have { Tokens::text_strong() } else { Tokens::text_faint() };
                    p.text(pos2(r.left() + 10.0, cy), Align2::LEFT_CENTER, crate::i18n::tr_s(row.label), theme::body(12.5), col);
                    let field = Rect::from_min_size(pos2(r.right() - 139.0, cy - 10.5), vec2(129.0, 21.0));
                    p.rect_filled(field, radius(5), if row.have { theme::c(0x58585c) } else { theme::c(0x3a3a3c) });
                    let kcol = if row.have { Color32::WHITE } else { Tokens::text_faint() };
                    // Pending edits, then saved customisations, then the default keys.
                    let keys = edits.iter().chain(&saved).find(|(l, _)| l == row.label).map_or(row.keys.to_string(), |(_, k)| k.clone());
                    let focused = focus.as_deref() == Some(row.label);
                    if focused {
                        p.rect_stroke(field.expand(1.0), radius(6), Stroke::new(2.5, theme::c(0x3b5bdb)), StrokeKind::Outside);
                    }
                    p.text(pos2(field.left() + 10.0, cy), Align2::LEFT_CENTER, theme::key_text(&keys), theme::body(12.0), kcol);
                    // Clicking a well makes it take the next key combination (s14-162).
                    if row.have && ui.interact(field, ui.scope_id().with(("key-well", row.label)), Sense::click()).clicked() {
                        new_focus = Some(Some(row.label.to_string()));
                    }
                }
                ui.add_space(10.0);
            }
        });
        // The focused well takes the next key with its modifiers (Escape lets go of it).
        if let Some(label) = &focus {
            let pressed = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = pressed {
                let mut edits = edits.clone();
                if key != egui::Key::Escape {
                    let combo = combo(key, m);
                    edits.retain(|(l, _)| l != label);
                    edits.push((label.clone(), combo));
                }
                ws.key_edits = Some((edits, None));
            }
        }
        if let Some(f) = new_focus {
            ws.key_edits = Some((ws.key_edits.take().map(|e| e.0).unwrap_or_default(), f));
        }
        // Footer as observed: Reset to Defaults and Save Changes light up once something can be reset or saved.
        let changed = ws.key_edits.as_ref().is_some_and(|e| !e.0.is_empty());
        let resettable = changed || !ws.settings.custom_keys.is_empty();
        let button = |r: Rect, text: &str, on: bool, id: &str| -> bool {
            let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
            p.rect_filled(
                r,
                radius(5),
                if on && resp.hovered() {
                    theme::c(0x6a6a6e)
                } else if on {
                    theme::c(0x58585c)
                } else {
                    theme::c(0x3a3a3c)
                },
            );
            p.text(r.center(), Align2::CENTER_CENTER, text, theme::body(12.0), if on { Color32::WHITE } else { Tokens::text_faint() });
            on && resp.clicked()
        };
        if button(
            Rect::from_min_size(pos2(side.right() + 16.0, rect.bottom() - 36.0), vec2(116.0, 20.0)),
            "Reset to Defaults",
            resettable,
            "keys-reset",
        ) {
            ws.settings.custom_keys.clear();
            ws.key_edits = None;
        }
        if button(
            Rect::from_min_size(pos2(rect.right() - 112.0, rect.bottom() - 36.0), vec2(96.0, 20.0)),
            "Save Changes",
            changed,
            "keys-save",
        ) {
            let edits = ws.key_edits.take().map(|e| e.0).unwrap_or_default();
            for (label, keys) in edits {
                // A key belongs to one command at a time.
                ws.settings.custom_keys.retain(|(l, k)| *l != label && *k != keys);
                ws.settings.custom_keys.push((label, keys));
            }
            close = true;
        }
        let close_r = Rect::from_min_size(pos2(rect.right() - 176.0, rect.bottom() - 36.0), vec2(56.0, 20.0));
        let resp = ui.interact(close_r, ui.scope_id().with("shortcuts-close"), Sense::click());
        p.rect_filled(close_r, radius(5), if resp.hovered() { theme::c(0x4a7ff8) } else { Tokens::accent() });
        p.text(close_r.center(), Align2::CENTER_CENTER, crate::i18n::tr("Cancel"), theme::body(12.0), Color32::WHITE);
        if resp.clicked() {
            close = true;
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, sel));
    let outside = !ws.just_opened()
        && ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !shown.response.rect.contains(p)));
    let taking_key = ws.key_edits.as_ref().is_some_and(|e| e.1.is_some());
    if close || outside || (!taking_key && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
        ws.show_shortcuts = false;
        ws.key_edits = None;
    }
}

/// A key combination as the reference writes it: ⌃⌥⇧⌘ then the key ("⌃⌥D").
pub(super) fn combo(key: egui::Key, m: egui::Modifiers) -> String {
    let mut s = String::new();
    for (on, sym) in [(m.ctrl && !m.command, "⌃"), (m.alt, "⌥"), (m.shift, "⇧"), (m.command, "⌘")] {
        if on {
            s.push_str(sym);
        }
    }
    s.push_str(key.symbol_or_name());
    s
}

/// Runs the command a customised key was given to: by its label in the command registry (as command search
/// does), else a toolbar tool of that name.
pub(super) fn run_label(ws: &mut Workspace, label: &str) {
    let plain = |t: &str| t.trim_end_matches('…').trim_end_matches("...").trim().to_lowercase();
    let want = plain(label);
    if let Some(c) = shapecraft_engine::registry().iter().find(|c| plain(c.label) == want) {
        super::panels::run_command_id(ws, c.id);
        return;
    }
    // Tools that aren't commands (Line, Extrude, Offset Plane…) start as their toolbar button would.
    if let Some(def) = super::toolbar::def_for_label(label.trim_end_matches('…').trim()) {
        ws.act(def.action, def.label);
        return;
    }
    ws.toast(format!("{label} can't take a custom key yet"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_category_has_rows() {
        let cats = super::categories();
        assert_eq!(cats.len(), 13);
        assert!(cats.iter().all(|(_, cards)| !cards.is_empty() && cards.iter().all(|c| !c.is_empty())));
        assert!(
            cats.iter().any(|(t, cards)| *t == "View" && cards.iter().flatten().any(|r| r.label == "Nearest Ortho View" && r.keys == "⌘8"))
        );
    }
}
