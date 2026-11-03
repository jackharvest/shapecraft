//! The floating vertical toolbar with label chips, and the contextual tool sets.
//!
//! Sets and order are as observed in the reference app (01-observed-ui.md, "Middle group is contextual").

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Start an interactive tool.
    Tool(ToolKind),
    /// Run an engine command directly.
    Command(&'static str),
    Ui(UiAction),
    /// Opens a flyout listing more tools.
    Flyout(Category),
    /// Shown for parity, not built yet.
    Soon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Sketch,
    Line,
    Rectangle,
    Circle,
    Extrude,
    Fillet,
    Shell,
    Revolve,
    Polygon,
    Arc,
    Move,
    Mirror,
    Pattern,
    Ellipse,
    Spline,
    OffsetFace,
    Scale,
    SplitBody,
    Loft,
    Sweep,
    Trim,
    OffsetEdge,
    RotateAxis,
    OffsetPlane,
    Plane3,
    CircularPattern,
    Project,
    Text,
    Axis2,
    /// Plane Perpendicular to Edge at Point / Plane Parallel to Face at Point: pick, then click a point.
    PlanePerpEdge,
    PlaneParFace,
    Align,
    ScaleNonUniform,
    /// Two-point move: click where from, then where to.
    Translate,
    PlaneEdge,
    PlaneCurve,
    Union,
    Subtract,
    Intersect,
    /// Rounds corners where sketch lines meet (26.200).
    SketchFillet,
    /// Chamfer/Fillet started as a bevel (26.200 has Fillet and Chamfer as separate buttons, s11-103).
    Chamfer,
    ReplaceFace,
    ProjectEdge,
    WrapEmboss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    /// View menu entries (the reference's View menu, 04-menu-tree-workspace).
    NearestOrtho,
    /// Rotate View: 0–3 left/right/up/down by 15°, 4–7 the same by 45°.
    RotateView(u8),
    /// Grid Position: 0 XY, 1 YZ, 2 ZX.
    GridPosition(u8),
    ToggleGrid,
    ToggleAxes,
    ToggleEdges,
    ToggleHiddenEdges,
    ToggleDecals,
    /// Units: Millimeter, Centimeter, Meter, Inch, Foot.
    Units(u8),
    /// Foot & Inch Format ▸ Architectural (true) / Decimal.
    FeetArchitectural(bool),
    /// Degree Format ▸ Fractional (true) / Decimal.
    DegreesFractional(bool),
    LockGrid,
    CollapseAll,
    /// File ▸ Import Material… / Import Image as Decal…: a picture into the Visualization library.
    ImportMaterial,
    ImportDecal,
    /// Section View's edit column (s16-021).
    DeleteSection,
    Section2D,
    SectionOnly,
    /// View ▸ Show Pinned Measurements.
    TogglePinned,
    ToggleItems,
    ToggleHistory,
    Search,
    ExitSketching,
    DeselectAll,
    /// Edit ▸ Copy / Paste (⌘C / ⌘V) of bodies and construction planes.
    Copy,
    Paste,
    Modeling,
    DisplayModes,
    Measure,
    SectionView,
    Isolate,
    Shortcuts,
    Variables,
    InsertImage,
    /// An annotation tool on a drawing sheet (see `annotate::DrawTool::from_index`).
    DrawTool(u8),
    /// A Views tool on a drawing sheet (see `sheet_views::ViewTool::from_index`), and its Done / Cancel.
    ViewTool(u8),
    ViewDone,
    ViewCancel,
    Screenshot,
    /// Shows something without doing anything when clicked (the sketch name row).
    Noop,
    ConstructionMode,
    Help,
    Preferences,
    NavPresets,
    Rename,
    Drawings,
    ExportDrawing,
    CreateDrawing,
    DrawingNext,
    DrawingCancel,
    DrawingProps,
    Visualization,
    /// The tool-mode buttons: confirm, abandon, and the tool's on/off option (Keep Originals, Copy).
    ToolDone,
    ToolCancel,
    ToolOption,
    // Menu-only commands (26.200 menu bar).
    SelectThrough,
    ShowHiddenBodies,
    InvertVisibility,
    ZoomToSelection,
    RevealInItems,
    RenameProject,
    Visualized,
    MeasureMode(u8),
    EditSection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Insert,
    Construct,
    /// Construct's cascades.
    Plane,
    Axis,
    /// The Drawings workspace's flyouts.
    Views,
    Dimensions,
    Geometries,
    Transform,
    Tools,
    /// The tools a short window cut from the contextual set (`Workspace::overflow`).
    Overflow,
}

#[derive(Clone, Copy, Debug)]
pub struct ToolDef {
    pub icon: Icon,
    pub label: &'static str,
    pub sub: Option<&'static str>,
    pub keys: &'static str,
    pub action: Action,
    pub flyout: bool,
}

/// The variants of a tool, picked from a cascade by clicking the tool again while it runs (as observed), and the
/// one used until another is picked.
pub fn variant_icon(label: &str, i: usize) -> Option<Icon> {
    let icons: &[Icon] = match label {
        "Rectangle" => &[Icon::RectCenter, Icon::Rectangle, Icon::Rect3],
        "Polygon" => &[Icon::Triangle, Icon::Polygon, Icon::Hexagon, Icon::Octagon],
        "Spline" => &[Icon::SplineControl, Icon::Spline],
        "Pattern" => &[Icon::Pattern, Icon::PatternCircular],
        "Offset Edge" => &[Icon::OffsetEdge, Icon::OffsetChain],
        "Scale" => &[Icon::Scale, Icon::Scale],
        "Measure mode" => &[Icon::Measure, Icon::MDistance, Icon::MAngle],
        _ => &[],
    };
    icons.get(i).copied()
}

pub fn variants(label: &str) -> (&'static [&'static str], usize) {
    match label {
        "Rectangle" => (&["Center", "Diagonal", "3 Points"], 1),
        "Polygon" => (&["Triangle", "Pentagon", "Hexagon", "Octagon"], 1),
        "Spline" => (&["Control", "Fit"], 1),
        "Pattern" => (&["Linear", "Circular"], 0),
        "Offset Edge" => (&["Single", "Chain"], 0),
        "Scale" => (&["Uniform", "Non-uniform"], 1),
        "Measure mode" => (&["Object Measurements", "Point-to-Point Distance", "3-Point Angle"], 0),
        _ => (&[], 0),
    }
}

