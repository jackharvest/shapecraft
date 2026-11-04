//! The floating left toolbar column: which tools show for the selection or running tool, which one is lit,
//! and drawing the column.

use egui::{Color32, Rect, Stroke, Ui, pos2, vec2};
use shapecraft_doc::Pick;
use shapecraft_engine::SelectionKind;

use crate::icons::{self, Icon};
use crate::theme::{Tokens, radius};
use crate::workspace::Workspace;
use crate::workspace::toolbar::{self, Action, ToolDef, ToolKind, UiAction};
use crate::workspace::tools::Tool;

const EXIT_SKETCHING: ToolDef =
    ToolDef { icon: Icon::Close, label: "Exit Sketching", sub: None, keys: "", action: Action::Ui(UiAction::ExitSketching), flyout: false };

/// The contextual middle group of the toolbar, and the run-time sub-label of its first button.
fn context_tools(ws: &Workspace) -> (Vec<ToolDef>, Option<String>) {
    let mut v = Vec::new();
    if let Some(d) = &ws.drawings {
        return match &d.view_tool {
            Some(t) if t.has_done() => (toolbar::VIEW_DONE.to_vec(), None),
            Some(_) => (toolbar::VIEW_CANCEL.to_vec(), None),
            None => (toolbar::DRAWINGS.to_vec(), None),
        };
    }
    if ws.creating_drawing {
        return (toolbar::CREATE_DRAWING.to_vec(), None);
    }
    if ws.visualizing {
        return (Vec::new(), None);
    }
    if ws.session.active_sketch.is_some() && !ws.session.selection.iter().any(|p| matches!(p, Pick::Region { .. })) {
        v.push(EXIT_SKETCHING);
        v.extend_from_slice(toolbar::SKETCHING);
        return (v, ws.active_sketch_name());
    }
    // Offset Edge on a region outside a sketch makes a new sketch: the sketch column, no plane yet (s11-075).
    if matches!(ws.tool, Tool::PickPlane { .. }) || ws.sketch_column_no_plane() {
        v.push(EXIT_SKETCHING);
        v.extend_from_slice(toolbar::SKETCHING);
        return (v, Some("No active plane".into()));
    }
    if ws.idle_column() {
        return (toolbar::IDLE.to_vec(), None);
    }
    let many = ws.session.selection.len() > 1;
    let set: &[ToolDef] = match ws.session.selection_kind() {
        SelectionKind::Nothing => return (toolbar::IDLE.to_vec(), None),
        SelectionKind::Regions => toolbar::REGION,
        SelectionKind::Faces if many => toolbar::FACES,
        SelectionKind::Faces if !ws.selected_face_is_flat() => toolbar::CURVED_FACE,
        SelectionKind::Faces => toolbar::FACE,
        SelectionKind::Edges if many => toolbar::EDGES,
        SelectionKind::Edges => toolbar::EDGE,
        SelectionKind::Bodies => toolbar::BODY,
        SelectionKind::Mixed => toolbar::MIXED,
        SelectionKind::SketchEntities => toolbar::SKETCHING,
        SelectionKind::Planes => toolbar::PLANE,
        SelectionKind::Axes | SelectionKind::Images => &[],
    };
    v.push(toolbar::SELECTION_HEAD);
    v.extend_from_slice(set);
    // Faces that aren't all flat can't be extruded, lofted, revolved or swept (s24 matrix: Shell on a cylinder).
    if many && ws.session.selection_kind() == SelectionKind::Faces {
        let curved = ws.session.selection.iter().any(|p| match *p {
            Pick::Face { body, face } => {
                ws.session.doc.body(body).is_some_and(|b| ws.session.kernel.face_plane(&b.shape, face).ok().flatten().is_none())
            }
            _ => false,
        });
        if curved {
            v.retain(|d| !matches!(d.label, "Extrude" | "Loft" | "Revolve" | "Sweep"));
        }
    }
    // A round edge has no line to put an axis along or a plane through (s24 matrix).
    if let [Pick::Edge { body, edge }] = ws.session.selection.as_slice()
        && let Some(b) = ws.session.doc.body(*body)
        && !matches!(ws.session.kernel.edge_curve(&b.shape, *edge), Ok(shapecraft_kernel::EdgeCurve::Line { .. }))
    {
        v.retain(|d| !matches!(d.label, "Axis - Along Edge" | "Plane - Through Edge"));
    }
    (v, ws.session.selection_summary())
}

