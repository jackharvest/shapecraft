//! The value box's variable popovers: the list behind its insert-variable button (#11), and the prompt that
//! creates a variable the box named before it existed (#12).

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use shapecraft_doc::VarKind;

use crate::theme::{self, Tokens, radius};
use crate::workspace::Workspace;

/// A dark rounded panel with a small tip pointing left at `at`, in front of everything; `body` draws into it.
fn panel<R>(ui: &mut Ui, id: &str, at: Pos2, size: egui::Vec2, body: impl FnOnce(&mut Ui, Rect) -> R) -> (R, Rect) {
    let rect = Rect::from_min_size(pos2(at.x + 12.0, at.y - 18.0), size);
    let shown =
        egui::Area::new(ui.scope_id().with(id)).fixed_pos(rect.min - vec2(8.0, 0.0)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            let (r, _) = ui.allocate_exact_size(rect.size() + vec2(8.0, 0.0), Sense::hover());
            let rect = Rect::from_min_max(pos2(r.left() + 8.0, r.top()), r.max);
            let p = ui.painter().clone();
            p.rect_filled(rect.translate(vec2(0.0, 4.0)), radius(9), Color32::from_black_alpha(70));
            let fill = theme::c(0x2e2e30);
            p.rect_filled(rect, radius(9), fill);
            p.rect_stroke(rect, radius(9), Stroke::new(1.0, theme::c(0x48484a)), StrokeKind::Inside);
            let tip = pos2(rect.left() - 7.0, rect.top() + 18.0);
            p.add(egui::Shape::convex_polygon(
                vec![tip, pos2(rect.left() + 1.0, tip.y - 8.0), pos2(rect.left() + 1.0, tip.y + 8.0)],
                fill,
                Stroke::NONE,
            ));
            body(ui, rect)
        });
    (shown.inner, shown.response.rect)
}

/// What a variable comes to, as the Variables window shows it.
fn value_text(ws: &Workspace, v: &shapecraft_doc::Variable) -> String {
    let doc = &ws.session.doc;
    match shapecraft_doc::expr::eval(&v.expr, &doc.variables) {
        Ok(x) => match v.kind {
            VarKind::Length => shapecraft_doc::format_length(x, doc.units),
            VarKind::Angle => format!("{}°", (x * 1e4).round() / 1e4),
            VarKind::Number => format!("{}", (x * 1e6).round() / 1e6),
        },
        Err(_) => "?".into(),
    }
}

/// The project's variables, name and value; a click puts the name into the value box.
pub(super) fn variable_menu(ws: &mut Workspace, ui: &mut Ui, at: Pos2) {
    if ws.numeric.is_none() || ws.session.doc.variables.is_empty() {
        ws.var_menu = None;
        return;
    }
    let rows: Vec<(String, String)> = ws.session.doc.variables.iter().map(|v| (v.name.clone(), value_text(ws, v))).collect();
    let row_h = 24.0;
    let size = vec2(220.0, 34.0 + row_h * rows.len().min(12) as f32);
    let (picked, shown) = panel(ui, "var-menu", at, size, |ui, rect| {
        let p = ui.painter().clone();
        p.text(pos2(rect.left() + 14.0, rect.top() + 16.0), Align2::LEFT_CENTER, "Variables", theme::body(12.5), Tokens::text_dim());
        let mut picked = None;
        for (i, (name, value)) in rows.iter().take(12).enumerate() {
            let row = Rect::from_min_size(pos2(rect.left() + 5.0, rect.top() + 28.0 + row_h * i as f32), vec2(rect.width() - 10.0, row_h));
            let resp = ui.interact(row, ui.scope_id().with(("var-row", i)), Sense::click());
            if resp.hovered() {
                p.rect_filled(row, radius(4), theme::c(0x2f6df5));
            }
            p.text(pos2(row.left() + 9.0, row.center().y), Align2::LEFT_CENTER, name, theme::body(13.0), Tokens::text_strong());
            let dim = if resp.hovered() { Tokens::text_strong() } else { Tokens::text_dim() };
            p.text(pos2(row.right() - 9.0, row.center().y), Align2::RIGHT_CENTER, value, theme::body(12.0), dim);
            if resp.clicked() {
                picked = Some(name.clone());
            }
        }
        picked
    });
    let outside = ws.now - ws.popup_opened_at > 0.15
        && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !shown.contains(q)));
    if let Some(name) = &picked {
        ws.insert_variable(name);
    }
    if picked.is_some() || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.var_menu = None;
    }
}