const fn t(icon: Icon, label: &'static str, keys: &'static str, action: Action) -> ToolDef {
    ToolDef { icon, label, sub: None, keys, action, flyout: false }
}

const fn tv(icon: Icon, label: &'static str, sub: &'static str, keys: &'static str, action: Action) -> ToolDef {
    ToolDef { icon, label, sub: Some(sub), keys, action, flyout: true }
}

use Action::{Command, Flyout, Soon, Tool, Ui as U};

/// 26.200: one "Project ⇧⌘\\" button (its sub-label the workspace you're in) opening the workspace drawer (s11-110).
/// A tool's button as the toolsets have it (the first set listing it).
pub fn def_for(kind: ToolKind) -> Option<ToolDef> {
    [FACE, BODY, EDGE, REGION, SKETCHING, FACES, CURVED_FACE, EDGES, MIXED, PLANE, SKETCH_TAIL]
        .iter()
        .flat_map(|set| set.iter())
        .find(|d| d.action == Action::Tool(kind))
        .copied()
}

pub const TOP: &[ToolDef] = &[t(Icon::Modeling, "Project", "⇧⌘\\", U(UiAction::Drawings))];

pub const ITEMS: ToolDef = t(Icon::Items, "Items", "", U(UiAction::ToggleItems));
pub const SEARCH: ToolDef = t(Icon::Search, "Search", "⌘F", U(UiAction::Search));

pub const IDLE: &[ToolDef] = &[
    t(Icon::Sketch, "Sketch", "", Tool(ToolKind::Sketch)),
    ToolDef { flyout: true, ..t(Icon::Insert, "Insert", "", Flyout(Category::Insert)) },
    ToolDef { flyout: true, ..t(Icon::Construct, "Construct", "", Flyout(Category::Construct)) },
    ToolDef { flyout: true, ..t(Icon::Transform, "Transform", "", Flyout(Category::Transform)) },
    ToolDef { flyout: true, ..t(Icon::Tools, "Tools", "", Flyout(Category::Tools)) },
    // 26.200 ends the idle set with Visualize (the workspaces moved into the Project drawer, s11-002).
    t(Icon::Visualize, "Visualize", "", U(UiAction::Visualization)),
];

