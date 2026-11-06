//! The modeling workspace: title bar, Items and History sidebars, the viewport with its floating
//! toolbar and right-hand controls, and the interactive tools.
//!
//! This file holds the `Workspace` state, `new` and the per-frame `ui`. Its other methods are grouped by topic:
//! `actions` (toolbar and menu actions, Escape, Delete), `start_tool`, `sketch`, `camera`, `commands` (running
//! commands, live steps, the live preview) and `keys`; drawing lives in `panels` and `viewport`.

mod actions;
mod annotate;
mod area;
mod badges;
mod camera;
mod commands;
mod constraints;
mod decals;
pub(crate) mod drawings;
pub mod export;
mod history;
pub mod import_prefs;
pub mod import_project;
mod keys;
mod measure;
mod panels;
mod popover;
pub(crate) mod prefs;
mod section;
mod sheet_views;
mod shortcuts;
mod sketch;
pub mod slicers;
mod start_tool;
mod step_card;
pub mod toolbar;
mod tools;
pub mod tutorial;
mod viewport;
mod visual;

use std::sync::Arc;

use egui::{Pos2, Rect, pos2, vec2};
use glam::{DVec2, DVec3};
use serde_json::{Value, json};
use shapecraft_doc::{ItemId, Pick};

/// Extrude's automatic Result for (revision, sketch, regions): the side of the plane its body is on.
pub(crate) type ExtrudeAuto = (u64, ItemId, Vec<u32>, Option<f64>);
use shapecraft_engine::Session;
use shapecraft_kernel::Kernel;
use shapecraft_render::{Camera, StandardView};

pub(crate) use panels::select_all;
use toolbar::{Action, Category, ToolKind};
pub use tools::Tool;
pub use viewport::ViewportGpu;
pub(crate) use viewport::unmix;

use crate::theme::Tokens;

/// A camera move in progress (view changes and sketch-plane focus animate like the reference app).
#[derive(Clone, Copy)]
pub(crate) struct Anim {
    from: Camera,
    to: Camera,
    start: f64,
    duration: f64,
}

pub struct Toast {
    pub text: String,
    pub until: f64,
}

pub struct Search {
    pub query: String,
    pub at: Pos2,
    pub selected: usize,
    pub opened_frame: bool,
    /// Opened by Return with nothing to finish: it sits at the pointer (`at`), not centred (26.202, s36).
    pub at_pointer: bool,
}

/// Which right-column popover is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightPop {
    Display,
    Snap,
    Units,
    View,
}

/// The Snap popover's switches (all on by default, as observed).
#[derive(Clone, Copy, Debug)]
pub struct SnapOptions {
    pub grid: bool,
    pub guide_lines: bool,
    pub guide_points: bool,
    pub points_3d: bool,
    pub distant_edges: bool,
    pub show_guide_points: bool,
    pub hints: bool,
}

impl Default for SnapOptions {
    fn default() -> Self {
        Self {
            grid: true,
            guide_lines: true,
            guide_points: true,
            points_3d: true,
            distant_edges: true,
            show_guide_points: true,
            hints: true,
        }
    }
}

/// Viewport display options (the reference's defaults: axes and edges on).
#[derive(Clone, Copy, Debug)]
pub struct ViewOptions {
    pub grid: bool,
    pub axes: bool,
    pub edges: bool,
    pub hidden_edges: bool,
    /// Pictures placed in the scene (the Display Modes popover's Show Decals).
    pub decals: bool,
    /// View ▸ Show Pinned Measurements.
    pub pinned: bool,
    /// View ▸ Show Sketches: off (the default) hides sketches in the Visualized look only; while
    /// modeling a sketch shows by its own eye either way.
    pub sketches: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self { grid: false, axes: true, edges: true, hidden_edges: false, decals: true, pinned: true, sketches: false }
    }
}

/// The Items sidebar's width limits (measured, L07).
const ITEMS_MIN_W: f32 = 200.0;
const ITEMS_MAX_W: f32 = 500.0;

/// Keyboard Shortcuts being customised: the keys changed so far (label, keys) and the row taking a key.
pub type KeyEdits = (Vec<(String, String)>, Option<String>);

/// A Variables dialog row: name, value, the name before editing, and the kind.
pub(crate) type VariableRow = (String, String, Option<String>, shapecraft_doc::VarKind);

/// The `variants` key of Move/Rotate's Auto-orientation (1 = off).
pub(crate) const AUTO_ORIENT: &str = "Move Auto-orientation";