/// The running tool's options, shown when its button is clicked again (behind its ≡ in 26.200): (option, on).
pub(super) fn tool_options(ws: &Workspace) -> Vec<(ToolDef, bool)> {
    match &ws.tool {
        Tool::Mirror(t) if ws.session.active_sketch.is_none() => vec![(toolbar::KEEP_ORIGINALS, t.keep)],
        Tool::Boolean(t) => {
            vec![(if t.op == "subtract" { toolbar::KEEP_ORIGINALS_OF } else { toolbar::KEEP_ORIGINALS }, t.keep_label() != "Off")]
        }
        Tool::Translate { copy, .. } => vec![(toolbar::COPY_OPTION, *copy)],
        // Bodies only: sketch curves keep their sketch's frame.
        Tool::Move(t) if !t.items.is_empty() => vec![(toolbar::AUTO_ORIENT_OPTION, ws.auto_orient())],
        Tool::Loft(t) => vec![(toolbar::RULED_OPTION, t.ruled)],
        Tool::CircularPattern(t) if !t.items.is_empty() => vec![(toolbar::UNIFORM_OPTION, t.uniform)],
        Tool::Pattern(_) => vec![(toolbar::PATTERN_TYPE, false)],
        Tool::BodyEdgeOffset(_) => vec![(toolbar::OFFSET_EDGE_TYPE, false)],
        Tool::Onto(t) if !t.project => vec![(toolbar::FLIP_ALIGNMENT, t.flip)],
        // Project as Edge's card has only its two pick rows (s11-137).
        Tool::Onto(_) => vec![(toolbar::ITEMS_TO_PROJECT, false)],
        _ => Vec::new(),
    }
}

/// While a tool waits for more picks, the toolbar's middle is only its option(s), then Done once there's something
/// to apply, then Cancel (as observed, 324–334). `None` for tools that keep the full set.
pub(super) fn tool_mode(ws: &Workspace) -> Option<Vec<ToolDef>> {
    // Measure takes the column: Exit Measure and the mode (as observed, L24).
    if ws.measuring && matches!(ws.tool, Tool::Idle) && ws.drawings.is_none() && !ws.visualizing {
        let names = toolbar::variants("Measure mode").0;
        let i = ws.variant("Measure mode");
        let mode = ToolDef {
            sub: names.get(i).copied(),
            icon: toolbar::variant_icon("Measure mode", i).unwrap_or(Icon::Measure),
            ..toolbar::MEASURE_MODE
        };
        return Some(vec![toolbar::EXIT_MEASURE, mode]);
    }
    let on_off = |def: ToolDef, on: bool| ToolDef { sub: Some(if on { "On" } else { "Off" }), ..def };
    if ws.sketch_transform().is_some() {
        return None;
    }
    // Section View picking its reference: nothing in the middle (s11-117).
    if ws.section_pick.is_some() {
        return Some(Vec::new());
    }
    if let Some(s) = ws.section.filter(|s| s.editing && matches!(ws.tool, Tool::Idle)) {
        return Some(vec![toolbar::DELETE_SECTION, on_off(toolbar::SECTION_2D, s.flat.is_some()), on_off(toolbar::SECTION_ONLY, s.only)]);
    }
    // 26.200 keeps the toolset for these too: options behind the running tool's ≡, Cancel / Done at the bottom.
    if matches!(
        ws.tool,
        Tool::Pattern(_)
            | Tool::Mirror(_)
            | Tool::BodyEdgeOffset(_)
            | Tool::Translate { .. }
            | Tool::RotateAxis(_)
            | Tool::Align { .. }
            | Tool::CircularPattern(_)
            | Tool::Onto(_)
    ) {
        return None;
    }
    let mut v = match &ws.tool {
        // Text hides the column (its Cancel / Done sit at the bottom of the view, s11-072/074).
        Tool::Text { .. } => return Some(Vec::new()),
        Tool::Pattern(t) if t.spacing.abs() > 1e-9 => vec![toolbar::TOOL_DONE],
        Tool::Pattern(_) => vec![toolbar::PATTERN_TYPE],
        // Sketch Mirror keeps the sketch column (Cancel sits at the bottom, s11-091).
        Tool::Mirror(_) if ws.session.active_sketch.is_some() => return None,
        Tool::Mirror(t) => vec![on_off(toolbar::KEEP_ORIGINALS, t.keep)],
        // As observed (348): once pulled, Done takes Cancel's place.
        Tool::BodyEdgeOffset(t) if t.distance.abs() > 1e-9 => return Some(vec![toolbar::TOOL_DONE, toolbar::OFFSET_EDGE_TYPE]),
        Tool::BodyEdgeOffset(_) => vec![toolbar::OFFSET_EDGE_TYPE],
        // 26.200 keeps the body column for Union/Subtract/Intersect; Cancel and Done sit at the bottom (s11-112).
        Tool::Boolean(_) => return None,
        #[allow(unreachable_patterns)]
        Tool::Boolean(t) => {
            let keep = if t.op == "subtract" { toolbar::KEEP_ORIGINALS_OF } else { toolbar::KEEP_ORIGINALS };
            let keep = ToolDef { sub: Some(t.keep_label()), ..keep };
            if t.ready() { vec![toolbar::TOOL_DONE, keep] } else { vec![keep] }
        }
        Tool::Translate { copy, to: Some(_), .. } => vec![toolbar::TOOL_DONE, on_off(toolbar::COPY_OPTION, *copy)],
        Tool::Translate { copy, .. } => vec![on_off(toolbar::COPY_OPTION, *copy)],
        Tool::RotateAxis(t) if t.angle.abs() > 1e-9 || ws.is_live() => vec![toolbar::TOOL_DONE],
        Tool::Align { to: Some(_), .. } => vec![toolbar::TOOL_DONE],
        Tool::Align { .. } | Tool::RotateAxis(_) => Vec::new(),
        Tool::CircularPattern(t) if t.about.is_some() => vec![toolbar::TOOL_DONE],
        Tool::CircularPattern(_) => Vec::new(),
        _ => return None,
    };
    v.push(toolbar::TOOL_CANCEL);
    Some(v)
}