pub const SKETCHING: &[ToolDef] = &[
    t(Icon::Line, "Line", "L", Tool(ToolKind::Line)),
    t(Icon::Arc, "Arc", "A", Tool(ToolKind::Arc)),
    tv(Icon::Spline, "Spline", "Fit", "I", Tool(ToolKind::Spline)),
    tv(Icon::Rectangle, "Rectangle", "Diagonal", "R", Tool(ToolKind::Rectangle)),
    t(Icon::Circle, "Circle", "C", Tool(ToolKind::Circle)),
    t(Icon::Ellipse, "Ellipse", "", Tool(ToolKind::Ellipse)),
    tv(Icon::Polygon, "Polygon", "Pentagon", "G", Tool(ToolKind::Polygon)),
    t(Icon::Project, "Project", "", Tool(ToolKind::Project)),
    t(Icon::OffsetEdge, "Offset Edge", "O", Tool(ToolKind::OffsetEdge)),
    t(Icon::Fillet, "Fillet", "", Tool(ToolKind::SketchFillet)),
    t(Icon::Text, "Text", "", Tool(ToolKind::Text)),
    t(Icon::SketchMove, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale", "", Tool(ToolKind::Scale)),
    t(Icon::SketchPattern, "Pattern - Linear", "", Tool(ToolKind::Pattern)),
    t(Icon::PatternCircular, "Pattern - Circular", "", Tool(ToolKind::CircularPattern)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::SketchMirror, "Mirror", "", Tool(ToolKind::Mirror)),
];

/// Trim and Delete close the sketch column, each in a group of its own (as observed, 26.200).
pub const SKETCH_TAIL: &[ToolDef] = &[t(Icon::Trim, "Trim", "T", Tool(ToolKind::Trim)), t(Icon::Delete, "Delete", "", Command("delete"))];

/// The second row of the head group while sketching: the active sketch's name, or "No active plane" (ghosted)
/// while waiting for one. It only shows which sketch is open. The empty label takes the run-time sub-label.
pub const SKETCH_ROW: ToolDef =
    ToolDef { icon: Icon::Sketch, label: "", sub: None, keys: "", action: Action::Ui(UiAction::Noop), flyout: false };

/// A sketch region picked (26.200, s11-078).
pub const REGION: &[ToolDef] = &[
    t(Icon::Sketch, "Sketch", "", Tool(ToolKind::Sketch)),
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale Sketch", "", Tool(ToolKind::Scale)),
    t(Icon::SketchPattern, "Pattern - Linear", "", Tool(ToolKind::Pattern)),
    t(Icon::PatternCircular, "Pattern - Circular", "", Tool(ToolKind::CircularPattern)),
    t(Icon::Extrude, "Extrude", "E", Tool(ToolKind::Extrude)),
    t(Icon::Axis, "Axis - Perp. to Face", "", Command("axis_perp_face")),
    t(Icon::Plane, "Plane - Offset", "", Tool(ToolKind::OffsetPlane)),
    t(Icon::Text, "Text", "", Tool(ToolKind::Text)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Mirror, "Mirror", "", Tool(ToolKind::Mirror)),
    t(Icon::Loft, "Loft", "", Tool(ToolKind::Loft)),
    t(Icon::SplitBody, "Split Body", "", Soon),
    t(Icon::Revolve, "Revolve", "V", Tool(ToolKind::Revolve)),
    t(Icon::Sweep, "Sweep", "W", Tool(ToolKind::Sweep)),
    t(Icon::Visualize, "Fill (Beta)", "", Command("fill")),
    t(Icon::OffsetEdge, "Offset Edge", "O", Tool(ToolKind::OffsetEdge)),
    t(Icon::ProjectSketch, "Project as Sketch", "", Tool(ToolKind::Project)),
    t(Icon::ProjectEdge, "Project as Edge", "", Tool(ToolKind::ProjectEdge)),
    t(Icon::WrapEmboss, "Wrap & Emboss", "", Tool(ToolKind::WrapEmboss)),
];

// Face, edge and body sets as observed (screens 60, 61, 64).
/// A flat face picked (26.200, s11-103).
pub const FACE: &[ToolDef] = &[
    t(Icon::Sketch, "Sketch", "", Tool(ToolKind::Sketch)),
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale", "", Tool(ToolKind::Scale)),
    t(Icon::Align, "Align", "", Tool(ToolKind::Align)),
    t(Icon::OffsetFace, "Offset Face", "", Tool(ToolKind::OffsetFace)),
    t(Icon::Fillet, "Fillet", "F", Tool(ToolKind::Fillet)),
    t(Icon::Fillet, "Chamfer", "", Tool(ToolKind::Chamfer)),
    t(Icon::Extrude, "Extrude", "E", Tool(ToolKind::Extrude)),
    t(Icon::Shell, "Shell", "H", Tool(ToolKind::Shell)),
    t(Icon::Axis, "Axis - Perp. to Face", "", Command("axis_perp_face")),
    t(Icon::Plane, "Plane - Offset", "", Tool(ToolKind::OffsetPlane)),
    t(Icon::Visualize, "Visualize", "", U(UiAction::Visualization)),
    t(Icon::Pattern, "Pattern - Linear", "", Tool(ToolKind::Pattern)),
    t(Icon::PatternCircular, "Pattern - Circular", "", Tool(ToolKind::CircularPattern)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Mirror, "Mirror", "", Tool(ToolKind::Mirror)),
    t(Icon::Loft, "Loft", "", Tool(ToolKind::Loft)),
    t(Icon::SplitBody, "Split Body", "", Tool(ToolKind::SplitBody)),
    t(Icon::Revolve, "Revolve", "V", Tool(ToolKind::Revolve)),
    // The rest goes behind More when the window is too short, which it usually is (s11-122); with only Isolate
    // below (a tool running) Sweep and Replace Face fit (s11-123).
    t(Icon::Sweep, "Sweep", "W", Tool(ToolKind::Sweep)),
    t(Icon::Visualize, "Fill (Beta)", "", Command("fill")),
    t(Icon::ReplaceFace, "Replace Face", "", Tool(ToolKind::ReplaceFace)),
    t(Icon::OffsetEdge, "Offset Edge", "O", Tool(ToolKind::OffsetEdge)),
    t(Icon::ProjectSketch, "Project as Sketch", "", Tool(ToolKind::Project)),
    t(Icon::ProjectEdge, "Project as Edge", "", Tool(ToolKind::ProjectEdge)),
    t(Icon::Text, "Text", "", Tool(ToolKind::Text)),
];

/// Ends a contextual set that doesn't fit the window's height and opens the rest (as observed, L05).
pub const MORE_OVERFLOW: ToolDef = ToolDef { flyout: true, ..t(Icon::More, "More", "", Flyout(Category::Overflow)) };

/// One edge picked (26.200, s11-105).
pub const EDGE: &[ToolDef] = &[
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale", "", Tool(ToolKind::Scale)),
    t(Icon::Align, "Align", "", Tool(ToolKind::Align)),
    t(Icon::Fillet, "Fillet", "F", Tool(ToolKind::Fillet)),
    t(Icon::Fillet, "Chamfer", "", Tool(ToolKind::Chamfer)),
    t(Icon::OffsetEdge, "Offset Edge", "O", Tool(ToolKind::OffsetEdge)),
    t(Icon::Axis, "Axis - Along Edge", "", Command("axis_along_edge")),
    t(Icon::Plane, "Plane - On Curve at Point", "", Tool(ToolKind::PlaneCurve)),
    t(Icon::Plane, "Plane - Through Edge", "", Tool(ToolKind::PlaneEdge)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::SplitBody, "Split Body", "", Tool(ToolKind::SplitBody)),
    t(Icon::ProjectSketch, "Project as Sketch", "", Tool(ToolKind::Project)),
    t(Icon::ProjectEdge, "Project as Edge", "", Tool(ToolKind::ProjectEdge)),
];

/// A whole body picked (26.200, s11-108).
pub const BODY: &[ToolDef] = &[
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale", "", Tool(ToolKind::Scale)),
    t(Icon::Pattern, "Pattern - Linear", "", Tool(ToolKind::Pattern)),
    t(Icon::PatternCircular, "Pattern - Circular", "", Tool(ToolKind::CircularPattern)),
    t(Icon::Align, "Align", "", Tool(ToolKind::Align)),
    t(Icon::Fillet, "Fillet", "F", Tool(ToolKind::Fillet)),
    t(Icon::Fillet, "Chamfer", "", Tool(ToolKind::Chamfer)),
    t(Icon::Shell, "Shell", "H", Tool(ToolKind::Shell)),
    t(Icon::Visualize, "Visualize", "", U(UiAction::Visualization)),
    t(Icon::CreateDrawing, "Create Drawing", "", U(UiAction::CreateDrawing)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Mirror, "Mirror", "", Tool(ToolKind::Mirror)),
    t(Icon::Union, "Union", "⌘U", Tool(ToolKind::Union)),
    t(Icon::Subtract, "Subtract", "⌘B", Tool(ToolKind::Subtract)),
    t(Icon::Intersect, "Intersect", "⌘I", Tool(ToolKind::Intersect)),
    t(Icon::SplitBody, "Split Body", "", Tool(ToolKind::SplitBody)),
    t(Icon::ProjectSketch, "Project as Sketch", "", Tool(ToolKind::Project)),
    t(Icon::ProjectEdge, "Project as Edge", "", Tool(ToolKind::ProjectEdge)),
];

// A curved face, e.g. a fillet's (362).
pub const CURVED_FACE: &[ToolDef] = &[
    // As 26.200 lists it (s24 matrix).
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale", "", Tool(ToolKind::Scale)),
    t(Icon::Align, "Align", "", Tool(ToolKind::Align)),
    t(Icon::OffsetFace, "Offset Face", "", Tool(ToolKind::OffsetFace)),
    t(Icon::Fillet, "Fillet", "F", Tool(ToolKind::Fillet)),
    t(Icon::Fillet, "Chamfer", "", Tool(ToolKind::Chamfer)),
    t(Icon::Axis, "Axis - Cylinder/Cone", "", Command("axis_cylinder")),
    t(Icon::Visualize, "Visualize", "", U(UiAction::Visualization)),
    t(Icon::Pattern, "Pattern - Linear", "", Tool(ToolKind::Pattern)),
    t(Icon::PatternCircular, "Pattern - Circular", "", Tool(ToolKind::CircularPattern)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Mirror, "Mirror", "", Tool(ToolKind::Mirror)),
    t(Icon::SplitBody, "Split Body", "", Tool(ToolKind::SplitBody)),
    t(Icon::Visualize, "Fill (Beta)", "", Command("fill")),
    t(Icon::ReplaceFace, "Replace Face", "", Tool(ToolKind::ReplaceFace)),
    t(Icon::ProjectSketch, "Project as Sketch", "", Tool(ToolKind::Project)),
    t(Icon::ProjectEdge, "Project as Edge", "", Tool(ToolKind::ProjectEdge)),
    t(Icon::WrapEmboss, "Wrap & Emboss", "", Tool(ToolKind::WrapEmboss)),
];

// More than one edge (311): only what works on several edges at once, no Delete.
pub const EDGES: &[ToolDef] = &[
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    tv(Icon::Scale, "Scale", "Non-uniform", "S", Tool(ToolKind::ScaleNonUniform)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Fillet, "Fillet", "F", Tool(ToolKind::Fillet)),
    t(Icon::Fillet, "Chamfer", "", Tool(ToolKind::Chamfer)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::ProjectSketch, "Project as Sketch", "", Tool(ToolKind::Project)),
    t(Icon::ProjectEdge, "Project as Edge", "", Tool(ToolKind::ProjectEdge)),
];

// More than one face (329).
/// Several faces picked (26.200, s11-111).
pub const FACES: &[ToolDef] = &[
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale", "", Tool(ToolKind::Scale)),
    t(Icon::OffsetFace, "Offset Face", "", Tool(ToolKind::OffsetFace)),
    t(Icon::Fillet, "Fillet", "F", Tool(ToolKind::Fillet)),
    t(Icon::Fillet, "Chamfer", "", Tool(ToolKind::Chamfer)),
    t(Icon::Extrude, "Extrude", "E", Tool(ToolKind::Extrude)),
    t(Icon::Shell, "Shell", "H", Tool(ToolKind::Shell)),
    t(Icon::Loft, "Loft", "", Tool(ToolKind::Loft)),
    t(Icon::Visualize, "Fill (Beta)", "", Command("fill")),
    t(Icon::ReplaceFace, "Replace Face", "", Tool(ToolKind::ReplaceFace)),
    t(Icon::Visualize, "Visualize", "", U(UiAction::Visualization)),
    t(Icon::Pattern, "Pattern - Linear", "", Tool(ToolKind::Pattern)),
    t(Icon::PatternCircular, "Pattern - Circular", "", Tool(ToolKind::CircularPattern)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Mirror, "Mirror", "", Tool(ToolKind::Mirror)),
    t(Icon::SplitBody, "Split Body", "", Tool(ToolKind::SplitBody)),
    t(Icon::Revolve, "Revolve", "V", Tool(ToolKind::Revolve)),
    t(Icon::Sweep, "Sweep", "W", Tool(ToolKind::Sweep)),
    t(Icon::ProjectSketch, "Project as Sketch", "", Tool(ToolKind::Project)),
    t(Icon::ProjectEdge, "Project as Edge", "", Tool(ToolKind::ProjectEdge)),
];

// Bodies, faces and edges together (310).
pub const MIXED: &[ToolDef] = &[
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    tv(Icon::Scale, "Scale", "Non-uniform", "S", Tool(ToolKind::ScaleNonUniform)),
    tv(Icon::Pattern, "Pattern", "Linear", "", Tool(ToolKind::Pattern)),
    t(Icon::Align, "Align", "", Tool(ToolKind::Align)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Mirror, "Mirror", "", Tool(ToolKind::Mirror)),
    t(Icon::Project, "Project", "P", Tool(ToolKind::Project)),
    t(Icon::CreateDrawing, "Create Drawing", "", U(UiAction::CreateDrawing)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
];

// While a tool waits for more picks the middle of the toolbar is just its option(s) and Cancel, then Done once
// there is something to apply (324–334).
pub const TOOL_DONE: ToolDef = t(Icon::Check, "Done", "", U(UiAction::ToolDone));
pub const TOOL_CANCEL: ToolDef = t(Icon::Close, "Cancel", "", U(UiAction::ToolCancel));
pub const KEEP_ORIGINALS: ToolDef = t(Icon::Gear, "Keep Originals", "", U(UiAction::ToolOption));
pub const KEEP_ORIGINALS_OF: ToolDef = t(Icon::Gear, "Keep Originals of", "", U(UiAction::ToolOption));
/// Stands for Project as Edge's card (it has no switches; the card shows its picks).
pub const ITEMS_TO_PROJECT: ToolDef = t(Icon::Gear, "Items to Project", "", U(UiAction::ToolOption));
pub const FLIP_ALIGNMENT: ToolDef = t(Icon::Gear, "Flip Alignment", "", U(UiAction::ToolOption));
pub const COPY_OPTION: ToolDef = t(Icon::Gear, "Copy", "", U(UiAction::ToolOption));
/// Move/Rotate's Auto-orientation: the gizmo turns to the face its center is dragged onto.
pub const AUTO_ORIENT_OPTION: ToolDef = t(Icon::Gear, "Auto-orientation", "", U(UiAction::ToolOption));
pub const UNIFORM_OPTION: ToolDef = t(Icon::Gear, "Uniform Orientation", "", U(UiAction::ToolOption));
pub const RULED_OPTION: ToolDef = t(Icon::Gear, "Ruled", "", U(UiAction::ToolOption));
pub const OFFSET_EDGE_TYPE: ToolDef = tv(Icon::OffsetEdge, "Offset Edge Type", "Single", "", Soon);
pub const PATTERN_TYPE: ToolDef = tv(Icon::Pattern, "Pattern Type", "Linear", "", Tool(ToolKind::Pattern));
/// Measure mode's column, as observed (L24): Exit Measure, and the mode with its cascade.
pub const EXIT_MEASURE: ToolDef = t(Icon::Close, "Exit Measure", "", U(UiAction::Measure));
pub const MEASURE_MODE: ToolDef = ToolDef { flyout: true, ..tv(Icon::Measure, "Measure mode", "Object Measurements", "", Soon) };

// A construction plane, as observed (291).
pub const PLANE: &[ToolDef] = &[
    t(Icon::Sketch, "Sketch", "", Tool(ToolKind::Sketch)),
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    tv(Icon::Scale, "Scale", "Non-uniform", "S", Soon),
    t(Icon::Project, "Project", "P", Soon),
    t(Icon::Axis, "Axis - Perp. to Face", "", Soon),
    t(Icon::Plane, "Plane - Offset", "", Tool(ToolKind::OffsetPlane)),
    t(Icon::Text, "Text", "", Soon),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Soon),
    t(Icon::Mirror, "Mirror", "", Soon),
    t(Icon::SplitBody, "Split Body", "", Tool(ToolKind::SplitBody)),
    t(Icon::ReplaceFace, "Replace Face", "", Tool(ToolKind::ReplaceFace)),
];

// The Drawings workspace's tools, as observed (dimensions and annotations are still to come).
pub const DRAWINGS: &[ToolDef] = &[
    ToolDef { flyout: true, ..t(Icon::DViews, "Views", "", Flyout(Category::Views)) },
    ToolDef { flyout: true, ..t(Icon::DDimensions, "Dimensions", "", Flyout(Category::Dimensions)) },
    ToolDef { flyout: true, ..t(Icon::DGeometries, "Geometries", "", Flyout(Category::Geometries)) },
    t(Icon::DNote, "Note", "", U(UiAction::DrawTool(6))),
    t(Icon::DImage, "Image", "", U(UiAction::InsertImage)),
];

pub const CREATE_DRAWING: &[ToolDef] =
    &[t(Icon::ChevronRight, "Next", "", U(UiAction::DrawingNext)), t(Icon::Close, "Cancel", "", U(UiAction::DrawingCancel))];

const VIEWS: &[ToolDef] = &[
    t(Icon::DViews, "Base View", "", U(UiAction::ViewTool(0))),
    t(Icon::Project, "Projection", "", U(UiAction::ViewTool(1))),
    t(Icon::SectionView, "Section View", "", U(UiAction::ViewTool(2))),
    t(Icon::Circle, "Detail View", "", U(UiAction::ViewTool(3))),
];

pub const VIEW_DONE: &[ToolDef] = &[t(Icon::Check, "Done", "", U(UiAction::ViewDone))];
pub const VIEW_CANCEL: &[ToolDef] = &[t(Icon::Close, "Cancel", "", U(UiAction::ViewCancel))];

const DIMENSIONS: &[ToolDef] = &[
    t(Icon::Measure, "Line Length", "", U(UiAction::DrawTool(0))),
    t(Icon::Measure, "Point-to-Point Distance", "", U(UiAction::DrawTool(1))),
    t(Icon::Measure, "Point-to-Line Distance", "", U(UiAction::DrawTool(7))),
    t(Icon::Measure, "Line-to-Line Distance", "", U(UiAction::DrawTool(8))),
    t(Icon::Arc, "Arc Angle", "", U(UiAction::DrawTool(11))),
    t(Icon::Arc, "3-Point Angle", "", U(UiAction::DrawTool(10))),
    t(Icon::Arc, "Line-to-Line Angle", "", U(UiAction::DrawTool(9))),
    t(Icon::Circle, "Radius", "", U(UiAction::DrawTool(2))),
    t(Icon::Circle, "Diameter", "", U(UiAction::DrawTool(3))),
    t(Icon::Measure, "Min-Max Distance", "", U(UiAction::DrawTool(16))),
];

const GEOMETRIES: &[ToolDef] = &[
    t(Icon::Axis, "2-Point Centerline", "", U(UiAction::DrawTool(4))),
    t(Icon::Axis, "2-Line Centerline", "", U(UiAction::DrawTool(12))),
    t(Icon::Circle, "2-Point Circular Centerline", "", U(UiAction::DrawTool(13))),
    t(Icon::Circle, "3-Point Circular Centerline", "", U(UiAction::DrawTool(14))),
    t(Icon::Plus, "Center Mark", "", U(UiAction::DrawTool(5))),
    t(Icon::Plus, "Intersection Mark", "", U(UiAction::DrawTool(15))),
];

pub const SELECTION_HEAD: ToolDef =
    ToolDef { icon: Icon::DeselectAll, label: "Deselect All", sub: None, keys: "⇧⌘A", action: U(UiAction::DeselectAll), flyout: false };

pub const DELETE_SECTION: ToolDef = t(Icon::Delete, "Delete Section", "", U(UiAction::DeleteSection));
pub const SECTION_2D: ToolDef = t(Icon::Section2D, "2D Section View", "", U(UiAction::Section2D));
pub const SECTION_ONLY: ToolDef = t(Icon::SectionOnly, "Section Only", "", U(UiAction::SectionOnly));
pub const DELETE: ToolDef = t(Icon::Delete, "Delete", "", Command("delete"));

// Flyout contents as observed (screens 50–56).
const INSERT: &[ToolDef] = &[
    t(Icon::Variable, "Variable", "⇧⌘V", U(UiAction::Variables)),
    t(Icon::DImage, "Image", "", U(UiAction::InsertImage)),
    t(Icon::FolderFilled, "File", "⇧⌘I", Command("import_file")),
    t(Icon::Merged, "Project", "⇧⌘P", Soon),
];

const CONSTRUCT: &[ToolDef] = &[
    ToolDef { flyout: true, ..t(Icon::PlaneFlat, "Plane", "", Flyout(Category::Plane)) },
    ToolDef { flyout: true, ..t(Icon::AxisDashed, "Axis", "", Flyout(Category::Axis)) },
];

const PLANES: &[ToolDef] = &[
    t(Icon::Plane, "Offset Plane", "", Tool(ToolKind::OffsetPlane)),
    t(Icon::Plane, "Midplane", "", Command("midplane")),
    t(Icon::Plane, "Plane Through 3 Points", "", Tool(ToolKind::Plane3)),
    t(Icon::Plane, "Plane on Curve at Point", "", Tool(ToolKind::PlaneCurve)),
    t(Icon::Plane, "Plane Perpendicular to Edge at Point", "", Tool(ToolKind::PlanePerpEdge)),
    t(Icon::Plane, "Plane Through Edge at Angle", "", Tool(ToolKind::PlaneEdge)),
    t(Icon::Plane, "Plane Parallel to Face at Point", "", Tool(ToolKind::PlaneParFace)),
];

const AXES_SET: &[ToolDef] = &[
    t(Icon::Axis, "Axis Through 2 Points", "", Tool(ToolKind::Axis2)),
    t(Icon::Axis, "Axis Through 2 Planes", "", Command("axis_2planes")),
    t(Icon::Axis, "Axis of Cylinder/Cone", "", Command("axis_cylinder")),
    t(Icon::Axis, "Axis Along Edge", "", Command("axis_along_edge")),
    t(Icon::Axis, "Axis Perpendicular to Face at Point", "", Command("axis_perp_face")),
];

const TRANSFORM: &[ToolDef] = &[
    t(Icon::MoveRotate, "Move/Rotate", "M", Tool(ToolKind::Move)),
    t(Icon::Scale, "Scale", "S", Tool(ToolKind::Scale)),
    t(Icon::Translate, "Translate", "N", Tool(ToolKind::Translate)),
    t(Icon::Pattern, "Pattern", "", Tool(ToolKind::Pattern)),
    t(Icon::RotateAxis, "Rotate Around Axis", "", Tool(ToolKind::RotateAxis)),
    t(Icon::Align, "Align", "", Tool(ToolKind::Align)),
    t(Icon::Mirror, "Mirror", "", Tool(ToolKind::Mirror)),
];

const TOOLS: &[ToolDef] = &[
    t(Icon::OffsetFace, "Offset Face", "", Tool(ToolKind::OffsetFace)),
    t(Icon::Fillet, "Chamfer/Fillet", "F", Tool(ToolKind::Fillet)),
    t(Icon::Extrude, "Extrude", "E", Tool(ToolKind::Extrude)),
    t(Icon::Shell, "Shell", "H", Tool(ToolKind::Shell)),
    t(Icon::Loft, "Loft", "", Tool(ToolKind::Loft)),
    t(Icon::Union, "Union", "⌘U", Tool(ToolKind::Union)),
    t(Icon::Subtract, "Subtract", "⌘B", Tool(ToolKind::Subtract)),
    t(Icon::Intersect, "Intersect", "⌘I", Tool(ToolKind::Intersect)),
    t(Icon::SplitBody, "Split Body", "", Tool(ToolKind::SplitBody)),
    t(Icon::Revolve, "Revolve", "V", Tool(ToolKind::Revolve)),
    t(Icon::Sweep, "Sweep", "W", Tool(ToolKind::Sweep)),
    t(Icon::ReplaceFace, "Replace Face", "", Tool(ToolKind::ReplaceFace)),
    t(Icon::OffsetEdge, "Offset Edge", "O", Tool(ToolKind::OffsetEdge)),
    t(Icon::Project, "Project", "P", Tool(ToolKind::Project)),
    t(Icon::WrapEmboss, "Wrap & Emboss", "", Tool(ToolKind::WrapEmboss)),
    t(Icon::Visualize, "Visualize", "", U(UiAction::Visualization)),
];

pub fn flyout(cat: Category) -> &'static [ToolDef] {
    match cat {
        Category::Insert => INSERT,
        Category::Construct => CONSTRUCT,
        Category::Plane => PLANES,
        Category::Axis => AXES_SET,
        Category::Views => VIEWS,
        Category::Dimensions => DIMENSIONS,
        Category::Geometries => GEOMETRIES,
        Category::Transform => TRANSFORM,
        Category::Tools => TOOLS,
        Category::Overflow => &[],
    }
}