pub struct Workspace {
    pub session: Session,
    /// The Drawings workspace: the sheet (or why there isn't one) and the history length it was made for.
    pub drawings: Option<drawings::DrawingWs>,
    /// The workspace switcher (Modeling / Visualization / Drawings) sliding in from the left.
    pub switcher: bool,
    /// How long the last frame's 3D view took (`ui.frames`).
    pub view_ms: f32,
    /// Create Drawing's first step: pick the bodies (banner + Next / Cancel).
    pub creating_drawing: bool,
    pub drawing_prefs: Option<drawings::Prefs>,
    pub export_choice: Option<drawings::ExportChoice>,
    /// Set when the workspace wants the app to open a save dialog for a drawing.
    pub wants_drawing_export: Option<Vec<&'static str>>,
    /// The Export dialog (⇧⌘E) while it is open.
    pub export: Option<export::ExportDialog>,
    /// Import Project's sheet, and a double-clicked card asking to import at once.
    pub import_sheet: Option<import_project::ImportSheet>,
    import_go: bool,
    /// Import Preferences, open over a file waiting to come in; then the file and its "heal" to import (app.rs).
    pub import_prefs: Option<import_prefs::ImportPrefs>,
    pub import_file_go: Option<(std::path::PathBuf, serde_json::Value)>,
    /// Set when Export was pressed: the app asks where to save, then writes each format.
    pub wants_export: Option<export::ExportDialog>,
    /// The Visualization workspace: materials on, edges off.
    pub visualizing: bool,
    /// Its sidebar (in the History slot), tab, material search, the "Change" mode while something is selected, a
    /// color being dragged in the picker. The environment and its settings live in the document (`visual`).
    pub vis_sidebar: bool,
    pub vis_tab: usize,
    pub vis_search: String,
    pub vis_changing: bool,
    pub vis_draft: Option<egui::Color32>,
    /// Decals: thumbnails of the library's pictures, a picture being dragged onto a body, the decal picked on one,
    /// and whether its centre handle is being dragged.
    pub decal_thumbs: std::collections::HashMap<u64, (std::sync::Arc<shapecraft_doc::Pixels>, egui::TextureHandle)>,
    pub decal_drag: Option<ItemId>,
    pub decal_sel: Option<ItemId>,
    pub decal_handle: bool,
    /// Decal ⊕ was pressed: the app asks for a picture.
    pub wants_decal: bool,
    /// Material ▸ Custom ⊕ was clicked (the app shows the file dialog).
    pub wants_material: bool,
    /// Environment ▸ Import… was clicked (the app asks for a panorama).
    pub wants_environment: bool,
    /// Thumbnails of the custom environments' panoramas.
    pub env_thumbs: std::collections::HashMap<u64, (std::sync::Arc<shapecraft_doc::HdrPixels>, egui::TextureHandle)>,
    /// Material categories folded shut in Visualization.
    pub vis_collapsed: std::collections::HashSet<String>,
    /// A library picture being renamed in its card (id, text).
    pub picture_rename: Option<(ItemId, String)>,
    /// The selected environment's settings (its card's gear) show in the sidebar.
    pub env_settings: bool,
    /// View options from the right-click menu (and Display Modes).
    pub view_opts: ViewOptions,
    pub snap_opts: SnapOptions,
    /// The open right-column popover and the top-left of its button.
    pub(crate) right_pop: Option<(RightPop, Pos2, f64)>,
    /// Units popover: a fixed grid size instead of one that follows the zoom; angles as degrees-minutes.
    pub grid_locked: Option<f64>,
    pub degrees_fractional: bool,
    /// View ▸ Foot & Inch Format ▸ Architectural.
    pub feet_architectural: bool,
    /// The plane the floor grid lies in (0 XY, 1 YZ, 2 ZX).
    pub grid_plane: usize,
    /// The View popover's tab: 0 Views, 1 Appearance.
    pub view_tab: usize,
    /// The viewport's right-click menu: where, and what was under the pointer.
    pub context_menu: Option<(Pos2, Option<Pick>, f64)>,
    /// The Items row under the pointer (its body lights up in the viewport, next frame).
    pub items_hover: Option<Pick>,
    /// Display Modes ▸ X-Ray ▸ Opacity.
    pub xray_opacity: f32,
    /// A step card's Profile ▸ Edit...: the step, its sketch, and the regions picked so far (clicks toggle them).
    pub profile_edit: Option<(usize, ItemId, Vec<u32>)>,
    /// "Select Through This Point…": where its list opened and what's under the point; the entry hovered.
    pub(crate) through: Option<(Pos2, Vec<Pick>)>,
    pub(crate) menu_hover: Option<Pick>,
    pub camera: Camera,
    pub(crate) anim: Option<Anim>,
    pub tool: Tool,
    /// A sketch tool chosen before a plane was picked; started once there is one.
    pub(super) pending_tool: Option<ToolKind>,
    /// The Items list has focus (its last click picked a row): its selected rows are blue, else grey.
    pub(crate) items_focus: bool,
    /// An Area Selection box being dragged, and its filter (All Items, Bodies, Faces, Edges).
    pub(crate) area: Option<area::AreaSel>,
    pub(crate) area_filter: usize,
    pub hover: Option<Pick>,
    /// The pointer over the viewport, if it is there.
    pub(crate) cursor: Option<Pos2>,
    pub show_items: bool,
    pub show_history: bool,
    /// A tutorial running here (its own toolbar, no sidebars; see `tutorial.rs`).
    pub tutorial: Option<Box<tutorial::TutorialRun>>,
    pub display: shapecraft_render::DisplayMode,
    /// The Measure panel is open (it follows the selection).
    pub measuring: bool,
    /// Last Measure result, keyed by the selection and the model's history length.
    history: history::HistoryUi,
    /// The feature card (Extrude's settings, or a History step's).
    card: step_card::CardUi,
    card_popup_at: Pos2,
    section_caps: section::Caps,
    /// A constraint badge clicked in the active sketch (Delete removes it).
    pub(crate) selected_constraint: Option<(ItemId, usize)>,
    /// New sketch geometry is drawn as construction geometry.
    pub(crate) construction_mode: bool,
    /// Screenshot / Capture was asked for (the app shows the save dialog).
    pub wants_capture: bool,
    /// Insert ▸ Image was asked for (the app shows the open dialog).
    pub wants_image: bool,
    /// Offset Face shows the part's whole thickness ("Total", the reference's default) rather than the offset.
    pub(crate) offset_total: bool,
    /// A round face's Distance Type (Offset / Radius / Diameter); Diameter until another is picked (26.202, s37).
    pub(crate) offset_round: tools::OffsetKind,
    /// Linear Pattern's Distance Type: Total (true) or Spacing; kept between uses.
    pub(crate) pattern_total: bool,
    /// Where the last click hit the model: tools started from it put their handle there.
    pub(crate) last_hit: Option<DVec3>,
    /// The Offset Face "Distance Type" menu, open at this point.
    pub(crate) distance_menu: Option<Pos2>,
    /// The running tool's badge whose menu is open, and where it points.
    pub(crate) badge_menu: Option<(badges::Badge, Pos2)>,
    /// A running tool's key pressed again cycled an option: which badge, and when (the bubble shows for a while).
    pub(crate) cycled: Option<(badges::Badge, f64)>,
    /// The picked variant of each tool that has them (by label; see `toolbar::variants`).
    pub(crate) variants: std::collections::HashMap<String, usize>,
    /// Revolve's automatic Result as the engine works it out, for (revision, params): the Result menu's row.
    pub(crate) revolve_auto: std::cell::RefCell<Option<(u64, String, usize)>>,
    /// Extrude's automatic Result body for (revision, sketch, regions): which side of the sketch plane it's on.
    pub(crate) extrude_auto: std::cell::RefCell<Option<ExtrudeAuto>>,
    /// A tool's variant cascade, open beside its button.
    pub(crate) variant_menu: Option<(&'static str, Pos2)>,
    /// Where a press-drag-release sketch stroke began (screen).
    pub(crate) sketch_drag: Option<Pos2>,
    /// A selected sketch line's ↕ being dragged: (sketch, line, the elements before, press point).
    /// What the last sketch tool drew (selected afterwards, shown blue rather than orange).
    pub(crate) just_drawn: Vec<Pick>,
    /// Which elements of the sketch being edited are fully defined, for the revision and sketch it was found for.
    defined: Option<(u64, ItemId, Vec<bool>)>,
    pub(crate) side_drag: Option<(ItemId, u32, Vec<shapecraft_doc::SketchEntity>, Pos2)>,
    /// The revision a pulled tool's step left (Extrude, Offset Face, Shell, Chamfer/Fillet): pulling again amends
    /// that step, as the reference's live handles do, until another tool or selection takes over.
    pub(crate) live_step: Option<u64>,
    /// The document as it was when Extrude started: a cut shows the part whole with the red volume over it
    /// (26.202), not the hole the live step already made.
    pub(crate) cut_base: Option<shapecraft_doc::Document>,
    /// Done / Cancel this frame (Some(whether Done shows)), last frame's (what the toolbar's ✓ / ✕ cells show), and a
    /// cell's click waiting for the code that owns those buttons.
    /// The Text tool's font family (none = the plain sans-serif), kept for the next text; and its font list while
    /// open, with what's typed to narrow it.
    pub(crate) text_font: Option<String>,
    pub(crate) font_list: Option<String>,
    /// What the last frame showed, for the control channel's ui.state (parity checks): the banner's text, the left
    /// column's labels top to bottom (with their sub-labels), and the bottom row's buttons.
    pub(crate) seen_banner: String,
    pub(crate) seen_column: Vec<String>,
    pub(crate) seen_bottom: Vec<String>,
    pub(crate) confirm: Option<bool>,
    pub(crate) confirm_seen: Option<bool>,
    pub(crate) confirm_click: Option<&'static str>,
    /// The selection and revision last frame: a selection change on its own becomes an undo step.
    sel_seen: (Vec<Pick>, Option<ItemId>, u64),
    pub(crate) measure_cache: Option<(Vec<Pick>, u64, Result<Value, String>)>,
    /// An item (or the project, with `None`) being renamed in place, and the text so far.
    pub renaming: Option<(Option<ItemId>, String, bool)>,
    /// A right-click menu on an Items row.
    pub(crate) item_menu: Option<(ItemId, Pos2)>,
    /// The Items sidebar's filter (index into its choices) and which of its menus is open.
    pub(crate) items_filter: usize,
    pub(crate) items_menu: Option<&'static str>,
    pub section: Option<Section>,
    /// A sketch dimension being retyped (click its label).
    pub(crate) dim_edit: Option<viewport::DimEdit>,
    /// Isolate: only these bodies are shown (and pickable).
    pub isolated: Option<Vec<ItemId>>,
    pub show_shortcuts: bool,
    /// Keyboard Shortcuts being customised: the keys changed so far (label, keys) and the row taking a key.
    pub key_edits: Option<KeyEdits>,
    /// App preferences (copied back to the app, which saves them).
    pub settings: crate::settings::Settings,
    /// Where the in-window menus start (Linux, Windows): just after the project name in the title bar.
    pub menus_at: Pos2,
    /// A finger, not the pencil or a mouse, has the pointer (tablet): a drag on nothing in the view turns it even with
    /// a sketch tool on, and never draws a selection box.
    pub touch_drag: bool,
    /// How much wider edges, curves and snaps reach than for a mouse: more for a pencil or a finger (tablet). Above
    /// 1, what is lit up also holds on until the pointer is well away, so a tap picks what the hover showed.
    pub pick_slop: f32,
    /// The Items column's width as dragged on a tablet (not saved: the computer keeps its own).
    items_w_here: Option<f32>,
    /// The pointer was over the 3D view itself last frame, with no panel or control on top.
    pub viewport_hot: bool,
    /// Help popover, Preferences window, and the Navigation Presets modal (with the preset being looked at).
    pub help_open: bool,
    pub prefs_open: bool,
    /// Preferences: General, ShapeCraft, Plugins or Sync (0..=3).
    pub prefs_tab: usize,
    pub nav_presets: Option<&'static str>,
    /// The Variables dialog's rows being edited: name, value, and the variable's name before editing.
    pub(crate) variables_dialog: Option<Vec<VariableRow>>,
    /// When the last popup (flyout, menu, window) opened, so the click that opened it doesn't close it.
    pub(crate) popup_opened_at: f64,
    pub(crate) display_menu_at: Pos2,
    /// Digits typed while a tool is running (the reference app opens a value box as soon as you type).
    pub numeric: Option<String>,
    /// A rectangle's width, typed and fixed with Tab as observed; the next typed value is its height.
    pub(crate) rect_width: Option<f64>,
    /// A line length typed and fixed with Tab: the pointer then only aims the segment (26.200, s11-035/036).
    pub(crate) line_len: Option<f64>,
    /// The text handle being dragged (1 up arrow, 2 right arrow, 3 square, 4 turn), with where the text was and
    /// its angle when the drag began.
    pub(crate) text_drag: Option<(usize, DVec2, f64)>,
    /// Sketch Fillet's radius handle is being dragged.
    pub(crate) fillet_drag: bool,
    /// The running tool was started by the selection (a region → Extrude, an edge → Fillet…), not picked: it
    /// shows the selection pill until it changes something; a picked tool shows Cancel at once (s11-099/111).
    pub(crate) predicted: bool,
    /// Selection filter: what a click can pick ("all", "bodies", "faces" or "edges").
    pub(crate) pick_filter: &'static str,
    /// The running tool's options menu, open at this point.
    pub(crate) options_menu: Option<Pos2>,
    /// Section View waiting for its reference: a plane tile or a flat face (26.200, s11-117), and the hovered tile.
    pub(crate) section_pick: Option<Option<usize>>,
    /// Set from `numeric` when the running tool takes an angle (Revolve).
    numeric_angle: Option<f64>,
    pub search: Option<Search>,
    flyout: Option<(Category, Pos2)>,
    /// A cascade opened from the flyout (Construct ▸ Plane / Axis).
    flyout_sub: Option<(Category, Pos2)>,
    pub toasts: Vec<Toast>,
    /// A tool's refusal shown in place of the hint banner for a while (Split Body, s12-004): text and until when.
    pub notice: Option<(String, f64)>,
    pub gpu: ViewportGpu,
    /// The free area between the sidebars: toolbars, banners and pills are laid out in it.
    pub viewport_rect: Rect,
    /// Where the 3D view is drawn: the whole window under the title bar, as observed (the sidebars float over
    /// it as frosted glass, and resizing the window keeps the view centered on the window).
    pub view_rect: Rect,
    /// Contextual tools the window was too short for; the More button opens them.
    pub overflow: Vec<toolbar::ToolDef>,
    /// The Items sidebar's width while its edge is dragged (saved to the settings on release).
    items_drag: Option<f32>,
    /// Constraint Settings' popover: the gear button's top-left corner and when it opened.
    pub(super) constraint_pop: Option<(Pos2, f64)>,
    /// ⇧⌘C was pressed: copy the view to the clipboard this frame.
    copy_shot: bool,
    /// Items folders shown open, and the item being dragged in the list.
    pub(super) items_expanded: std::collections::HashSet<shapecraft_doc::ItemId>,
    pub(super) items_dragging: Option<shapecraft_doc::ItemId>,
    /// Measure: points picked in the point modes, Include X, Y, Z Deltas, and the pinned measurements.
    pub(super) measure_points: Vec<DVec3>,
    pub(super) measure_deltas: bool,
    pub(super) pinned: Vec<measure::Measured>,
    hint_collapsed: bool,
    /// Set when the user asked to go back to the projects screen.
    pub go_home: bool,
    recent_commands: Vec<&'static str>,
    /// Edit ▸ Copy: the bodies and planes copied, and the tag put on the system clipboard with them.
    pub(super) clipboard: (Vec<ItemId>, String),
    /// Text waiting to go onto the system clipboard (an action without the egui context asked for it).
    pub(super) clip_out: Option<String>,
    /// A window of the app's own (the first-run questions) is over the workspace: keys aren't ours.
    pub keys_blocked: bool,
    pub(crate) now: f64,
    /// What the running tool would produce, shown in place while you drag or type (live geometry).
    pub(crate) preview: Option<Preview>,
}

/// Section View: a cut through `center + normal * offset`, removing the side `normal` points to.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub center: DVec3,
    pub normal: DVec3,
    pub offset: f64,
    pub dragging: Option<(Pos2, f64)>,
    /// Just placed or reopened: arrows and the Delete Section / 2D Section View / Section Only column (s16-021).
    pub editing: bool,
    /// Section Only: just the cut faces (s16-023). 2D Section View: those, square on, in wireframe (s16-024).
    pub only: bool,
    pub flat: Option<shapecraft_render::DisplayMode>,
    /// A rotate handle being dragged: which (0, 1), where the drag began, the normal then.
    pub turning: Option<(usize, Pos2, DVec3)>,
}