/// "New variable": its name, a field for its value (read like the value box: a bare length is in the project's
/// unit), its type, Cancel and Create. Create makes it and applies the value box.
pub(super) fn create_variable(ws: &mut Workspace, ui: &mut Ui) {
    let Some(mut vc) = ws.var_create.take() else { return };
    let size = vec2(260.0, if vc.error.is_some() { 132.0 } else { 112.0 });
    let mut create = false;
    let mut cancel = false;
    let (_, shown) = panel(ui, "var-create", vc.at, size, |ui, rect| {
        let p = ui.painter().clone();
        p.text(pos2(rect.left() + 14.0, rect.top() + 16.0), Align2::LEFT_CENTER, "New Variable", theme::body(12.5), Tokens::text_dim());
        let y = rect.top() + 44.0;
        p.text(pos2(rect.left() + 14.0, y), Align2::LEFT_CENTER, format!("{} =", vc.name), theme::body(13.0), Tokens::text_strong());
        let name_w = p.layout_no_wrap(format!("{} =", vc.name), theme::body(13.0), Color32::WHITE).size().x;
        let fx = rect.left() + 22.0 + name_w;
        let field = Rect::from_min_max(pos2(fx, y - 11.0), pos2(rect.right() - 84.0, y + 11.0));
        let hint = match vc.kind {
            VarKind::Length => format!("10 {}", ws.session.doc.units.suffix()),
            VarKind::Angle => "45°".into(),
            VarKind::Number => "4".into(),
        };
        let resp = ui.put(field, egui::TextEdit::singleline(&mut vc.value).hint_text(hint).font(theme::body(13.0)));
        if !vc.focused {
            resp.request_focus();
            vc.focused = true;
        }
        if resp.changed() {
            vc.error = None;
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            create = true;
        }
        // The type: a small pill that turns Length → Angle → Number, as in the Variables window.
        let pill = Rect::from_min_size(pos2(rect.right() - 76.0, y - 10.0), vec2(62.0, 20.0));
        let pr = ui.interact(pill, ui.scope_id().with("var-create-kind"), Sense::click());
        p.rect_filled(pill, radius(5), if pr.hovered() { theme::c(0x4a4a4e) } else { theme::c(0x3a3a3c) });
        p.text(pill.center(), Align2::CENTER_CENTER, vc.kind.label(), theme::body(11.5), Tokens::text_strong());
        if pr.clicked() {
            vc.kind = match vc.kind {
                VarKind::Length => VarKind::Angle,
                VarKind::Angle => VarKind::Number,
                VarKind::Number => VarKind::Length,
            };
            vc.error = None;
        }
        if let Some(e) = &vc.error {
            p.text(pos2(rect.left() + 14.0, y + 24.0), Align2::LEFT_CENTER, e, theme::body(11.5), Tokens::danger());
        }
        let by = rect.bottom() - 26.0;
        cancel = crate::workspace::drawings::button(
            ui,
            Rect::from_min_size(pos2(rect.right() - 160.0, by - 10.0), vec2(70.0, 20.0)),
            "Cancel",
            false,
            "var-create-cancel",
        );
        create |= crate::workspace::drawings::button(
            ui,
            Rect::from_min_size(pos2(rect.right() - 84.0, by - 10.0), vec2(70.0, 20.0)),
            "Create",
            true,
            "var-create-ok",
        );
    });
    let outside = ws.now - ws.popup_opened_at > 0.15
        && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !shown.contains(q)));
    if cancel || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        return;
    }
    if create {
        match ws.create_variable(&vc.name.clone(), vc.value.trim(), vc.kind) {
            Ok(()) => return,
            Err(e) => vc.error = Some(e),
        }
    }
    ws.var_create = Some(vc);
}