/// The toolbar entry with this label (tools, flyout items, the column's buttons), for custom keys of things that
/// aren't registry commands.
pub fn def_for_label(label: &str) -> Option<ToolDef> {
    let sets: [&[ToolDef]; 22] = [
        SKETCHING,
        SKETCH_TAIL,
        REGION,
        FACE,
        EDGE,
        BODY,
        CURVED_FACE,
        EDGES,
        FACES,
        MIXED,
        PLANE,
        INSERT,
        CONSTRUCT,
        PLANES,
        AXES_SET,
        TRANSFORM,
        TOOLS,
        IDLE,
        TOP,
        DRAWINGS,
        DIMENSIONS,
        GEOMETRIES,
    ];
    sets.iter()
        .flat_map(|s| s.iter())
        .find(|d| d.label.eq_ignore_ascii_case(label) && !matches!(d.action, Action::Soon | Action::Flyout(_)))
        .copied()
}

/// The toolbar icon of a command's label, for the command search's rows.
pub fn icon_for_label(label: &str) -> Option<Icon> {
    let sets: [&[ToolDef]; 13] = [SKETCHING, SKETCH_TAIL, REGION, FACE, EDGE, BODY, PLANE, INSERT, CONSTRUCT, TRANSFORM, TOOLS, IDLE, TOP];
    let base = label.split(" - ").next().unwrap_or(label);
    sets.iter().flat_map(|s| s.iter()).find(|d| d.label == label || d.label == base).map(|d| d.icon)
}