impl Section {
    pub fn new(center: DVec3, normal: DVec3) -> Self {
        Self { center, normal, offset: 0.0, dragging: None, editing: true, only: false, flat: None, turning: None }
    }

    /// The two in-plane directions the rotate handles sit along (each turns the cut about the other).
    pub fn axes(&self) -> (DVec3, DVec3) {
        let n = self.normal.normalize_or(DVec3::Z);
        let u = if n.z.abs() < 0.9 { DVec3::Z.cross(n).normalize() } else { DVec3::X.cross(n).normalize() };
        (u, n.cross(u))
    }

    /// Only the cut faces show (Section Only, or 2D Section View).
    pub fn caps_only(&self) -> bool {
        self.only || self.flat.is_some()
    }

    pub fn origin(&self) -> DVec3 {
        self.center + self.normal * self.offset
    }
}

pub(crate) struct Preview {
    key: String,
    computed_at: f64,
    pub doc: Option<shapecraft_doc::Document>,
}

impl Workspace {
    /// The Distance Type Offset Face uses for this face: a round face's own, else Total or Offset.
    /// The menu bar's check marks (menubar::check_bit): sidebars, view options, display mode, units and formats.
    pub(crate) fn menu_checks(&self) -> u32 {
        use crate::menubar::{MenuAction as M, check_bit};
        use shapecraft_doc::Units;
        use toolbar::UiAction;
        let o = &self.view_opts;
        let units = match self.session.doc.units {
            Units::Millimeter => 0,
            Units::Centimeter => 1,
            Units::Meter => 2,
            Units::Inch => 3,
            Units::Foot => 4,
        };
        let on = [
            (M::Ui(UiAction::ToggleHistory), self.show_history),
            (M::Ui(UiAction::ToggleItems), self.show_items),
            (M::Ui(UiAction::ToggleGrid), o.grid),
            (M::Ui(UiAction::ToggleAxes), o.axes),
            (M::Ui(UiAction::ToggleEdges), o.edges),
            (M::Ui(UiAction::ToggleSketches), o.sketches),
            (M::Ui(UiAction::ToggleHiddenEdges), o.hidden_edges),
            (M::Ui(UiAction::ToggleDecals), o.decals),
            (M::Ui(UiAction::TogglePinned), o.pinned),
            (M::Display(self.display), true),
            (M::Ui(UiAction::Units(units)), true),
            (M::Ui(UiAction::FeetArchitectural(self.feet_architectural)), true),
            (M::Ui(UiAction::DegreesFractional(self.degrees_fractional)), true),
        ];
        on.iter().filter(|(_, v)| *v).filter_map(|(a, _)| check_bit(a)).fold(0, |acc, b| acc | (1 << b))
    }