pub(super) fn is_active(ws: &Workspace, def: &ToolDef) -> bool {
    use crate::workspace::toolbar::ToolKind as K;
    if let (Action::Tool(k), Tool::Await { kind, .. }) = (def.action, &ws.tool)
        && k == *kind
    {
        return true;
    }
    match (def.action, &ws.tool) {
        (Action::Tool(K::Line), Tool::Line { .. })
        | (Action::Tool(K::Rectangle), Tool::Rectangle { .. } | Tool::Rect3 { .. })
        | (Action::Tool(K::Polygon), Tool::Polygon { .. })
        | (Action::Tool(K::Spline), Tool::Spline { .. })
        | (Action::Tool(K::Arc), Tool::Arc { .. })
        | (Action::Tool(K::Ellipse), Tool::Ellipse { .. })
        | (Action::Tool(K::Circle), Tool::Circle { .. })
        | (Action::Tool(K::Extrude), Tool::Extrude(_))
        | (Action::Tool(K::Shell), Tool::Shell(_))
        | (Action::Tool(K::Fillet), Tool::Fillet(crate::workspace::tools::FilletTool { chamfer: false, .. }))
        | (Action::Tool(K::Chamfer), Tool::Fillet(crate::workspace::tools::FilletTool { chamfer: true, .. }))
        | (Action::Tool(K::Move), Tool::Move(_))
        | (Action::Tool(K::Pattern), Tool::Pattern(_))
        | (Action::Tool(K::OffsetFace), Tool::OffsetFace(_))
        | (Action::Tool(K::Scale), Tool::Scale(_))
        | (Action::Tool(K::ScaleNonUniform), Tool::Scale(_))
        | (Action::Tool(K::Loft), Tool::Loft(_))
        | (Action::Tool(K::Sweep), Tool::Sweep(_))
        | (Action::Tool(K::Trim), Tool::Trim { .. })
        | (Action::Tool(K::OffsetEdge), Tool::OffsetEdge(_))
        | (Action::Tool(K::RotateAxis), Tool::RotateAxis(_))
        | (Action::Tool(K::OffsetPlane), Tool::OffsetPlane(_))
        | (Action::Tool(K::CircularPattern), Tool::CircularPattern(_))
        | (Action::Tool(K::Project), Tool::Project)
        | (Action::Tool(K::Text), Tool::Text { .. })
        | (Action::Tool(K::SketchFillet), Tool::SketchFillet { .. })
        | (Action::Tool(K::Axis2), Tool::Axis2 { .. })
        | (Action::Tool(K::PlanePerpEdge), Tool::PointPlane { perp: true, .. })
        | (Action::Tool(K::PlaneParFace), Tool::PointPlane { perp: false, .. })
        | (Action::Tool(K::Align), Tool::Align { .. })
        | (Action::Tool(K::ReplaceFace), Tool::Onto(crate::workspace::tools::OntoTool { project: false, .. }))
        | (Action::Tool(K::ProjectEdge), Tool::Onto(crate::workspace::tools::OntoTool { project: true, sketch: false, .. }))
        | (Action::Tool(K::Project), Tool::Onto(crate::workspace::tools::OntoTool { sketch: true, .. }))
        | (Action::Tool(K::Translate), Tool::Translate { .. })
        | (Action::Tool(K::Union), Tool::Boolean(crate::workspace::tools::BooleanTool { op: "union", .. }))
        | (Action::Tool(K::Subtract), Tool::Boolean(crate::workspace::tools::BooleanTool { op: "subtract", .. }))
        | (Action::Tool(K::Intersect), Tool::Boolean(crate::workspace::tools::BooleanTool { op: "intersect", .. }))
        | (Action::Tool(K::PlaneEdge), Tool::EdgePlane(crate::workspace::tools::EdgePlaneTool { through: true, .. }))
        | (Action::Tool(K::PlaneCurve), Tool::EdgePlane(crate::workspace::tools::EdgePlaneTool { through: false, .. }))
        | (Action::Tool(K::Plane3), Tool::Plane3 { .. })
        | (Action::Tool(K::Revolve), Tool::Revolve(_)) => true,
        // Waiting for a plane, the tool that will start on it is shown on (Line unless another was picked).
        (Action::Tool(kind), Tool::PickPlane { .. }) => ws.pending_tool == Some(kind),
        (Action::Tool(K::Mirror), Tool::Mirror(t)) => !t.split,
        (Action::Tool(K::SplitBody), Tool::Mirror(t)) => t.split,
        (Action::Flyout(cat), _) => ws.flyout.is_some_and(|(c, _)| c == cat) || ws.flyout_sub.is_some_and(|(c, _)| c == cat),
        (Action::Ui(UiAction::Modeling), _) => ws.drawings.is_none() && !ws.visualizing,
        (Action::Ui(UiAction::Visualization), _) => ws.visualizing,
        (Action::Ui(UiAction::Drawings), _) => ws.drawings.is_some() || ws.switcher,
        (Action::Ui(UiAction::ToggleItems), _) => ws.show_items,
        _ => false,
    }
}