/// One key cap per glyph ("⇧⌘A" → ⇧ ⌘ A). Returns the width used.
pub fn keycaps(ui: &Ui, left: Pos2, keys: &str, dim: bool) -> f32 {
    let mut x = left.x;
    let font = theme::body(9.5);
    for cap in theme::key_caps(keys) {
        let galley = ui.painter().layout_no_wrap(cap.clone(), font.clone(), Tokens::text_dim());
        let w = galley.size().x.max(7.0) + 7.0;
        let r = Rect::from_min_size(pos2(x, left.y - 8.0), vec2(w, 16.0));
        ui.painter().rect_filled(r, radius(4), if dim { Tokens::chip() } else { Tokens::keycap() });
        let color = if dim { Tokens::text_ghost() } else { Tokens::text_dim() };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, cap, font.clone(), color);
        x += w + 3.0;
    }
    x - left.x
}

/// The label chip beside a tool button. `left_side` puts it to the left of `anchor` (right-hand column).
pub fn chip(ui: &Ui, anchor: Pos2, def: &ToolDef, left_side: bool, dim: bool) -> Rect {
    chip_with(ui, anchor, def, def.sub, left_side, dim, false)
}

/// Like [`chip`], with a sub-label decided at run time (sketch name, selection count).
/// Whether a running tool's chip shows the ≡ settings glyph (26.200: modeling tools and Offset Edge, not the
/// drawing tools).
fn has_settings_glyph(def: &ToolDef) -> bool {
    use ToolKind as K;
    matches!(def.action, Tool(k) if !matches!(k, K::Line | K::Arc | K::Spline | K::Rectangle | K::Circle | K::Ellipse | K::Polygon | K::Trim | K::Text | K::Sketch))
}