    /// Ends a running modeling tool, keeping a result it already applied (as Done does). Sketch tools and plane
    /// picking stay.
    pub(crate) fn end_model_tool(&mut self) {
        if matches!(self.tool, tools::Tool::Idle | tools::Tool::PickPlane { .. }) || self.tool.is_sketch_tool() {
            return;
        }
        self.live_step = None;
        self.numeric = None;
        self.preview = None;
        self.predicted = false;
        self.tool = tools::Tool::Idle;
    }

    /// Done on a live result: the tool ends and, outside sketches, nothing stays picked (Offset Face, Fillet, Move on
    /// a face or a body, Extrude alike).
    pub(crate) fn finish_live(&mut self) {
        let live = self.is_live();
        // Sketch curves moved with the gizmo let go too.
        let sketch_move = matches!(&self.tool, tools::Tool::Move(t) if t.items.is_empty() && t.faces.is_none() && t.edge.is_none());
        self.live_step = None;
        self.numeric = None;
        self.preview = None;
        self.predicted = false;
        self.tool = tools::Tool::Idle;
        if live && (self.session.active_sketch.is_none() || sketch_move) {
            self.session.selection.clear();
        }
    }

    /// A body made of a single face (a sphere, a closed free-form shape).
    pub(crate) fn one_face_body(&self, body: shapecraft_doc::ItemId) -> bool {
        self.session.doc.body(body).is_some_and(|b| self.session.kernel.face_count(&b.shape).is_ok_and(|n| n == 1))
    }