impl Workspace {
    pub(in crate::workspace) fn floating_controls(&mut self, ui: &mut Ui) {
        let vp = self.viewport_rect;
        // 8 px from the Items sidebar (measured, L02).
        let x = vp.left() + 5.0;
        let mut clicked: Option<ToolDef> = None;
        let gap = toolbar::GROUP_GAP;

        // Top: workspaces and the Items toggle.
        // In a drawing, the Drawings chip names the open drawing instead of showing its shortcut, and Items goes away.
        let open_drawing =
            self.drawings.as_ref().and_then(|d| self.session.doc.drawings.iter().find(|s| s.id == d.id)).map(|s| s.name.clone());
        let top: Vec<ToolDef> = toolbar::TOP.to_vec();
        // The sub-label names the workspace: Modeling, Visualization, or the open drawing (s11-110).
        let workspace = open_drawing.clone().unwrap_or_else(|| if self.visualizing { "Visualization".into() } else { "Modeling".into() });
        let subs: Vec<Option<String>> = vec![Some(workspace)];
        let top_active: Vec<bool> = vec![self.switcher];
        if let Some(i) = toolbar::group_with(ui, pos2(x, vp.top() + 9.0), &top, &subs, &top_active, true, "top") {
            clicked = toolbar::TOP.get(i).copied();
        }
        // Measured (L05): groups 9 px under the title bar, 6 px apart; a 2 px line 9 px under Items, Search 9 px
        // under that.
        let items_y = vp.top() + 9.0 + toolbar::group_height(toolbar::TOP.len()) + gap;
        let sep_y = items_y + toolbar::group_height(1) + 10.0;
        if open_drawing.is_none() {
            if toolbar::group(ui, pos2(x, items_y), &[toolbar::ITEMS], &[self.show_items], true, "items").is_some() {
                clicked = Some(toolbar::ITEMS);
            }
            ui.painter().line_segment([pos2(x + 11.0, sep_y), pos2(x + 33.0, sep_y)], Stroke::new(2.0, Tokens::tool_separator()));
        }

        // A tool waiting for picks: its option(s), Done, Cancel, each in its own container, centered.
        let mode = tool_mode(self);
        if let Some(defs) = &mode {
            let labels = self.variant_menu.is_none() && self.settings.menu_labels != "Never";
            let height = defs.len() as f32 * (toolbar::group_height(1) + gap) - gap;
            let mut y = (vp.center().y - height / 2.0).max(sep_y + 10.0);
            for (i, d) in defs.iter().enumerate() {
                if toolbar::group(ui, pos2(x, y), &[*d], &[false], labels, &format!("mode{i}")).is_some() {
                    clicked = Some(*d);
                    // Pattern Type opens the Linear / Circular cascade, as the Pattern button does.
                    if d.label == "Pattern Type" || d.label == "Measure mode" {
                        let at = pos2(x + Tokens::TOOL_BUTTON + 5.0, y);
                        let label = if d.label == "Pattern Type" { "Pattern" } else { "Measure mode" };
                        self.variant_menu = if self.variant_menu.is_some() { None } else { Some((label, at)) };
                        self.popup_opened_at = self.now;
                        clicked = None;
                    }
                }
                y += toolbar::group_height(1) + gap;
            }
        }
        // Bottom group (3D only).
        let mut bottom: Vec<ToolDef> = Vec::new();
        let drawing = self.drawings.is_some();
        if drawing {
            // The Drawings workspace has none.
        } else if self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. }) || self.sketch_column_no_plane() {
            bottom.push(ToolDef {
                icon: Icon::Construction,
                label: "Construction",
                sub: Some(if self.construction_mode { "On" } else { "Off" }),
                keys: "",
                action: Action::Ui(UiAction::ConstructionMode),
                flyout: false,
            });
            // With something picked in the sketch, Isolate joins in (s11-088); not before there's a plane (s24 matrix).
            let onto = matches!(&self.tool, Tool::Onto(t) if t.sketch);
            if (!self.session.selection.is_empty() || self.isolated.is_some()) && (!self.sketch_column_no_plane() || onto) {
                bottom.push(ToolDef {
                    icon: Icon::Isolate,
                    label: "Isolate",
                    sub: Some(if self.isolated.is_some() { "On" } else { "Off" }),
                    keys: "",
                    action: Action::Ui(UiAction::Isolate),
                    flyout: false,
                });
            }
        } else if !self.session.selection.is_empty() || self.isolated.is_some() || matches!(self.tool, Tool::Onto(_) | Tool::Mirror(_)) {
            bottom.push(ToolDef {
                icon: Icon::Isolate,
                label: "Isolate",
                sub: Some(if self.isolated.is_some() { "On" } else { "Off" }),
                keys: "",
                action: Action::Ui(UiAction::Isolate),
                flyout: false,
            });
        }
        // Visualization and a tool waiting for picks keep only Isolate down here; Measure keeps itself (as observed).
        if !drawing && !self.visualizing && self.measuring {
            bottom.push(ToolDef {
                icon: Icon::Measure,
                label: "Measure",
                sub: None,
                keys: "",
                action: Action::Ui(UiAction::Measure),
                flyout: false,
            });
        } else if !drawing
            && !self.visualizing
            && mode.is_none()
            && !matches!(self.tool, Tool::Project)
            && (!(self.picked_modify_tool()
                || self.is_live() && !matches!(self.tool, Tool::Idle)
                || matches!(self.tool, Tool::Await { .. }))
                || self.sketch_column_no_plane() && !matches!(&self.tool, Tool::Onto(t) if t.sketch)
                || self.session.active_sketch.is_some() && matches!(self.tool, Tool::Await { .. }))
        {
            bottom.push(ToolDef {
                icon: Icon::SectionView,
                label: "Section View",
                sub: Some(if self.section_pick.is_some() {
                    "Select plane..."
                } else if self.section.is_some_and(|s| s.editing) {
                    "Edit"
                } else if self.section.is_some() {
                    "On"
                } else {
                    "Off"
                }),
                keys: "",
                action: Action::Ui(UiAction::SectionView),
                flyout: false,
            });
            // Sketching keeps only Construction and Section View down here (26.200).
            if !(self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. }) || self.sketch_column_no_plane()) {
                bottom.push(ToolDef {
                    icon: Icon::Measure,
                    label: "Measure",
                    sub: None,
                    keys: "",
                    action: Action::Ui(UiAction::Measure),
                    flyout: false,
                });
            }
        }
        // Picking Section View's reference, or editing the cut, leaves only Section View down here (s11-117, s16-021).
        let section_edit = self.section.is_some_and(|s| s.editing) && matches!(self.tool, Tool::Idle);
        if self.section_pick.is_some() || section_edit {
            bottom.clear();
            let sub = if section_edit { "Edit" } else { "Select plane..." };
            bottom.push(ToolDef {
                icon: Icon::SectionView,
                label: "Section View",
                sub: Some(sub),
                keys: "",
                action: Action::Ui(UiAction::SectionView),
                flyout: false,
            });
        }
        let by = vp.bottom() - 9.0 - toolbar::group_height(bottom.len());
        // Preferences ▸ Done / Cancel Buttons ▸ Toolbar or Both: the classic ✓ / ✕ cells (each its own container, as
        // 26.142 had them) above the bottom group, standing in for the bottom row's buttons.
        let confirm: Vec<ToolDef> = match self.confirm_seen {
            Some(done) if self.settings.confirm_buttons != "Bottom of View" && !drawing && mode.is_none() => {
                if done {
                    vec![toolbar::TOOL_DONE, toolbar::TOOL_CANCEL]
                } else {
                    vec![toolbar::TOOL_CANCEL]
                }
            }
            _ => Vec::new(),
        };
        let confirm_y = by - 20.0 - confirm.len() as f32 * (toolbar::group_height(1) + gap);
        let by_middle = if confirm.is_empty() { by } else { confirm_y };
        // Middle: Search, then the contextual set, centered vertically in the viewport.
        let (tools, head_sub) = if mode.is_some() { (Vec::new(), None) } else { context_tools(self) };
        // Edges have no Delete in the reference.
        let idle_column = self.idle_column();
        let delete_tail = !idle_column
            && (!matches!(self.session.selection_kind(), SelectionKind::Nothing | SelectionKind::Edges)
                || self.session.active_sketch.is_some());
        let sketching =
            self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. }) || self.sketch_column_no_plane();
        let region_selected = self.session.selection_kind() == SelectionKind::Regions && !self.sketch_column_no_plane();
        // While sketching the tail is always Trim + Delete, so the toolbar doesn't jump when you select.
        let tail: Vec<ToolDef> = if drawing {
            Vec::new()
        } else if sketching && (!region_selected || self.tool.is_sketch_tool()) {
            toolbar::SKETCH_TAIL.to_vec()
        } else if delete_tail {
            vec![toolbar::DELETE]
        } else if let Tool::Await { kind, .. } = self.tool {
            // A tool waiting for its first pick: the idle set, then the tool in its own group, lit (s24 matrix).
            toolbar::def_for(kind).into_iter().collect()
        } else if idle_column && matches!(self.tool, Tool::Extrude(_)) {
            // Extrude picked or pulled: the idle set, then Extrude in its own group, lit (s24-007).
            toolbar::def_for(ToolKind::Extrude).into_iter().collect()
        } else if let Tool::Onto(t) = &self.tool {
            // Applied, nothing is picked: the idle set, then the running tool in its own group (s11-130, s11-135).
            toolbar::FACE.iter().filter(|d| d.label == t.title()).copied().collect()
        } else if matches!(&self.tool, Tool::Mirror(t) if t.split && !t.with.is_empty()) {
            // Split Body once a splitting tool is picked: the same (s12-004).
            toolbar::FACE.iter().filter(|d| d.label == "Split Body").copied().collect()
        } else {
            Vec::new()
        };
        let mut tools: Vec<ToolDef> = tools.into_iter().map(|d| self.with_variant(d)).collect();
        let head = tools.first().copied();
        let has_head = head.is_some_and(|h| matches!(h.action, Action::Ui(UiAction::DeselectAll | UiAction::ExitSketching)));
        if has_head {
            tools.remove(0);
        }
        let tail = if mode.is_some() { Vec::new() } else { tail };
        // While sketching the head group also shows the open sketch's name under Exit Sketching (26.200).
        let head_rows = if head.is_some_and(|h| h.action == Action::Ui(UiAction::ExitSketching)) { 2 } else { 1 };
        // The sketch tail is Trim and Delete as two groups; any other tail is one group.
        let mut tail_groups: Vec<Vec<ToolDef>> = if tail.len() == 2 && tail[0].label == "Trim" {
            tail.iter().map(|d| vec![*d]).collect()
        } else if tail.is_empty() {
            Vec::new()
        } else {
            vec![tail.clone()]
        };
        // A running tool the set doesn't list joins in its own group, lit (Shell on a curved face: s24 matrix).
        let running = match &self.tool {
            Tool::Await { kind, .. } => Some(*kind),
            Tool::Shell(_) => Some(ToolKind::Shell),
            Tool::Sweep(_) => Some(ToolKind::Sweep),
            Tool::Revolve(_) => Some(ToolKind::Revolve),
            Tool::Loft(_) => Some(ToolKind::Loft),
            Tool::Extrude(_) if self.options_menu.is_some() => Some(ToolKind::Extrude),
            Tool::Fillet(t) if self.options_menu.is_some() && !t.chamfer => Some(ToolKind::Fillet),
            _ => None,
        };
        if let Some(d) = running.and_then(toolbar::def_for)
            && !tools.iter().chain(&tail).any(|t| t.action == d.action)
            && !matches!(self.tool, Tool::Await { .. } if tail.iter().any(|t| t.action == d.action))
            && !tail_groups.iter().flatten().any(|t| t.action == d.action)
        {
            tail_groups.push(vec![d]);
        }
        let fixed = toolbar::group_height(1)
            + gap
            + if has_head { toolbar::group_height(head_rows) + gap } else { 0.0 }
            + tail_groups.iter().map(|g| toolbar::group_height(g.len()) + gap).sum::<f32>();
        let height = fixed + toolbar::group_height(tools.len());
        let top_min = sep_y + 10.0;
        // Measured (L05 at 680 and 900 px): the middle stops 10–27 px above the bottom group.
        let limit = if drawing { vp.bottom() - 9.0 } else { by_middle - 16.0 };
        let mut y = (vp.center().y - height / 2.0).max(top_min);
        // Too tall for the window: packed under the top group, cut to what fits, the rest behind More (L05).
        let mut split = None;
        self.overflow.clear();
        if y + height > limit && tools.len() > 1 {
            y = top_min;
            let room = limit - y - fixed - 1.0 - toolbar::SPLIT_GAP;
            let k = ((room / Tokens::TOOL_PITCH).floor() as usize).saturating_sub(1).min(tools.len() - 1);
            self.overflow = tools.split_off(k);
            // A running tool cut off takes the last place shown; what it displaces leads the rest (s11-123).
            if k > 0
                && let Some(i) = self.overflow.iter().position(|d| is_active(self, d))
            {
                let running = self.overflow.remove(i);
                if let Some(last) = tools.pop() {
                    self.overflow.insert(0, last);
                }
                tools.push(running);
            }
            tools.push(toolbar::MORE_OVERFLOW);
            split = Some(k);
        }
        let overflow_new = self.overflow.iter().any(toolbar::is_new);
        toolbar::OVERFLOW_NEW.with(|c| c.set(overflow_new));
        if self.overflow.is_empty() && self.flyout.is_some_and(|(c, _)| c == toolbar::Category::Overflow) {
            self.flyout = None;
        }
        // What the column shows, top to bottom, for ui.state (bottom group and ✓ / ✕ cells added below).
        let def_label = |d: &ToolDef, sub: Option<&str>| match sub.or(d.sub) {
            Some(s) if !s.is_empty() => format!("{} ({s})", d.label),
            _ => d.label.to_string(),
        };
        let mut seen: Vec<String> = mode.iter().flatten().map(|d| def_label(d, None)).collect();
        if mode.is_none() {
            seen.push("Search".into());
        }
        if let (true, Some(h)) = (has_head, head) {
            seen.push(def_label(&h, head_sub.as_deref()));
        }
        seen.extend(tools.iter().map(|d| def_label(d, None)));
        seen.extend(tail_groups.iter().flatten().map(|d| def_label(d, None)));
        seen.extend(confirm.iter().chain(&bottom).map(|d| def_label(d, None)));
        self.seen_column = seen;
        // With a flyout open the reference drops the labels of the main column.
        let labels = self.flyout.is_none() && self.variant_menu.is_none() && self.settings.menu_labels != "Never";
        if mode.is_none() && toolbar::group(ui, pos2(x, y), &[toolbar::SEARCH], &[self.search.is_some()], labels, "search").is_some() {
            clicked = Some(toolbar::SEARCH);
        }
        y += toolbar::group_height(1) + gap;
        if let (true, Some(h)) = (has_head, head) {
            if head_rows == 2 {
                let no_plane = self.session.active_sketch.is_none();
                let defs = [h, toolbar::SKETCH_ROW];
                if let Some(i) =
                    toolbar::group_full(ui, pos2(x, y), &defs, &[None, head_sub], &[false, false], &[false, no_plane], labels, "head", None)
                {
                    clicked = defs.get(i).copied();
                }
            } else if toolbar::group_with(ui, pos2(x, y), &[h], &[head_sub], &[false], labels, "head").is_some() {
                clicked = Some(h);
            }
            y += toolbar::group_height(head_rows) + gap;
        }
        let active: Vec<bool> = tools.iter().map(|d| is_active(self, d)).collect();
        if let Some(i) = toolbar::group_full(ui, pos2(x, y), &tools, &[], &active, &[], labels, "context", split) {
            clicked = tools.get(i).copied();
            // Clicking a running tool that has variants opens its cascade, as observed.
            if let Some(d) = clicked
                && !toolbar::variants(d.label).0.is_empty()
                && d.sub != Some("Chain")
                && active.get(i).copied().unwrap_or(false)
            {
                let at = pos2(x + Tokens::TOOL_BUTTON + 5.0, y + i as f32 * Tokens::TOOL_PITCH);
                self.variant_menu = if self.variant_menu.is_some_and(|(l, _)| l == d.label) { None } else { Some((d.label, at)) };
                self.popup_opened_at = self.now;
                clicked = None;
            }
            // The running tool clicked again: its options (26.200's ≡).
            if let Some(d) = clicked
                && active.get(i).copied().unwrap_or(false)
                && toolbar::variants(d.label).0.is_empty()
                && (!tool_options(self).is_empty() || self.card_tool())
            {
                let at = pos2(x + Tokens::TOOL_BUTTON + 5.0, y + i as f32 * Tokens::TOOL_PITCH);
                self.options_menu = if self.options_menu.is_some() { None } else { Some(at) };
                self.popup_opened_at = self.now;
                clicked = None;
            }
            if let Some(d) = clicked
                && let Action::Flyout(cat) = d.action
            {
                // Docked right next to the toolbar column, level with the button.
                let below = if split.is_some_and(|k| i >= k) { toolbar::SPLIT_GAP } else { 0.0 };
                let at = pos2(x + Tokens::TOOL_BUTTON + 5.0, y + i as f32 * Tokens::TOOL_PITCH + below);
                self.flyout = if self.flyout.is_some_and(|(c, _)| c == cat) { None } else { Some((cat, at)) };
                self.flyout_sub = None;
                self.popup_opened_at = self.now;
                clicked = None;
            }
        }
        y += toolbar::group_height(tools.len()) + if split.is_some() { toolbar::SPLIT_GAP } else { 0.0 } + gap;
        let nothing_selected = self.session.selection.is_empty();
        for (k, group) in tail_groups.iter().enumerate() {
            let tail_disabled: Vec<bool> = group.iter().map(|d| d.label == "Delete" && nothing_selected).collect();
            let trim_on: Vec<bool> = group
                .iter()
                .map(|d| {
                    d.label == "Trim" && matches!(self.tool, Tool::Trim { .. })
                        || matches!(self.tool, Tool::Await { kind, .. } if d.action == Action::Tool(kind))
                        || is_active(self, d)
                        || matches!(&self.tool, Tool::Onto(t) if t.title() == d.label)
                        || d.label == "Split Body" && matches!(&self.tool, Tool::Mirror(t) if t.split)
                })
                .collect();
            if let Some(i) = toolbar::group_full(ui, pos2(x, y), group, &[], &trim_on, &tail_disabled, labels, &format!("tail{k}"), None) {
                clicked = group.get(i).copied();
                // The running Replace Face clicked again: its card.
                if trim_on.get(i).copied().unwrap_or(false) && matches!(self.tool, Tool::Onto(_) | Tool::Mirror(_)) {
                    self.options_menu = if self.options_menu.is_some() { None } else { Some(pos2(x, y)) };
                    self.popup_opened_at = self.now;
                    clicked = None;
                }
            }
            y += toolbar::group_height(group.len()) + gap;
        }

        if drawing {
            self.right_column(ui);
            if let Some(def) = clicked {
                self.act(def.action, def.label);
            }
            return;
        }
        ui.painter().line_segment([pos2(x + 11.0, by - 10.0), pos2(x + 33.0, by - 10.0)], Stroke::new(2.0, Tokens::tool_separator()));
        let bottom_active: Vec<bool> = bottom
            .iter()
            .map(|d| match d.label {
                "Measure" => self.measuring,
                "Section View" => self.section.is_some() || self.section_pick.is_some(),
                "Isolate" => self.isolated.is_some(),
                "Construction" => self.construction_mode,
                _ => false,
            })
            .collect();
        if let Some(i) = toolbar::group(ui, pos2(x, by), &bottom, &bottom_active, labels, "bottom") {
            clicked = bottom.get(i).copied();
        }
        for (i, d) in confirm.iter().enumerate() {
            let at = pos2(x, confirm_y + i as f32 * (toolbar::group_height(1) + gap));
            if toolbar::group(ui, at, &[*d], &[false], labels, &format!("confirm{i}")).is_some() {
                self.confirm_click = Some(d.label);
            }
        }
        // Section View picking its reference or editing: white with a dark icon too (s11-117, s16-021).
        if (self.section_pick.is_some() || section_edit)
            && let Some(k) = bottom.iter().position(|d| d.label == "Section View")
        {
            let cell =
                Rect::from_min_size(pos2(x, by + 1.0 + k as f32 * Tokens::TOOL_PITCH), vec2(Tokens::TOOL_BUTTON, Tokens::TOOL_PITCH))
                    .shrink(3.0);
            ui.painter().rect_filled(cell, radius(7), Color32::WHITE);
            icons::paint(
                ui.painter(),
                Rect::from_center_size(cell.center(), vec2(28.0, 28.0)),
                Icon::SectionView,
                crate::workspace::viewport::PILL_INK,
            );
        }
        // Measure on: its button turns white with a dark icon (as observed, L24).
        if self.measuring
            && let Some(k) = bottom.iter().position(|d| d.label == "Measure")
        {
            let cell =
                Rect::from_min_size(pos2(x, by + 1.0 + k as f32 * Tokens::TOOL_PITCH), vec2(Tokens::TOOL_BUTTON, Tokens::TOOL_PITCH))
                    .shrink(3.0);
            ui.painter().rect_filled(cell, radius(7), Color32::WHITE);
            icons::paint(
                ui.painter(),
                Rect::from_center_size(cell.center(), vec2(28.0, 28.0)),
                Icon::Measure,
                crate::workspace::viewport::PILL_INK,
            );
        }

        self.right_column(ui);

        if let Some(def) = clicked {
            self.act(def.action, def.label);
        }
    }
}