pub fn chip_with(ui: &Ui, anchor: Pos2, def: &ToolDef, sub_text: Option<&str>, left_side: bool, dim: bool, hot: bool) -> Rect {
    chip_full(ui, anchor, def, sub_text, left_side, dim, hot, false)
}

#[allow(clippy::too_many_arguments)]
pub fn chip_full(ui: &Ui, anchor: Pos2, def: &ToolDef, sub_text: Option<&str>, left_side: bool, dim: bool, hot: bool, gear: bool) -> Rect {
    let painter = ui.painter();
    // An empty label shows the run-time text as the label itself (the sketch name row).
    let (title, sub_text) = if def.label.is_empty() { (sub_text.unwrap_or(""), None) } else { (def.label, sub_text) };
    let tr = crate::i18n::tr;
    let label = painter.layout_no_wrap(tr(title).to_string(), theme::strong(13.5), Tokens::text_strong());
    let sub = sub_text.map(|s| painter.layout_no_wrap(tr(s).to_string(), theme::body(9.5), Tokens::text_dim()));
    let caps_w: f32 = theme::key_caps(def.keys)
        .into_iter()
        .map(|c| painter.layout_no_wrap(c, theme::body(9.5), Tokens::text_dim()).size().x.max(7.0) + 10.0)
        .sum();
    let text_w = label.size().x.max(sub.as_ref().map_or(0.0, |s| s.size().x));
    let caps_gap = if def.keys.is_empty() { 0.0 } else { 6.0 };
    let gear_w = if gear { 20.0 } else { 0.0 };
    let w = 8.0 + text_w + caps_gap + caps_w + 7.0 + gear_w;
    let h = Tokens::CHIP_H;
    let min = if left_side { pos2(anchor.x - w, anchor.y - h / 2.0) } else { pos2(anchor.x, anchor.y - h / 2.0) };
    let rect = Rect::from_min_size(min, vec2(w, h));
    painter.rect_filled(rect, radius(Tokens::CHIP_RADIUS), if hot { Tokens::chip_hover() } else { Tokens::chip() });
    painter.rect_stroke(rect, radius(Tokens::CHIP_RADIUS), Stroke::new(1.0, Tokens::chip_edge()), StrokeKind::Inside);
    let text_color = if dim { Tokens::text_ghost() } else { Tokens::text_strong() };
    let (label_y, sub_y) = if sub.is_some() { (rect.center().y - 6.0, rect.center().y + 7.0) } else { (rect.center().y, 0.0) };
    // A chip left of its button (right column, constraints) mirrors: key caps first, text right-aligned (L05, 12).
    let (text_x, align) = if left_side { (rect.right() - 8.0, Align2::RIGHT_CENTER) } else { (rect.left() + 8.0, Align2::LEFT_CENTER) };
    painter.text(pos2(text_x, label_y), align, tr(title), theme::strong(13.5), text_color);
    if let Some(s) = sub_text {
        painter.text(pos2(text_x, sub_y), align, tr(s), theme::body(9.5), Tokens::text_dim());
    }
    if !def.keys.is_empty() {
        let x =
            if left_side { rect.right() - 8.0 - label.size().x - caps_gap - caps_w } else { rect.left() + 8.0 + label.size().x + caps_gap };
        keycaps(ui, pos2(x, label_y), def.keys, dim);
    }
    if gear {
        // ≡ with a knob on each bar: the tool's settings (s11-099/108/111).
        let c = pos2(rect.right() - 15.0, rect.center().y);
        for (k, dx) in [(-1.0f32, 2.5f32), (0.0, -2.5), (1.0, 1.5)] {
            let y = c.y + k * 3.5;
            painter.line_segment([pos2(c.x - 6.0, y), pos2(c.x + 6.0, y)], Stroke::new(1.1, Tokens::text_dim()));
            painter.circle_filled(pos2(c.x + dx, y), 1.6, Tokens::text_dim());
        }
    }
    rect
}