    pub(crate) fn offset_kind(&self, t: &tools::OffsetFaceTool) -> tools::OffsetKind {
        if !t.kinds {
            return tools::OffsetKind::Offset;
        }
        match (t.radius.is_some(), self.offset_total) {
            (true, _) => self.offset_round,
            (false, true) => tools::OffsetKind::Total,
            (false, false) => tools::OffsetKind::Offset,
        }
    }

    pub fn new(session: Session, settings: crate::settings::Settings) -> Self {
        let mut camera = Camera::default();
        if let Some((min, max)) = session.doc.bounds() {
            camera.fit(min, max);
            camera = Camera::standard(StandardView::Default, camera.target, camera.distance);
        }
        settings.apply_view(&mut camera);
        Self {
            flyout_sub: None,
            drawings: None,
            switcher: false,
            view_ms: 0.0,
            creating_drawing: false,
            drawing_prefs: None,
            export_choice: None,
            wants_drawing_export: None,
            export: None,
            import_sheet: None,
            import_go: false,
            import_prefs: None,
            import_file_go: None,
            wants_export: None,
            visualizing: false,
            vis_sidebar: true,
            vis_tab: 0,
            vis_search: String::new(),
            vis_changing: false,
            vis_draft: None,
            decal_thumbs: std::collections::HashMap::new(),
            decal_drag: None,
            decal_sel: None,
            decal_handle: false,
            wants_decal: false,
            wants_material: false,
            wants_environment: false,
            env_thumbs: std::collections::HashMap::new(),
            vis_collapsed: Default::default(),
            picture_rename: None,
            env_settings: false,
            items_hover: None,
            xray_opacity: 0.28,
            profile_edit: None,
            through: None,
            menu_hover: None,
            view_opts: ViewOptions::default(),
            snap_opts: SnapOptions::default(),
            right_pop: None,
            grid_locked: None,
            degrees_fractional: false,
            feet_architectural: false,
            grid_plane: 0,
            view_tab: 0,
            context_menu: None,
            session,
            camera,
            anim: None,
            tool: Tool::Idle,
            pending_tool: None,
            items_focus: false,
            area: None,
            area_filter: 0,
            hover: None,
            cursor: None,
            show_items: true,
            // A tablet opens projects with History hidden, for room to model (the button brings it back).
            show_history: !crate::tablet::on_tablet(),
            tutorial: None,
            display: Default::default(),
            measuring: false,
            history: history::HistoryUi::default(),
            card: step_card::CardUi::default(),
            card_popup_at: Pos2::ZERO,
            section_caps: section::Caps::default(),
            selected_constraint: None,
            construction_mode: false,
            wants_capture: false,
            wants_image: false,
            offset_total: true,
            offset_round: tools::OffsetKind::Diameter,
            pattern_total: false,
            last_hit: None,
            distance_menu: None,
            badge_menu: None,
            cycled: None,
            variants: settings.tool_variants.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            revolve_auto: Default::default(),
            extrude_auto: Default::default(),
            variant_menu: None,
            sketch_drag: None,
            side_drag: None,
            just_drawn: Vec::new(),
            defined: None,
            live_step: None,
            cut_base: None,
            text_font: None,
            font_list: None,
            seen_banner: String::new(),
            seen_column: Vec::new(),
            seen_bottom: Vec::new(),
            confirm: None,
            confirm_seen: None,
            confirm_click: None,
            sel_seen: (Vec::new(), None, 0),
            measure_cache: None,
            renaming: None,
            item_menu: None,
            items_filter: 0,
            items_menu: None,
            section: None,
            dim_edit: None,
            isolated: None,
            show_shortcuts: false,
            key_edits: None,
            settings,
            help_open: false,
            prefs_open: false,
            menus_at: Pos2::ZERO,
            touch_drag: false,
            pick_slop: 1.0,
            items_w_here: None,
            viewport_hot: false,
            prefs_tab: 0,
            nav_presets: None,
            variables_dialog: None,
            popup_opened_at: -1.0,
            display_menu_at: Pos2::ZERO,
            numeric: None,
            rect_width: None,
            line_len: None,
            text_drag: None,
            fillet_drag: false,
            predicted: false,
            pick_filter: "all",
            options_menu: None,
            section_pick: None,
            numeric_angle: None,
            search: None,
            flyout: None,
            toasts: Vec::new(),
            notice: None,
            gpu: ViewportGpu::default(),
            viewport_rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            view_rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            overflow: Vec::new(),
            items_drag: None,
            constraint_pop: None,
            copy_shot: false,
            items_expanded: std::collections::HashSet::new(),
            items_dragging: None,
            measure_points: Vec::new(),
            measure_deltas: false,
            pinned: Vec::new(),
            hint_collapsed: false,
            go_home: false,
            recent_commands: vec!["extrude", "rectangle"],
            clipboard: (Vec::new(), String::new()),
            clip_out: None,
            keys_blocked: false,
            now: 0.0,
            preview: None,
        }
    }

