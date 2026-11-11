//! Title bar, sidebars, the floating toolbar and right-hand column, and the HUD pieces.
//!
//! This file holds the frosted-glass `panel` every sidebar sits on. The pieces live in:
//! - `titlebar`: the title bar.
//! - `items`: the Items sidebar.
//! - `left_toolbar`: the floating toolbar column on the left and which tools it shows.
//! - `right_column`: the right-hand column and the view cube.
//! - `right_popover`: the right column's popovers and Constraint Settings.
//! - `popups`: everything drawn over the viewport each frame (hint banner, menus, toasts).
//! - `context_menu`: the right-click menu, Select Through, and the variant and distance menus.
//! - `command_search`: ⌘F command search and running a command by id.

use egui::{Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2};

use crate::theme::{Tokens, radius};

mod command_search;
mod context_menu;
mod items;
mod left_toolbar;
mod popups;
mod right_column;
mod right_popover;
mod titlebar;
mod variables;

pub(super) use command_search::run_command_id;
pub(crate) use command_search::select_all;
pub(super) use items::items_sidebar;
pub(super) use popups::popups;
pub(super) use titlebar::{titlebar, touch_title};

/// How much of the blurred view shows through a sidebar; the rest is the tint. Measured: over the empty view a
/// sidebar is PANEL, over a light grey body it brightens by about a third of the difference.
const GLASS: f32 = 0.35;

/// A sidebar: frosted glass over the 3D view (`glass` is the blurred view and the rect it covers), and it takes
/// the pointer so clicks and scrolls on it never reach the view underneath.
pub(super) fn panel(ui: &Ui, r: Rect, glass: Option<(egui::TextureId, Rect)>) {
    let _ = ui.interact(r, ui.scope_id().with(("panel", r.left() as i32)), Sense::click_and_drag());
    // A soft shadow around it: 13 levels darker at the edge, gone 22 px out over the light view (s23, L-idle);
    // the same shadow is a level or two over the dark one.
    let shadow = egui::epaint::Shadow { offset: [0, 0], blur: 22, spread: 0, color: Color32::from_black_alpha(30) };
    ui.painter().add(shadow.as_shape(r, radius(Tokens::PANEL_RADIUS)));
    let fill = match glass {
        Some((tex, view)) if view.width() > 1.0 && view.height() > 1.0 => {
            let uv = Rect::from_min_max(
                pos2((r.left() - view.left()) / view.width(), (r.top() - view.top()) / view.height()),
                pos2((r.right() - view.left()) / view.width(), (r.bottom() - view.top()) / view.height()),
            );
            ui.painter().add(egui::epaint::RectShape::filled(r, radius(Tokens::PANEL_RADIUS), Color32::WHITE).with_texture(tex, uv));
            // The tint that gives exactly PANEL over the empty viewport colour.
            let mix = |p: u8, v: u8| ((p as f32 - GLASS * v as f32) / (1.0 - GLASS)).round().clamp(0.0, 255.0) as u8;
            let (p, v) = (Tokens::panel(), Tokens::viewport());
            Color32::from_rgba_unmultiplied(mix(p.r(), v.r()), mix(p.g(), v.g()), mix(p.b(), v.b()), ((1.0 - GLASS) * 255.0).round() as u8)
        }
        _ => Tokens::panel(),
    };
    ui.painter().rect_filled(r, radius(Tokens::PANEL_RADIUS), fill);
    ui.painter().rect_stroke(r, radius(Tokens::PANEL_RADIUS), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
}