/// A group of stacked buttons in one rounded container. Returns the clicked index.
pub fn group(ui: &mut Ui, top_left: Pos2, defs: &[ToolDef], active: &[bool], chips: bool, id: &str) -> Option<usize> {
    group_with(ui, top_left, defs, &[], active, chips, id)
}

/// [`group`] with run-time sub-labels (`subs[i]` overrides `defs[i].sub` when present).
pub fn group_with(
    ui: &mut Ui,
    top_left: Pos2,
    defs: &[ToolDef],
    subs: &[Option<String>],
    active: &[bool],
    chips: bool,
    id: &str,
) -> Option<usize> {
    group_full(ui, top_left, defs, subs, active, &[], chips, id, None)
}

/// [`group_with`] plus per-button disabled flags (drawn ghosted, like the reference's idle Delete).
#[allow(clippy::too_many_arguments)]
pub fn group_full(
    ui: &mut Ui,
    top_left: Pos2,
    defs: &[ToolDef],
    subs: &[Option<String>],
    active: &[bool],
    disabled: &[bool],
    chips: bool,
    id: &str,
    split: Option<usize>,
) -> Option<usize> {
    let disabled_at = |i: usize| disabled.get(i).copied().unwrap_or(false);
    if defs.is_empty() {
        return None;
    }
    let pitch = Tokens::TOOL_PITCH;
    let w = Tokens::TOOL_BUTTON;
    let extra = if split.is_some() { SPLIT_GAP } else { 0.0 };
    let container = Rect::from_min_size(top_left, vec2(w, group_height(defs.len()) + extra));
    ui.painter().rect_filled(container, radius(Tokens::TOOL_RADIUS), Tokens::button());
    ui.painter().rect_stroke(container, radius(Tokens::TOOL_RADIUS), Stroke::new(1.0, Tokens::tool_border()), StrokeKind::Inside);
    // A thin line in the gap before `split` (the More button of a cut-down set, as observed).
    if let Some(k) = split {
        let y = top_left.y + k as f32 * pitch + extra / 2.0 + 0.5;
        ui.painter().line_segment([pos2(top_left.x + 11.0, y), pos2(top_left.x + w - 11.0, y)], Stroke::new(2.0, Tokens::tool_separator()));
    }
    let mut clicked = None;
    for (i, def) in defs.iter().enumerate() {
        let below = if split.is_some_and(|k| i >= k) { extra } else { 0.0 };
        let r = Rect::from_min_size(pos2(top_left.x, top_left.y + 1.0 + i as f32 * pitch + below), vec2(w, pitch));
        let resp: Response = ui.interact(r, ui.scope_id().with((id, i)), Sense::click());
        let disabled = disabled_at(i);
        // The label chip is part of the button, as in the reference.
        // Hover is decided before painting so the chip can light up with the icon cell (it uses last frame's
        // chip rect, which is where the chip still is).
        let chip_key = ui.scope_id().with((id, i, "chip-rect"));
        let last_chip: Option<Rect> = ui.ctx().data(|d| d.get_temp(chip_key));
        let pointer = ui.input(|inp| inp.pointer.hover_pos());
        let over_chip = chips && pointer.is_some_and(|p| last_chip.is_some_and(|c| c.contains(p)));
        let hovered = resp.hovered() || over_chip;
        let gear = active.get(i).copied().unwrap_or(false) && has_settings_glyph(def);
        let chip_rect = chips.then(|| {
            let sub = subs.get(i).and_then(|s| s.as_deref()).or(def.sub);
            chip_full(ui, pos2(r.right() + 6.0, r.center().y), def, sub, false, disabled, hovered && !disabled, gear)
        });
        if let Some(cr) = chip_rect {
            ui.ctx().data_mut(|d| d.insert_temp(chip_key, cr));
        }
        let chip_resp = chip_rect.map(|cr| ui.interact(cr, ui.scope_id().with((id, i, "chip")), Sense::click()));
        let is_active = active.get(i).copied().unwrap_or(false);
        let inner = r.shrink(3.0);
        // A tool's Done and Cancel are tinted, with a blue ✓ and a red ✕ (measured, 326).
        let tint = match def.action {
            Action::Ui(UiAction::ToolDone | UiAction::ViewDone) => Some((theme::c(0x2e385b), Tokens::sketch_blue())),
            Action::Ui(UiAction::ToolCancel | UiAction::DrawingCancel | UiAction::ViewCancel) => {
                Some((theme::c(0x5b3536), theme::c(0xff4245)))
            }
            _ => None,
        };
        if let Some((fill, _)) = tint {
            ui.painter().rect_filled(inner, radius(7), if hovered { fill.gamma_multiply(1.25) } else { fill });
        } else if is_active && matches!(def.action, Action::Tool(_)) {
            // The running tool's button is white with a dark icon (26.200, s11-007); toggles stay grey.
            ui.painter().rect_filled(inner, radius(7), Color32::WHITE);
        } else if is_active {
            ui.painter().rect_filled(inner, radius(7), Tokens::button_active());
        } else if hovered && !disabled {
            ui.painter().rect_filled(inner, radius(7), Tokens::button_hover());
        }
        let color = if disabled {
            Tokens::text_ghost()
        } else if let Some((_, ink)) = tint {
            ink
        } else if is_active && matches!(def.action, Action::Tool(_)) {
            // Dark on the white cell in both themes (the Light theme's button colour is near white).
            if theme::is_light() { super::viewport::PILL_INK } else { Tokens::button() }
        } else if def.action == Action::Soon {
            hex_dim(Tokens::icon())
        } else {
            Tokens::icon()
        };
        icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(28.0, 28.0)), def.icon, color);
        if def.flyout {
            let c = r.right_bottom() + vec2(-6.0, -6.0);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![c, c + vec2(-4.0, 0.0), c + vec2(0.0, -4.0)],
                Tokens::text_dim(),
                Stroke::NONE,
            ));
        }
        let was_clicked = resp.clicked() || chip_resp.is_some_and(|c| c.clicked());
        if was_clicked && !disabled {
            clicked = Some(i);
        }
    }
    clicked
}

fn hex_dim(c: Color32) -> Color32 {
    c.gamma_multiply(0.92)
}

/// The extra room around the line before a split group's last part.
pub const SPLIT_GAP: f32 = 6.0;
/// Between a toolbar column's groups (measured, L05).
pub const GROUP_GAP: f32 = 6.0;

/// Total height of a group: 40 per button and the 1 px border top and bottom (measured, L05).
pub fn group_height(n: usize) -> f32 {
    Tokens::TOOL_PITCH * n as f32 + 2.0
}