    pub fn kernel(&self) -> Arc<dyn Kernel> {
        self.session.kernel.clone()
    }

    /// Drawn the Visualize way: in the Visualize workspace, or with the Visualized display mode while modeling.
    pub(crate) fn visualized_look(&self) -> bool {
        self.visualizing || self.display == shapecraft_render::DisplayMode::Visualized
    }

    /// View ▸ Visualized (⌥V): a display mode: the left column, History and every tool stay
    /// as they are, only the look changes (and "Visualized" shows briefly). Again goes back to Shaded.
    pub(crate) fn toggle_visualized(&mut self) {
        use shapecraft_render::DisplayMode as D;
        self.display = if self.display == D::Visualized { D::Shaded } else { D::Visualized };
        if self.display == D::Visualized {
            // Briefly in the hint banner's place, as a small pill.
            self.notice = Some(("Visualized".into(), self.now + 1.5));
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast { text: text.into(), until: self.now + 3.0 });
    }

    /// Move/Rotate's Auto-orientation (on unless switched off; remembered like a variant).
    pub(crate) fn auto_orient(&self) -> bool {
        self.variants.get(AUTO_ORIENT) != Some(&1)
    }

    /// Copies the picked tool variants into the settings (saved with them), so they outlast the project and the run.
    pub fn keep_variants(&mut self) {
        let kept = &self.settings.tool_variants;
        if self.variants.len() != kept.len() || self.variants.iter().any(|(k, v)| kept.get(k) != Some(v)) {
            self.settings.tool_variants = self.variants.iter().map(|(k, v)| (k.clone(), *v)).collect();
        }
    }

