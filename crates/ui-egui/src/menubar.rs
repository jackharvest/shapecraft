//! The native menu bar (macOS), laid out like the reference app's (plan/shapr3d/04-menu-tree-workspace.txt)
//! and wired to the same actions as the toolbar and shortcuts.
//!
//! Only shortcuts with a modifier are registered with the menu. Bare-key tools (L, R, E…) stay with the
//! workspace's own key handling, so typing into a field never triggers a tool, and ⌘C/⌘V/⌘A keep working in
//! text fields.

use shapecraft_render::StandardView;

use crate::theme::{self, Tokens};
use crate::workspace::export::Format;
use crate::workspace::toolbar::{ToolKind, UiAction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    NewProject,
    CloseProject,
    Save,
    Import,
    ImportProject,
    Export,
    /// Opens the Export dialog with one format picked.
    ExportAs(Format),
    /// The last export's formats into its folder again.
    QuickExport,
    Quit,
    GoHome,
    /// Help ▸ Check for Updates…
    CheckUpdates,
    /// Help ▸ Get Started / Tutorials: closes the project and opens that page of the home screen, as the reference does.
    GetStarted,
    Tutorials,
    /// Help: opens a web page in the browser.
    OpenPage(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuAction {
    App(AppAction),
    Command(&'static str),
    Tool(ToolKind),
    Ui(UiAction),
    View(StandardView),
    Display(shapecraft_render::DisplayMode),
    Soon(&'static str),
    /// A tool started as one of its types: (tool, the toolbar's variant key, which type).
    Variant(ToolKind, &'static str, usize),
    /// A home-screen item (New Folder, Go ▸ Back, View ▸ Sort By …).
    Home(crate::landing::HomeCmd),
}

/// When an item is enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    /// A note in the menu, never clickable.
    Never,
    /// Only on the home screen.
    Home,
    InProject,
    Bodies,
    TwoBodies,
    Selection,
    Sketching,
    Undo,
    Redo,
    /// In a project, outside Visualization (it has no History sidebar).
    Modeling,
    /// In Visualization.
    Visualizing,
    /// In the Drawings sheet.
    Drawing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    pub in_project: bool,
    pub bodies: usize,
    pub selection: bool,
    pub sketching: bool,
    pub undo: bool,
    pub redo: bool,
    /// The grid's spacing in micrometres (0 outside a project), for "Lock Grid Size (1 mm)".
    pub grid_um: u64,
    pub grid_locked: bool,
    pub visualizing: bool,
    pub drawing: bool,
    /// Which items carry a check mark: one bit per [`check_bit`].
    pub checks: u32,
}

/// The items that show a check mark when on (as the reference's View menu does, s40), and their bit in
/// [`MenuState::checks`].
pub fn check_bit(action: &MenuAction) -> Option<u32> {
    use shapecraft_render::DisplayMode as D;
    Some(match action {
        MenuAction::Ui(UiAction::ToggleHistory) => 0,
        MenuAction::Ui(UiAction::ToggleItems) => 1,
        MenuAction::Ui(UiAction::ToggleGrid) => 2,
        MenuAction::Ui(UiAction::ToggleAxes) => 3,
        MenuAction::Ui(UiAction::ToggleEdges) => 4,
        MenuAction::Ui(UiAction::ToggleSketches) => 5,
        MenuAction::Ui(UiAction::ToggleHiddenEdges) => 6,
        MenuAction::Ui(UiAction::ToggleDecals) => 7,
        MenuAction::Ui(UiAction::TogglePinned) => 8,
        MenuAction::Display(D::Shaded) => 9,
        MenuAction::Display(D::Wireframe) => 10,
        MenuAction::Display(D::XRay) => 11,
        MenuAction::Display(D::Zebra { .. }) => 12,
        MenuAction::Display(D::Curvature { .. }) => 13,
        MenuAction::Ui(UiAction::Units(i)) if *i < 5 => 14 + u32::from(*i),
        MenuAction::Ui(UiAction::FeetArchitectural(on)) => 19 + u32::from(!on),
        MenuAction::Ui(UiAction::DegreesFractional(on)) => 21 + u32::from(!on),
        _ => return None,
    })
}

impl MenuState {
    /// Whether a check item is on (`None` for an item without a check mark).
    pub fn checked(&self, action: &MenuAction) -> Option<bool> {
        check_bit(action).map(|b| self.checks & (1 << b) != 0)
    }
}

/// "Lock Grid Size (1 mm)", or "Unlock Grid Size (1 mm)" once locked.
pub fn grid_label(s: &MenuState) -> String {
    let mm = s.grid_um as f64 / 1000.0;
    let size = format!("{mm:.3}");
    let size = size.trim_end_matches('0').trim_end_matches('.');
    let what = crate::i18n::tr(if s.grid_locked { "Unlock Grid Size" } else { "Lock Grid Size" });
    if s.grid_um == 0 { what.to_string() } else { format!("{what} ({size} mm)") }
}

impl When {
    pub fn holds(self, s: &MenuState) -> bool {
        match self {
            When::Always => true,
            When::Never => false,
            When::Home => !s.in_project,
            When::InProject => s.in_project,
            When::Bodies => s.in_project && s.bodies > 0,
            When::TwoBodies => s.in_project && s.bodies > 1,
            When::Selection => s.in_project && s.selection,
            When::Sketching => s.in_project && s.sketching,
            When::Undo => s.in_project && s.undo,
            When::Redo => s.in_project && s.redo,
            When::Modeling => s.in_project && !s.visualizing,
            When::Visualizing => s.in_project && s.visualizing,
            When::Drawing => s.in_project && s.drawing,
        }
    }
}

pub enum Entry {
    Item { label: &'static str, accel: Option<&'static str>, action: MenuAction, when: When },
    Separator,
    Sub { label: &'static str, items: Vec<Entry> },
}

fn item(label: &'static str, accel: Option<&'static str>, action: MenuAction, when: When) -> Entry {
    Entry::Item { label, accel, action, when }
}

fn soon(label: &'static str) -> Entry {
    item(label, None, MenuAction::Soon(label), When::InProject)
}

/// Help menu links.
pub const HELP: &str = "https://github.com/jackharvest/shapecraft#readme";
pub const WEBSITE: &str = "https://jackharvest.com/shapecraft/";
pub const RELEASES: &str = "https://github.com/jackharvest/shapecraft/releases";
pub const ISSUES: &str = "https://github.com/jackharvest/shapecraft/issues/new/choose";

/// The menus after the application menu inside a project, in the reference's order.
pub fn tree() -> Vec<(&'static str, Vec<Entry>)> {
    use AppAction::*;
    use MenuAction::{App, Command, Display, Tool, Ui, View};
    use When::*;
    let sep = || Entry::Separator;
    // As 26.200 has them (plan/shapr3d/04-menu-tree-workspace-26200.txt), less the cloud items (View in Browser,
    // Copy Link, Share…, Publish…) and the online help links; Save and Projects are ours (local files). The tools'
    // single keys show beside them (L, A, C…); a bare key typed in the workspace still goes to the
    // workspace (see `bare_key`), so typing into fields and the keys' in-tool meanings (E flips Sides…) stay.
    vec![
        (
            "File",
            vec![
                item("New Project", Some("CmdOrCtrl+N"), App(NewProject), Always),
                sep(),
                item("New Drawing", None, Ui(UiAction::CreateDrawing), Bodies),
                item("Import File…", Some("CmdOrCtrl+Shift+I"), App(Import), Always),
                item("Import Project…", Some("CmdOrCtrl+Shift+P"), App(ImportProject), InProject),
                item("Import Material…", None, Ui(UiAction::ImportMaterial), InProject),
                item("Import Image as Decal...", None, Ui(UiAction::ImportDecal), InProject),
                sep(),
                item("Save", Some("CmdOrCtrl+S"), App(Save), InProject),
                item("Close Project", Some("CmdOrCtrl+W"), App(CloseProject), InProject),
                sep(),
                item("Rename Project…", Some("CmdOrCtrl+Alt+R"), Ui(UiAction::RenameProject), InProject),
                sep(),
                Entry::Sub {
                    label: "Export to",
                    items: vec![
                        item("Export…", Some("CmdOrCtrl+Shift+E"), App(Export), Bodies),
                        item("Export to Slicer…", None, App(ExportAs(Format::Slicer)), Bodies),
                        item("Quick Export to Favorite Formats", None, App(QuickExport), Bodies),
                        item("Open Screenshot Tool…", None, Ui(UiAction::Screenshot), InProject),
                        item("Open Visualization…", None, Ui(UiAction::Visualization), InProject),
                        sep(),
                        item(".shapr…", None, App(ExportAs(Format::Shapr)), Bodies),
                        item("3MF…", None, App(ExportAs(Format::ThreeMf)), Bodies),
                        item("Parasolid file…", None, App(ExportAs(Format::Parasolid)), Bodies),
                        item("STEP…", None, App(ExportAs(Format::Step)), Bodies),
                        item("DXF from Sketch…", None, App(ExportAs(Format::DxfSketch)), InProject),
                        item("DXF from Drawing…", None, App(ExportAs(Format::DxfDrawing)), Bodies),
                        item("IGES…", None, App(ExportAs(Format::Iges)), Bodies),
                        item("STL…", None, App(ExportAs(Format::Stl)), Bodies),
                        item("USDZ…", None, App(ExportAs(Format::Usdz)), Bodies),
                        soon("JT…"),
                        item("GLB…", None, App(ExportAs(Format::Glb)), Bodies),
                        item("OBJ…", None, App(ExportAs(Format::Obj)), Bodies),
                        item("DWG from Sketch…", None, App(ExportAs(Format::DwgSketch)), InProject),
                        item("DWG from Drawing…", None, App(ExportAs(Format::DwgDrawing)), Bodies),
                        item("SVG…", None, App(ExportAs(Format::Svg)), InProject),
                        item("PDF…", None, App(ExportAs(Format::Pdf)), Bodies),
                    ],
                },
                sep(),
                item("Projects", None, App(GoHome), InProject),
            ],
        ),
        (
            "Edit",
            vec![
                item("Undo", Some("CmdOrCtrl+Z"), Command("undo"), Undo),
                item("Redo", Some("CmdOrCtrl+Shift+Z"), Command("redo"), Redo),
                sep(),
                // No accelerators: ⌘C / ⌘V must still reach text fields; the workspace takes them otherwise.
                item("Copy", None, Ui(UiAction::Copy), Selection),
                item("Paste", None, Ui(UiAction::Paste), InProject),
                item("Delete", None, Command("delete"), Selection),
                item("Rename", Some("CmdOrCtrl+R"), Ui(UiAction::Rename), Selection),
                item("Select All", Some("CmdOrCtrl+A"), Command("select_all"), Bodies),
                item("Select Through...", Some("CmdOrCtrl+Shift+S"), Ui(UiAction::SelectThrough), Bodies),
                item("Deselect All", Some("CmdOrCtrl+Shift+A"), Ui(UiAction::DeselectAll), Selection),
            ],
        ),
        (
            "Item",
            vec![
                item("Show/Hide", Some("CmdOrCtrl+Shift+H"), Command("show_hide"), Selection),
                sep(),
                item("Isolate", None, Ui(UiAction::Isolate), Selection),
                item("Exit Isolation", None, Ui(UiAction::Isolate), InProject),
                sep(),
                item("Zoom to", None, Ui(UiAction::ZoomToSelection), Selection),
                item("Reveal in Items Sidebar", Some("Alt+Shift+I"), Ui(UiAction::RevealInItems), Selection),
                sep(),
                item("Show Hidden Bodies", Some("Alt+H"), Ui(UiAction::ShowHiddenBodies), InProject),
                item("Invert Body Visibility", Some("Alt+Shift+H"), Ui(UiAction::InvertVisibility), InProject),
            ],
        ),
        (
            "Sketch",
            vec![
                item("Line", Some("L"), Tool(ToolKind::Line), InProject),
                item("Arc", Some("A"), Tool(ToolKind::Arc), InProject),
                Entry::Sub {
                    label: "Spline",
                    items: vec![
                        item("Recent", Some("I"), Tool(ToolKind::Spline), InProject),
                        sep(),
                        item("Control", None, MenuAction::Variant(ToolKind::Spline, "Spline", 0), InProject),
                        item("Fit", None, MenuAction::Variant(ToolKind::Spline, "Spline", 1), InProject),
                    ],
                },
                Entry::Sub {
                    label: "Rectangle",
                    items: vec![
                        item("Recent", Some("R"), Tool(ToolKind::Rectangle), InProject),
                        sep(),
                        item("Center", None, MenuAction::Variant(ToolKind::Rectangle, "Rectangle", 0), InProject),
                        item("Diagonal", None, MenuAction::Variant(ToolKind::Rectangle, "Rectangle", 1), InProject),
                        item("3 Points", None, MenuAction::Variant(ToolKind::Rectangle, "Rectangle", 2), InProject),
                    ],
                },
                item("Circle", Some("C"), Tool(ToolKind::Circle), InProject),
                item("Ellipse", None, Tool(ToolKind::Ellipse), InProject),
                Entry::Sub {
                    label: "Polygon",
                    items: vec![
                        item("Recent", Some("G"), Tool(ToolKind::Polygon), InProject),
                        sep(),
                        item("Triangle", None, MenuAction::Variant(ToolKind::Polygon, "Polygon", 0), InProject),
                        item("Pentagon", None, MenuAction::Variant(ToolKind::Polygon, "Polygon", 1), InProject),
                        item("Hexagon", None, MenuAction::Variant(ToolKind::Polygon, "Polygon", 2), InProject),
                        item("Octagon", None, MenuAction::Variant(ToolKind::Polygon, "Polygon", 3), InProject),
                    ],
                },
                sep(),
                item("Project as Sketch", Some("P"), Tool(ToolKind::Project), InProject),
                // A plain item: 26.202 has no submenu here (s39 menu dump).
                item("Offset Edge", None, Tool(ToolKind::OffsetEdge), InProject),
                item("Fillet Sketch", None, Tool(ToolKind::SketchFillet), Sketching),
                item("Text…", None, Tool(ToolKind::Text), InProject),
                sep(),
                item("Scale Sketch", None, Tool(ToolKind::Scale), Selection),
                item("Pattern - Linear", None, Tool(ToolKind::Pattern), Selection),
                item("Pattern - Circular", None, Tool(ToolKind::CircularPattern), Selection),
                item("Rotate Sketch Around Axis", None, Tool(ToolKind::RotateAxis), Selection),
                item("Mirror Sketch", None, Tool(ToolKind::Mirror), Selection),
                sep(),
                item("Trim", Some("T"), Tool(ToolKind::Trim), InProject),
            ],
        ),
        (
            "Insert",
            vec![
                item("Variable…", Some("CmdOrCtrl+Shift+V"), Ui(UiAction::Variables), InProject),
                item("Image…", None, Ui(UiAction::InsertImage), InProject),
                item("File…", None, App(Import), Always),
                item("Import Project…", None, App(ImportProject), InProject),
            ],
        ),
        (
            "Construct",
            vec![
                Entry::Sub {
                    label: "Plane",
                    items: vec![
                        item("Offset Plane", None, Tool(ToolKind::OffsetPlane), Selection),
                        item("Midplane", None, Command("midplane"), Selection),
                        item("Plane Through 3 Points", None, Tool(ToolKind::Plane3), InProject),
                        item("Plane on Curve at Point", None, Tool(ToolKind::PlaneCurve), Selection),
                        item("Plane Perpendicular to Edge at Point", None, Tool(ToolKind::PlanePerpEdge), InProject),
                        item("Plane Through Edge at Angle", None, Tool(ToolKind::PlaneEdge), Selection),
                        item("Plane Parallel to Face at Point", None, Tool(ToolKind::PlaneParFace), InProject),
                    ],
                },
                Entry::Sub {
                    label: "Axis",
                    items: vec![
                        item("Axis Through 2 Points", None, Tool(ToolKind::Axis2), InProject),
                        item("Axis Through 2 Planes", None, Command("axis_2planes"), Selection),
                        item("Axis of Cylinder/Cone", None, Command("axis_cylinder"), Selection),
                        item("Axis Along Edge", None, Command("axis_along_edge"), Selection),
                        item("Axis Perpendicular to Face at Point", None, Command("axis_perp_face"), Selection),
                    ],
                },
            ],
        ),
        (
            "Transform",
            vec![
                item("Move/Rotate", Some("M"), Tool(ToolKind::Move), InProject),
                item("Scale", Some("S"), Tool(ToolKind::Scale), InProject),
                item("Translate", Some("N"), Tool(ToolKind::Translate), InProject),
                Entry::Sub {
                    label: "Pattern",
                    items: vec![
                        item("Pattern - Linear", None, Tool(ToolKind::Pattern), InProject),
                        item("Pattern - Circular", None, Tool(ToolKind::CircularPattern), InProject),
                    ],
                },
                item("Rotate Around Axis", None, Tool(ToolKind::RotateAxis), InProject),
                item("Align", None, Tool(ToolKind::Align), InProject),
                item("Mirror", None, Tool(ToolKind::Mirror), InProject),
            ],
        ),
        (
            "Tools",
            vec![
                item("Offset Face", None, Tool(ToolKind::OffsetFace), InProject),
                item("Fillet", Some("F"), Tool(ToolKind::Fillet), Bodies),
                item("Chamfer", None, Tool(ToolKind::Chamfer), Bodies),
                item("Extrude", Some("E"), Tool(ToolKind::Extrude), InProject),
                item("Shell", Some("H"), Tool(ToolKind::Shell), Bodies),
                item("Loft", None, Tool(ToolKind::Loft), InProject),
                sep(),
                item("Union", Some("CmdOrCtrl+U"), Tool(ToolKind::Union), TwoBodies),
                item("Subtract", Some("CmdOrCtrl+B"), Tool(ToolKind::Subtract), TwoBodies),
                item("Intersect", Some("CmdOrCtrl+I"), Tool(ToolKind::Intersect), TwoBodies),
                sep(),
                item("Split Body", None, Tool(ToolKind::SplitBody), Bodies),
                item("Revolve", Some("V"), Tool(ToolKind::Revolve), InProject),
                item("Sweep", Some("W"), Tool(ToolKind::Sweep), InProject),
                item("Fill (Beta)", None, Command("fill"), Selection),
                item("Replace Face", None, Tool(ToolKind::ReplaceFace), Bodies),
                item("Project as Edge", None, Tool(ToolKind::ProjectEdge), Bodies),
                item("Wrap & Emboss", None, Tool(ToolKind::WrapEmboss), Bodies),
                sep(),
                item("Visualize", None, Ui(UiAction::Visualization), InProject),
                sep(),
                item("Section View", None, Ui(UiAction::SectionView), Bodies),
                item("Edit Section View", None, Ui(UiAction::EditSection), Bodies),
                item("2D Section View", None, Ui(UiAction::Section2D), Bodies),
                Entry::Sub {
                    label: "Measure",
                    items: vec![
                        item("Recent", None, Ui(UiAction::Measure), InProject),
                        sep(),
                        item("Object Measurements", None, Ui(UiAction::MeasureMode(0)), InProject),
                        item("Point-to-Point Distance", None, Ui(UiAction::MeasureMode(1)), InProject),
                        item("3-Point Angle", None, Ui(UiAction::MeasureMode(2)), InProject),
                    ],
                },
                sep(),
                item("Sketch", None, Tool(ToolKind::Sketch), InProject),
            ],
        ),
        (
            "View",
            vec![
                item("Show History Sidebar", Some("CmdOrCtrl+Alt+P"), Ui(UiAction::ToggleHistory), Modeling),
                item("Show Items Sidebar", Some("CmdOrCtrl+Alt+S"), Ui(UiAction::ToggleItems), InProject),
                sep(),
                item("Show Properties Sidebar", None, Ui(UiAction::DrawingProps), InProject),
                sep(),
                // The properties sidebar's pages (26.202: lit in Visualization, Drawing Properties in Drawings).
                item("Material Properties", None, Ui(UiAction::VisTab(0)), Visualizing),
                item("Environment Properties", None, Ui(UiAction::VisTab(1)), Visualizing),
                item("Camera Properties", None, Ui(UiAction::VisTab(2)), Visualizing),
                item("Drawing Properties", None, Ui(UiAction::ShowDrawingProps), Drawing),
                sep(),
                item("Collapse All", Some("Alt+L"), Ui(UiAction::CollapseAll), InProject),
                sep(),
                item("Default View", Some("CmdOrCtrl+1"), View(StandardView::Default), InProject),
                item("Front", Some("CmdOrCtrl+2"), View(StandardView::Front), InProject),
                item("Back", Some("CmdOrCtrl+3"), View(StandardView::Back), InProject),
                item("Top", Some("CmdOrCtrl+4"), View(StandardView::Top), InProject),
                item("Bottom", Some("CmdOrCtrl+5"), View(StandardView::Bottom), InProject),
                item("Right", Some("CmdOrCtrl+6"), View(StandardView::Right), InProject),
                item("Left", Some("CmdOrCtrl+7"), View(StandardView::Left), InProject),
                item("Nearest Ortho View", Some("CmdOrCtrl+8"), Ui(UiAction::NearestOrtho), InProject),
                Entry::Sub {
                    label: "Rotate View",
                    items: vec![
                        item("Rotate Left by 15°", None, Ui(UiAction::RotateView(0)), InProject),
                        item("Rotate Right by 15°", None, Ui(UiAction::RotateView(1)), InProject),
                        item("Rotate Up by 15°", None, Ui(UiAction::RotateView(2)), InProject),
                        item("Rotate Down by 15°", None, Ui(UiAction::RotateView(3)), InProject),
                        sep(),
                        item("Rotate Left by 45°", None, Ui(UiAction::RotateView(4)), InProject),
                        item("Rotate Right by 45°", None, Ui(UiAction::RotateView(5)), InProject),
                        item("Rotate Up by 45°", None, Ui(UiAction::RotateView(6)), InProject),
                        item("Rotate Down by 45°", None, Ui(UiAction::RotateView(7)), InProject),
                    ],
                },
                sep(),
                Entry::Sub {
                    label: "Grid Position",
                    items: vec![
                        item("XY Plane", None, Ui(UiAction::GridPosition(0)), InProject),
                        item("YZ Plane", None, Ui(UiAction::GridPosition(1)), InProject),
                        item("ZX Plane", None, Ui(UiAction::GridPosition(2)), InProject),
                    ],
                },
                item("Show Grid", None, Ui(UiAction::ToggleGrid), InProject),
                item("Show World Axes", None, Ui(UiAction::ToggleAxes), InProject),
                sep(),
                item("Field of View…", None, Ui(UiAction::DisplayModes), InProject),
                sep(),
                item("Wireframe", Some("Alt+W"), Display(shapecraft_render::DisplayMode::Wireframe), InProject),
                item("X-Ray", Some("Alt+X"), Display(shapecraft_render::DisplayMode::XRay), InProject),
                item("Shaded", Some("Alt+S"), Display(shapecraft_render::DisplayMode::Shaded), InProject),
                item("Visualized", Some("Alt+V"), Ui(UiAction::Visualized), InProject),
                item(
                    "Zebra Analysis",
                    Some("Alt+Z"),
                    Display(shapecraft_render::DisplayMode::Zebra { vertical: false, scale: 100 }),
                    InProject,
                ),
                item("Curvature Map", Some("Alt+C"), Display(shapecraft_render::DisplayMode::Curvature { scale: 100 }), InProject),
                sep(),
                item("Show Edges", None, Ui(UiAction::ToggleEdges), InProject),
                item("Show Sketches", None, Ui(UiAction::ToggleSketches), InProject),
                item("Show Hidden Edges", None, Ui(UiAction::ToggleHiddenEdges), InProject),
                item("Show Decals", None, Ui(UiAction::ToggleDecals), InProject),
                sep(),
                item("Show Pinned Measurements", None, Ui(UiAction::TogglePinned), InProject),
                sep(),
                item("Go to Modeling", None, Ui(UiAction::Modeling), InProject),
                sep(),
                Entry::Sub {
                    label: "Units",
                    items: vec![
                        item("Millimeter", None, Ui(UiAction::Units(0)), InProject),
                        item("Centimeter", None, Ui(UiAction::Units(1)), InProject),
                        item("Meter", None, Ui(UiAction::Units(2)), InProject),
                        item("Inch", None, Ui(UiAction::Units(3)), InProject),
                        item("Foot", None, Ui(UiAction::Units(4)), InProject),
                    ],
                },
                Entry::Sub {
                    label: "Foot & Inch Format",
                    items: vec![
                        item("Architectural (e.g. 1' - 4 1/2\")", None, Ui(UiAction::FeetArchitectural(true)), InProject),
                        item("Decimal (e.g. 1.375')", None, Ui(UiAction::FeetArchitectural(false)), InProject),
                    ],
                },
                Entry::Sub {
                    label: "Degree Format",
                    items: vec![
                        item("Fractional (e.g. 27°15')", None, Ui(UiAction::DegreesFractional(true)), InProject),
                        item("Decimal (e.g. 27.25°)", None, Ui(UiAction::DegreesFractional(false)), InProject),
                    ],
                },
                // The live grid size goes in its label (native menu), then a tip that's never enabled (26.200).
                item("Lock Grid Size", None, Ui(UiAction::LockGrid), InProject),
                item("Tip: Adjust grid size by zooming", None, MenuAction::Soon("Tip"), Never),
            ],
        ),
        (
            "Help",
            vec![
                // The reference's order (26.142, s35): Help Center | shortcuts, presets, search | Get Started,
                // Tutorials, Community | Contact Support. Ours: the README for Help Center, the website for
                // Community, GitHub issues for support (feedback is issues-only), then updates.
                item("ShapeCraft Help", None, App(OpenPage(HELP)), Always),
                sep(),
                item("Keyboard Shortcuts…", None, Ui(UiAction::Shortcuts), InProject),
                item("Navigation Presets…", None, Ui(UiAction::NavPresets), InProject),
                item("Search Commands", Some("CmdOrCtrl+F"), Ui(UiAction::Search), InProject),
                sep(),
                item("Get Started", None, App(GetStarted), Always),
                item("Tutorials", None, App(Tutorials), Always),
                item("ShapeCraft Website", None, App(OpenPage(WEBSITE)), Always),
                sep(),
                item("Report a Problem or Idea…", None, App(OpenPage(ISSUES)), Always),
                sep(),
                item("Release Notes", None, App(OpenPage(RELEASES)), Always),
                item("Check for Updates…", None, App(CheckUpdates), Always),
            ],
        ),
    ]
}

/// The home screen's own, smaller set (26.202, s39 menu dump): File, Edit, Go, View and Help. Its Edit and Help are
/// the project's.
pub fn home_tree() -> Vec<(&'static str, Vec<Entry>)> {
    use crate::landing::HomeCmd as H;
    use MenuAction::{App, Home};
    use When::{Always, Home as AtHome};
    let (mut edit, mut help) = (Vec::new(), Vec::new());
    for (title, entries) in tree() {
        match title {
            // Rename works on the selected project or folder here (26.202 keeps ⌘R lit at home).
            "Edit" => {
                edit = entries
                    .into_iter()
                    .map(|e| match e {
                        Entry::Item { label: "Rename", accel, .. } => item("Rename", accel, Home(H::Rename), AtHome),
                        e => e,
                    })
                    .collect()
            }
            // ⌘F is Search here: Search Commands goes without it, as in 26.202.
            "Help" => {
                help = entries
                    .into_iter()
                    .map(|e| match e {
                        Entry::Item { label: "Search Commands", action, when, .. } => item("Search Commands", None, action, when),
                        e => e,
                    })
                    .collect()
            }
            _ => {}
        }
    }
    vec![
        (
            "File",
            vec![
                item("New Project", Some("CmdOrCtrl+N"), App(AppAction::NewProject), Always),
                item("New Folder", Some("CmdOrCtrl+Shift+N"), Home(H::NewFolder), AtHome),
                item("New Folder with Selection", Some("CmdOrCtrl+Alt+N"), Home(H::NewFolderWithSelection), AtHome),
                Entry::Separator,
                item("Import as new project...", Some("CmdOrCtrl+Shift+I"), App(AppAction::Import), Always),
                Entry::Separator,
                item("Duplicate", Some("CmdOrCtrl+D"), Home(H::Duplicate), AtHome),
            ],
        ),
        ("Edit", edit),
        (
            "Go",
            vec![
                item("Back", Some("CmdOrCtrl+["), Home(H::Back), AtHome),
                item("Forward", Some("CmdOrCtrl+]"), Home(H::Forward), AtHome),
                item("Enclosing Folder", None, Home(H::EnclosingFolder), AtHome),
                Entry::Separator,
                item("Recents", Some("CmdOrCtrl+Shift+1"), Home(H::Recents), AtHome),
                item("Projects", Some("CmdOrCtrl+Shift+2"), Home(H::Projects), AtHome),
            ],
        ),
        (
            "View",
            vec![
                Entry::Sub {
                    label: "Sort By",
                    items: vec![
                        item("Date Modified (Latest to Oldest)", None, Home(H::Sort(0)), AtHome),
                        item("Date Modified (Oldest to Latest)", None, Home(H::Sort(1)), AtHome),
                        item("Name (A to Z)", None, Home(H::Sort(2)), AtHome),
                        item("Name (Z to A)", None, Home(H::Sort(3)), AtHome),
                    ],
                },
                item("Search", Some("CmdOrCtrl+F"), Home(H::Search), AtHome),
            ],
        ),
        ("Help", help),
    ]
}

/// The menus for where the window is: a project's, or the home screen's.
pub fn menus_for(in_project: bool) -> Vec<(&'static str, Vec<Entry>)> {
    if in_project { tree() } else { home_tree() }
}

#[cfg(target_os = "macos")]
mod native {
    use std::str::FromStr;

    use muda::accelerator::Accelerator;
    use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::{AppAction, Entry, MenuAction, MenuState, When, home_tree, tree};

    /// One whole menu bar and its Window menu (which macOS only fills once the bar is the app's).
    struct Bar {
        menu: Menu,
        window: Submenu,
    }

    pub struct MenuBar {
        /// A project's menus and the home screen's smaller set, swapped as the reference does.
        project: Bar,
        home: Bar,
        in_project: Option<bool>,
        /// Each item with its action, when it's enabled, and its single key if it has one.
        items: Vec<(Native, MenuAction, When, Option<egui::Key>)>,
        state: Option<MenuState>,
        /// The interface language the bars were built in; they're built again when it changes.
        lang: &'static str,
        /// Picked items, and whether a key press picked them. muda stops filling its own channel once a handler is
        /// set, so the handler forwards here.
        picked: std::sync::mpsc::Receiver<(MenuEvent, bool)>,
    }

    /// A native item: plain, or one that shows a check mark.
    enum Native {
        Plain(MenuItem),
        Check(CheckMenuItem),
    }

    impl Native {
        fn id(&self) -> &muda::MenuId {
            match self {
                Native::Plain(m) => m.id(),
                Native::Check(m) => m.id(),
            }
        }
        fn set_enabled(&self, on: bool) {
            match self {
                Native::Plain(m) => m.set_enabled(on),
                Native::Check(m) => m.set_enabled(on),
            }
        }
        fn set_text(&self, text: String) {
            match self {
                Native::Plain(m) => m.set_text(text),
                Native::Check(m) => m.set_text(text),
            }
        }
    }

    /// Whether the menu item being run was picked by a key equivalent rather than by the pointer: AppKit runs it
    /// while handling that key-down event.
    fn picked_by_key() -> bool {
        use objc2_app_kit::{NSApplication, NSEventType};
        let Some(mtm) = objc2::MainThreadMarker::new() else { return false };
        NSApplication::sharedApplication(mtm).currentEvent().is_some_and(|e| e.r#type() == NSEventType::KeyDown)
    }

    /// macOS reads "&" as a mnemonic marker and drops it ("Wrap  Emboss"); "&&" shows one.
    fn native_label(label: &str) -> String {
        crate::i18n::tr(label).replace('&', "&&")
    }

    fn fill(sub: &Submenu, entries: Vec<Entry>, items: &mut Vec<(Native, MenuAction, When, Option<egui::Key>)>) {
        for e in entries {
            match e {
                Entry::Separator => {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                }
                Entry::Item { label, accel: shown, action, when } => {
                    let bare = shown.and_then(super::bare_key);
                    let accel = shown.and_then(|a| Accelerator::from_str(a).ok());
                    let mi = if super::check_bit(&action).is_some() {
                        let mi = CheckMenuItem::new(native_label(label), true, false, accel);
                        let _ = sub.append(&mi);
                        Native::Check(mi)
                    } else {
                        let mi = MenuItem::new(native_label(label), true, accel);
                        let _ = sub.append(&mi);
                        Native::Plain(mi)
                    };
                    items.push((mi, action, when, bare));
                }
                Entry::Sub { label, items: children } => {
                    let child = Submenu::new(native_label(label), true);
                    fill(&child, children, items);
                    let _ = sub.append(&child);
                }
            }
        }
    }

    fn build(menus: Vec<(&'static str, Vec<Entry>)>, items: &mut Vec<(Native, MenuAction, When, Option<egui::Key>)>) -> Option<Bar> {
        let menu = Menu::new();
        let tr = crate::i18n::tr;
        let app = Submenu::new("ShapeCraft", true);
        let quit = MenuItem::new(tr("Quit ShapeCraft"), true, Accelerator::from_str("CmdOrCtrl+Q").ok());
        let prefs = MenuItem::new(tr("Preferences…"), true, Accelerator::from_str("CmdOrCtrl+Comma").ok());
        let _ = app.append_items(&[
            &PredefinedMenuItem::about(Some(tr("About ShapeCraft")), None),
            &PredefinedMenuItem::separator(),
            &prefs,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some(tr("Hide ShapeCraft"))),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &quit,
        ]);
        items.push((Native::Plain(quit), MenuAction::App(AppAction::Quit), When::Always, None));
        items.push((Native::Plain(prefs), MenuAction::Ui(crate::workspace::toolbar::UiAction::Preferences), When::Always, None));
        menu.append(&app).ok()?;
        // The system's Window menu: macOS adds Fill, Center, Move & Resize, tiling and the window list.
        let window = Submenu::new(tr("Window"), true);
        let _ = window.append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ]);
        for (title, entries) in menus {
            let sub = Submenu::new(native_label(title), true);
            fill(&sub, entries, items);
            if title == "View" {
                // Last, right after the grid tip, as the reference has it (macOS retitles it Exit Full Screen).
                let _ = sub.append(&PredefinedMenuItem::fullscreen(Some(tr("Enter Full Screen"))));
            }
            menu.append(&sub).ok()?;
            if title == "View" {
                menu.append(&window).ok()?;
            }
        }
        Some(Bar { menu, window })
    }

    impl MenuBar {
        /// Builds both menu bars and installs the home screen's. Call on the main thread once the app exists.
        pub fn install(wake: impl Fn() + Send + Sync + 'static) -> Option<MenuBar> {
            let mut items = Vec::new();
            let project = build(tree(), &mut items)?;
            let home = build(home_tree(), &mut items)?;
            let lang = crate::i18n::current();
            let (send, picked) = std::sync::mpsc::channel();
            let send = std::sync::Mutex::new(send);
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let keyed = picked_by_key();
                if let Ok(send) = send.lock() {
                    let _ = send.send((e, keyed));
                }
                wake();
            }));
            let mut bar = MenuBar { project, home, in_project: None, items, state: None, lang, picked };
            bar.show_bar(false);
            Some(bar)
        }

        fn show_bar(&mut self, in_project: bool) {
            if self.in_project == Some(in_project) {
                return;
            }
            let bar = if in_project { &self.project } else { &self.home };
            bar.menu.init_for_nsapp();
            // Only once the bar is installed does macOS take it as the Window menu.
            bar.window.set_as_windows_menu_for_nsapp();
            self.in_project = Some(in_project);
        }

        /// Actions picked since the last frame. An item with a single key that the key itself picked comes back as
        /// that key, for the workspace to take as typed (a text field, E flipping Sides while extruding…).
        pub fn poll(&self) -> Vec<(MenuAction, Option<egui::Key>)> {
            let mut out = Vec::new();
            while let Ok((ev, keyed)) = self.picked.try_recv() {
                if let Some((_, action, _, bare)) = self.items.iter().find(|(mi, ..)| *mi.id() == ev.id) {
                    out.push((*action, bare.filter(|_| keyed)));
                }
            }
            out
        }

        pub fn update(&mut self, state: MenuState) {
            if self.lang != crate::i18n::current() {
                self.rebuild();
            }
            if self.state == Some(state) {
                return;
            }
            self.show_bar(state.in_project);
            for (mi, action, when, _) in &self.items {
                mi.set_enabled(when.holds(&state));
                if matches!(action, MenuAction::Ui(crate::workspace::toolbar::UiAction::LockGrid)) {
                    mi.set_text(super::grid_label(&state));
                }
                if let (Native::Check(c), Some(on)) = (mi, state.checked(action)) {
                    c.set_checked(on);
                }
            }
            self.state = Some(state);
        }

        /// Builds both bars again in the current language (Preferences ▸ Language) and installs the right one.
        fn rebuild(&mut self) {
            self.lang = crate::i18n::current();
            let mut items = Vec::new();
            let (Some(project), Some(home)) = (build(tree(), &mut items), build(home_tree(), &mut items)) else { return };
            // The old bars go only once the new one is installed.
            let _old = (std::mem::replace(&mut self.project, project), std::mem::replace(&mut self.home, home));
            self.items = items;
            self.state = None;
            let in_project = self.in_project.take().unwrap_or(false);
            self.show_bar(in_project);
        }

        /// The menus live in the system's menu bar here: nothing to draw in the window.
        pub fn show(&mut self, _ui: &mut egui::Ui, _at: egui::Pos2) {}
    }
}

#[cfg(target_os = "macos")]
pub use native::MenuBar;

/// Elsewhere the menus are drawn inside the window.
#[cfg(not(target_os = "macos"))]
pub type MenuBar = WindowMenu;

/// The menus in use: the system's menu bar (the macOS desktop app), or drawn in the window (elsewhere, and on a
/// tablet, which has no menu bar of its own).
pub enum Menus {
    #[cfg(target_os = "macos")]
    Native(MenuBar),
    Window(WindowMenu),
}

impl Menus {
    /// The platform's menus for the desktop app. Call on the main thread once the app exists.
    pub fn install(wake: impl Fn() + Send + Sync + 'static) -> Option<Menus> {
        #[cfg(target_os = "macos")]
        return MenuBar::install(wake).map(Menus::Native);
        #[cfg(not(target_os = "macos"))]
        return MenuBar::install(wake).map(Menus::Window);
    }

    pub fn update(&mut self, state: MenuState) {
        match self {
            #[cfg(target_os = "macos")]
            Menus::Native(b) => b.update(state),
            Menus::Window(w) => w.update(state),
        }
    }

    pub fn poll(&mut self) -> Vec<(MenuAction, Option<egui::Key>)> {
        match self {
            #[cfg(target_os = "macos")]
            Menus::Native(b) => b.poll(),
            Menus::Window(w) => w.poll(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, at: egui::Pos2) {
        match self {
            #[cfg(target_os = "macos")]
            Menus::Native(b) => b.show(ui, at),
            Menus::Window(w) => w.show(ui, at),
        }
    }

    /// Menus drawn in the window catch their own shortcuts (nothing else would); the system's menu bar does that itself.
    pub fn take_shortcuts(&mut self, ctx: &egui::Context) {
        match self {
            #[cfg(target_os = "macos")]
            Menus::Native(_) => {}
            Menus::Window(w) => w.take_shortcuts(ctx),
        }
    }
}

/// The menus drawn inside the window where there is no native menu bar (Linux, Windows): the same tree and actions,
/// and the menus' Ctrl shortcuts (nothing else would catch them there).
#[derive(Default)]
pub struct WindowMenu {
    state: MenuState,
    pending: Vec<MenuAction>,
}

impl WindowMenu {
    pub fn install(_wake: impl Fn() + Send + Sync + 'static) -> Option<WindowMenu> {
        Some(WindowMenu::default())
    }

    pub fn update(&mut self, state: MenuState) {
        self.state = state;
    }

    /// Actions picked since the last frame (never by a single key: those stay the workspace's here).
    pub fn poll(&mut self) -> Vec<(MenuAction, Option<egui::Key>)> {
        std::mem::take(&mut self.pending).into_iter().map(|a| (a, None)).collect()
    }

    /// The menu titles in a row from `at` (the left end of the row's center line), over whatever the screen drew
    /// there. The home screen leaves out the menus it can't use; in a project they all stay, so none of them move.
    pub fn show(&mut self, ui: &mut egui::Ui, at: egui::Pos2) {
        let menus = window_tree(self.state.in_project);
        let rect = egui::Rect::from_min_max(egui::pos2(at.x, at.y - 13.0), egui::pos2(ui.max_rect().right(), at.y + 13.0));
        let builder = egui::UiBuilder::new().id_salt("window-menu").max_rect(rect);
        let mut strip = ui.new_child(builder.layout(egui::Layout::left_to_right(egui::Align::Center)));
        strip.style_mut().override_font_id = Some(theme::body(13.0));
        strip.visuals_mut().override_text_color = Some(Tokens::text());
        egui::MenuBar::new().ui(&mut strip, |ui| {
            for (title, entries) in &menus {
                if self.state.in_project || usable(entries, &self.state) {
                    ui.menu_button(crate::i18n::tr(title), |ui| self.entries(ui, entries));
                }
            }
        });
    }

    /// This frame's menu shortcuts.
    pub fn take_shortcuts(&mut self, ctx: &egui::Context) {
        for (_, entries) in &window_tree(self.state.in_project) {
            self.shortcuts(ctx, entries);
        }
        // The Windows and Linux redo, beside the reference's Ctrl+Shift+Z.
        if When::Redo.holds(&self.state) && shortcut("CmdOrCtrl+Y").is_some_and(|sc| pressed(ctx, &sc)) {
            self.pending.push(MenuAction::Command("redo"));
        }
    }

    fn entries(&mut self, ui: &mut egui::Ui, entries: &[Entry]) {
        for e in entries {
            match e {
                Entry::Item { label, accel, action, when } => {
                    let text = accel.map(shortcut_text).unwrap_or_default();
                    // Check items lead with ✓ when on, and keep the space when off so the labels line up.
                    let label = match self.state.checked(action) {
                        Some(true) => format!("✓ {}", crate::i18n::tr(label)),
                        Some(false) => format!("    {}", crate::i18n::tr(label)),
                        None => crate::i18n::tr_s(label),
                    };
                    if ui.add_enabled(when.holds(&self.state), egui::Button::new(label).shortcut_text(text)).clicked() {
                        self.pending.push(*action);
                        ui.close();
                    }
                }
                Entry::Separator => {
                    ui.separator();
                }
                Entry::Sub { label, items } => {
                    ui.menu_button(crate::i18n::tr(label), |ui| self.entries(ui, items));
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context, entries: &[Entry]) {
        for e in entries {
            match e {
                Entry::Item { accel: Some(a), action, when, .. } if when.holds(&self.state) && bare_key(a).is_none() => {
                    if let Some(sc) = shortcut(a)
                        && pressed(ctx, &sc)
                    {
                        self.pending.push(*action);
                    }
                }
                Entry::Sub { items, .. } => self.shortcuts(ctx, items),
                _ => {}
            }
        }
    }
}

/// Whether this frame has exactly this shortcut, which it then consumes. egui's own matching lets extra Shift and Alt
/// through, so Ctrl+Z would also take Ctrl+Shift+Z.
fn pressed(ctx: &egui::Context, sc: &egui::KeyboardShortcut) -> bool {
    ctx.input_mut(|i| {
        let exact = i.events.iter().any(|e| {
            matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. }
                if *key == sc.logical_key && modifiers.matches_exact(sc.modifiers))
        });
        exact && i.consume_shortcut(sc)
    })
}

/// Whether a menu has anything that can be used in this state.
fn usable(entries: &[Entry], state: &MenuState) -> bool {
    entries.iter().any(|e| match e {
        Entry::Item { when, .. } => when.holds(state),
        Entry::Separator => false,
        Entry::Sub { items, .. } => usable(items, state),
    })
}

/// The menu tree with what the macOS application menu holds (Preferences, Quit) moved into File.
fn window_tree(in_project: bool) -> Vec<(&'static str, Vec<Entry>)> {
    let mut menus = menus_for(in_project);
    if let Some((_, file)) = menus.iter_mut().find(|(t, _)| *t == "File") {
        file.push(Entry::Separator);
        file.push(item("Preferences…", Some("CmdOrCtrl+Comma"), MenuAction::Ui(UiAction::Preferences), When::Always));
        // A tablet's copy runs until the computer stops serving it.
        if !crate::tablet::on_tablet() {
            file.push(item("Quit", Some("CmdOrCtrl+Q"), MenuAction::App(AppAction::Quit), When::Always));
        }
    }
    menus
}

/// "CmdOrCtrl+Shift+P" as egui's shortcut.
fn shortcut(accel: &str) -> Option<egui::KeyboardShortcut> {
    let mut mods = egui::Modifiers::NONE;
    let mut key = None;
    for part in accel.split('+') {
        match part {
            "CmdOrCtrl" | "Cmd" | "Ctrl" => mods |= egui::Modifiers::COMMAND,
            "Shift" => mods |= egui::Modifiers::SHIFT,
            "Alt" => mods |= egui::Modifiers::ALT,
            k => key = egui::Key::from_name(k),
        }
    }
    Some(egui::KeyboardShortcut::new(mods, key?))
}

/// A tool's single key (no modifier, "L"): the menu shows it, the workspace handles the key itself.
pub fn bare_key(accel: &str) -> Option<egui::Key> {
    (!accel.contains('+')).then(|| egui::Key::from_name(accel)).flatten()
}

/// "CmdOrCtrl+Shift+P" as shown beside the item off macOS: "Ctrl+Shift+P".
fn shortcut_text(accel: &str) -> String {
    if !crate::tablet::shows_shortcuts() {
        return String::new();
    }
    accel.replace("CmdOrCtrl", "Ctrl").replace("Comma", ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_menu_draws_and_takes_its_shortcuts() {
        let ctx = egui::Context::default();
        let mut m = WindowMenu::default();
        m.update(MenuState {
            in_project: true,
            bodies: 1,
            selection: true,
            sketching: false,
            undo: true,
            redo: false,
            ..Default::default()
        });
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
        let mut out = ctx.run_ui(input, |ui| {
            m.take_shortcuts(ui.ctx());
            m.show(ui, egui::pos2(10.0, 25.0));
        });
        out.textures_delta.clear();
        assert_eq!(m.poll().len(), 1);
    }

    /// What one press of `key` with `modifiers` does, with undo and redo both available.
    fn press(key: egui::Key, modifiers: egui::Modifiers) -> Vec<MenuAction> {
        let ctx = egui::Context::default();
        let mut m = WindowMenu::default();
        m.update(MenuState {
            in_project: true,
            bodies: 1,
            selection: true,
            sketching: false,
            undo: true,
            redo: true,
            ..Default::default()
        });
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
        let mut out = ctx.run_ui(input, |ui| m.take_shortcuts(ui.ctx()));
        out.textures_delta.clear();
        m.poll().into_iter().map(|(a, _)| a).collect()
    }

    #[test]
    fn a_shortcut_with_shift_is_not_the_one_without() {
        // Ctrl on Windows and Linux sets both flags.
        let ctrl = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
        let shift = ctrl | egui::Modifiers::SHIFT;
        assert!(matches!(press(egui::Key::Z, ctrl).as_slice(), [MenuAction::Command("undo")]));
        assert!(matches!(press(egui::Key::Z, shift).as_slice(), [MenuAction::Command("redo")]));
        assert!(matches!(press(egui::Key::Y, ctrl).as_slice(), [MenuAction::Command("redo")]));
        assert!(press(egui::Key::Y, egui::Modifiers::NONE).is_empty());
    }

    #[test]
    fn the_home_screen_only_lists_menus_it_can_use() {
        let home: Vec<_> = window_tree(false).into_iter().filter(|(_, e)| usable(e, &MenuState::default())).map(|(t, _)| t).collect();
        assert!(home.contains(&"File") && home.contains(&"Go") && !home.contains(&"Sketch"), "{home:?}");
        let project: Vec<_> = window_tree(true).into_iter().map(|(t, _)| t).collect();
        assert!(project.contains(&"Sketch") && !project.contains(&"Go"), "{project:?}");
    }

    #[test]
    fn clicking_a_title_opens_its_menu() {
        let ctx = egui::Context::default();
        let mut m = WindowMenu::default();
        let at = egui::pos2(24.0, 25.0);
        let pointer = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput { events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| m.show(ui, egui::pos2(10.0, 25.0)));
            out.textures_delta.clear();
        };
        frame(vec![]);
        frame(vec![egui::Event::PointerMoved(at)]);
        frame(vec![pointer(true)]);
        frame(vec![pointer(false)]);
        frame(vec![]);
        assert!(egui::Popup::is_any_open(&ctx));
    }

    #[test]
    fn the_window_menu_reads_every_shortcut() {
        fn walk(entries: &[Entry]) {
            for e in entries {
                match e {
                    Entry::Item { accel: Some(a), .. } => assert!(shortcut(a).is_some() || bare_key(a).is_some(), "{a}"),
                    Entry::Sub { items, .. } => walk(items),
                    _ => {}
                }
            }
        }
        for (_, entries) in window_tree(true).into_iter().chain(window_tree(false)) {
            walk(&entries);
        }
        assert_eq!(shortcut_text("CmdOrCtrl+Shift+P"), "Ctrl+Shift+P");
    }

    #[test]
    fn every_menu_shortcut_parses_and_is_unique() {
        fn walk(entries: &[Entry], seen: &mut std::collections::HashSet<&'static str>) {
            for e in entries {
                match e {
                    Entry::Item { accel: Some(a), .. } => {
                        assert!(a.contains('+') || bare_key(a).is_some(), "{a}: a bare key must be one key");
                        assert!(seen.insert(a), "duplicate shortcut {a}");
                    }
                    Entry::Sub { items, .. } => walk(items, seen),
                    _ => {}
                }
            }
        }
        // Unique within each menu bar: the home screen's ⌘F (Search) is the project's Search Commands.
        for menus in [tree(), home_tree()] {
            let mut seen = std::collections::HashSet::new();
            for (_, entries) in menus {
                walk(&entries, &mut seen);
            }
        }
    }
}