    pub(crate) fn variant(&self, label: &str) -> usize {
        let (names, default) = toolbar::variants(label);
        self.variants.get(label).copied().filter(|i| *i < names.len()).unwrap_or(default)
    }

    /// A toolbar button as the picked variant shows it: its sub-label, and for Pattern the tool it starts.
    pub(crate) fn with_variant(&self, d: toolbar::ToolDef) -> toolbar::ToolDef {
        let (names, _) = toolbar::variants(d.label);
        // A region's Offset Edge is always a chain (its sub-label says so), as observed.
        // Buttons without a cascade corner (the 26.200 sketch column's Scale, Offset Edge, Patterns) show no variant.
        if names.is_empty() || d.sub == Some("Chain") || !d.flyout {
            return d;
        }
        let i = self.variant(d.label);
        // The sketch toolset's Pattern keeps its own 2D icon for the linear variant.
        let icon = if d.icon == crate::icons::Icon::SketchPattern && i == 0 {
            d.icon
        } else {
            toolbar::variant_icon(d.label, i).unwrap_or(d.icon)
        };
        let mut out = toolbar::ToolDef { sub: names.get(i).copied(), icon, ..d };
        if d.label == "Pattern" {
            out.action = Action::Tool(if i == 1 { ToolKind::CircularPattern } else { ToolKind::Pattern });
        }
        if d.label == "Scale" {
            out.action = Action::Tool(if i == 0 { ToolKind::Scale } else { ToolKind::ScaleNonUniform });
        }
        out
    }

    /// A modeling tool you picked yourself that changes the model in place: Cancel shows at the bottom from the
    /// start (26.200, s11-111).
    pub(super) fn picked_modify_tool(&self) -> bool {
        !self.predicted
            && self.session.active_sketch.is_none()
            && matches!(
                self.tool,
                Tool::Extrude(_)
                    | Tool::Fillet(_)
                    | Tool::Shell(_)
                    | Tool::OffsetFace(_)
                    | Tool::Revolve(_)
                    | Tool::RotateAxis(_)
                    | Tool::Pattern(_)
                    | Tool::CircularPattern(_)
                    | Tool::Scale(_)
                    | Tool::Loft(_)
                    | Tool::Sweep(_)
                    | Tool::BodyEdgeOffset(_)
                    | Tool::OffsetPlane(_)
                    | Tool::Boolean(_)
                    | Tool::Mirror(_)
                    | Tool::Translate { .. }
                    | Tool::Align { .. }
                    | Tool::Onto(_)
                    | Tool::Move(_)
            )
    }

    /// Places a picture on the selected planar face or construction plane (the top plane otherwise), sized to
    /// about half the view.
    pub fn insert_image(&mut self, path: &std::path::Path) -> Option<Value> {
        let mut params = match self.session.selection.first() {
            Some(&Pick::Face { body, face }) => json!({ "body": body.0, "face": face }),
            Some(&Pick::Plane { plane }) => json!({ "construction": plane.0 }),
            _ => json!({ "plane": "xy" }),
        };
        params["path"] = json!(path);
        params["width"] = json!((self.camera.distance * 0.5).clamp(1.0, 1e5).round());
        let out = self.run("insert_image", params)?;
        self.show_items = true;
        Some(out)
    }

    /// Selections step back with ⌘Z in the reference: a change of selection made while no tool runs (and no
    /// command ran) is recorded as an undo step of its own. Changes a command makes are part of its step.
    fn note_selection(&mut self) {
        let (seen, sketch, rev) = std::mem::take(&mut self.sel_seen);
        let quiet = matches!(self.tool, Tool::Idle) && self.area.is_none();
        let changed = seen != self.session.selection || sketch != self.session.active_sketch;
        if rev == self.session.revision && changed && quiet {
            self.session.record_selection(seen, sketch);
        }
        self.sel_seen = (self.session.selection.clone(), self.session.active_sketch, self.session.revision);
    }

    /// Shows the History sidebar with a step's parameters open (control channel).
    /// Whether the (single) selected face is flat.
    pub(crate) fn selected_face_is_flat(&self) -> bool {
        match self.session.selection.as_slice() {
            [Pick::Face { body, face }] => {
                self.session.doc.body(*body).is_none_or(|b| self.session.kernel.face_plane(&b.shape, *face).ok().flatten().is_some())
            }
            _ => true,
        }
    }

    pub(crate) fn open_history_step(&mut self, step: usize) {
        self.show_history = true;
        self.history.open = Some(step);
        self.history.editing = None;
    }

    pub fn active_sketch_name(&self) -> Option<String> {
        self.session.active_sketch().map(|s| s.name.clone())
    }

    /// The blurred view for the frosted sidebars and the rect it was rendered for.
    pub(super) fn glass(&self) -> Option<(egui::TextureId, Rect)> {
        self.gpu.glass.map(|t| (t, self.view_rect))
    }

    pub fn item_selected(&self, id: ItemId) -> bool {
        self.session.selection.iter().any(|p| p.item() == Some(id))
    }

    /// One frame of the workspace.
    pub fn ui(&mut self, ui: &mut egui::Ui, render_state: Option<&egui_wgpu::RenderState>) {
        shapecraft_doc::set_display_formats(self.feet_architectural, self.degrees_fractional);
        self.now = ui.input(|i| i.time);
        self.note_selection();
        if self.step_animation() {
            ui.ctx().request_repaint();
        }
        self.toasts.retain(|t| t.until > self.now);
        if self.notice.as_ref().is_some_and(|(_, until)| *until <= self.now) {
            self.notice = None;
        }
        if !self.toasts.is_empty() || self.notice.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, Tokens::window());
        let title = Rect::from_min_size(full.min, vec2(full.width(), Tokens::TITLEBAR_H));
        let body_top = title.bottom();
        let gap = Tokens::GAP;
        // The Drawings workspace hides the Items sidebar (as observed).
        // A tablet keeps the Items column at its narrowest for room to model; a drag there stays on the tablet.
        let saved = if crate::tablet::on_tablet() { self.items_w_here.unwrap_or(ITEMS_MIN_W) } else { self.settings.items_width };
        let items_w = self.items_drag.unwrap_or(saved);
        let items_w = if items_w.is_finite() { items_w.clamp(ITEMS_MIN_W, ITEMS_MAX_W) } else { Tokens::SIDEBAR_W };
        let teaching = self.tutorial.is_some();
        if teaching {
            self.tutorial_tick();
        }
        let items_rect = (self.show_items && self.drawings.is_none() && !teaching)
            .then(|| Rect::from_min_max(pos2(full.left() + gap, body_top + 8.0), pos2(full.left() + gap + items_w, full.bottom() - 8.0)));
        let side_open = match &self.drawings {
            Some(d) => d.props,
            None if self.visualizing => self.vis_sidebar,
            None => self.show_history && !teaching,
        };
        let history_rect = side_open.then(|| {
            Rect::from_min_max(pos2(full.right() - gap - Tokens::HISTORY_W, body_top + 8.0), pos2(full.right() - gap, full.bottom() - 8.0))
        });
        let vp_left = items_rect.map_or(full.left(), |r| r.right() + 3.0);
        let vp_right = history_rect.map_or(full.right(), |r| r.left() - 3.0);
        self.viewport_rect = Rect::from_min_max(pos2(vp_left, body_top), pos2(vp_right, full.bottom()));
        self.view_rect = Rect::from_min_max(pos2(full.left(), body_top), full.max);

        self.update_preview();
        // Viewport first so the floating controls drawn after it win the hit test.
        let began = std::time::Instant::now();
        self.viewport(ui, render_state);
        self.view_ms = began.elapsed().as_secs_f32() * 1000.0;
        if teaching {
            self.tutorial_controls(ui);
            self.tutorial_cube(ui);
        } else {
            self.floating_controls(ui);
        }
        panels::titlebar(self, ui, title);
        if let Some(r) = items_rect {
            panels::items_sidebar(self, ui, r);
            // Its right edge resizes it.
            let edge = Rect::from_min_max(pos2(r.right() - 3.0, r.top()), pos2(r.right() + 4.0, r.bottom()));
            let resp = ui.interact(edge, ui.scope_id().with("items-edge"), egui::Sense::drag());
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if resp.dragged()
                && let Some(p) = resp.interact_pointer_pos()
            {
                self.items_drag = Some((p.x - r.left()).clamp(ITEMS_MIN_W, ITEMS_MAX_W));
            }
            if resp.drag_stopped()
                && let Some(w) = self.items_drag.take()
            {
                if crate::tablet::on_tablet() {
                    self.items_w_here = Some(w.round());
                } else {
                    self.settings.items_width = w.round();
                }
            }
        }
        if let Some(r) = history_rect {
            if self.drawings.is_some() {
                drawings::properties(self, ui, r);
            } else if self.visualizing {
                if self.session.selection.is_empty() {
                    self.vis_changing = false;
                }
                visual::sidebar(self, ui, r);
            } else {
                history::sidebar(self, ui, r);
            }
        }
        step_card::show(self, ui);
        panels::popups(self, ui);
        if self.switcher {
            drawings::switcher(self, ui, full);
        }
        if self.drawing_prefs.is_some()
            || self.export_choice.is_some()
            || self.export.is_some()
            || self.import_sheet.is_some()
            || self.import_prefs.is_some()
            || self.prefs_open
            || self.nav_presets.is_some()
            || self.variables_dialog.is_some()
        {
            // Modals sit above everything, the 3D view included, and take every click.
            egui::Area::new(ui.scope_id().with("modal-layer")).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
                let _ = ui.allocate_rect(full, egui::Sense::click());
                if self.drawing_prefs.is_some() {
                    drawings::prefs_dialog(self, ui, full);
                }
                if self.export_choice.is_some() {
                    drawings::export_dialog(self, ui, full);
                }
                if self.export.is_some() {
                    export::dialog(self, ui, full);
                }
                if self.import_sheet.is_some() {
                    import_project::dialog(self, ui, full);
                }
                if self.import_prefs.is_some() {
                    import_prefs::dialog(self, ui, full);
                }
                prefs::modals(self, ui, full);
            });
        }
        if self.help_open {
            prefs::help(self, ui, full);
        }
        self.keyboard(ui);
        if std::mem::take(&mut self.copy_shot) {
            match self.copy_screenshot(ui.ctx()) {
                Ok(()) => self.toast("Screenshot copied"),
                Err(e) => self.toast(format!("Couldn't copy the view: {e}")),
            }
        }
    }

    /// A click outside a popup closes it, except the click that just opened it.
    pub(crate) fn just_opened(&self) -> bool {
        self.now - self.popup_opened_at < 1e-6
    }
}
