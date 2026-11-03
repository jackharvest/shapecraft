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
}

/// When an item is enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    InProject,
    Bodies,
    TwoBodies,
    Selection,
    Sketching,
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    pub in_project: bool,
    pub bodies: usize,
    pub selection: bool,
    pub sketching: bool,
    pub undo: bool,
    pub redo: bool,
}

impl When {
    pub fn holds(self, s: &MenuState) -> bool {
        match self {
            When::Always => true,
            When::InProject => s.in_project,
            When::Bodies => s.in_project && s.bodies > 0,
            When::TwoBodies => s.in_project && s.bodies > 1,
            When::Selection => s.in_project && s.selection,
            When::Sketching => s.in_project && s.sketching,
            When::Undo => s.in_project && s.undo,
            When::Redo => s.in_project && s.redo,
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

/// The menus after the application menu, in the reference's order.
pub fn tree() -> Vec<(&'static str, Vec<Entry>)> {
    use AppAction::*;
    use MenuAction::{App, Command, Display, Tool, Ui, View};
    use When::*;
    let sep = || Entry::Separator;
    // As 26.200 has them (plan/shapr3d/04-menu-tree-workspace-26200.txt), less the cloud items (View in Browser,
    // Copy Link, Share…, Publish…) and the online help links; Save and Projects are ours (local files). Single-key
    // shortcuts aren't menu accelerators here: a bare key in the menu would take letters typed into fields.
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
                item("Import Image as Decal…", None, Ui(UiAction::ImportDecal), InProject),
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
                        item("Shapr3D…", None, App(ExportAs(Format::Shapr)), Bodies),
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
                        soon("DWG from Sketch…"),
                        item("DWG from Drawing…", None, App(ExportAs(Format::DwgDrawing)), Bodies),
                        item("SVG…", None, App(ExportAs(Format::Svg)), Bodies),
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
                item("Delete", None, Command("delete"), Selection),
                item("Rename", Some("CmdOrCtrl+R"), Ui(UiAction::Rename), Selection),
                item("Select All", Some("CmdOrCtrl+A"), Command("select_all"), Bodies),
                item("Select Through…", Some("CmdOrCtrl+Shift+S"), Ui(UiAction::SelectThrough), Bodies),
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
                item("Line", None, Tool(ToolKind::Line), InProject),
                item("Arc", None, Tool(ToolKind::Arc), InProject),
                item("Spline", None, Tool(ToolKind::Spline), InProject),
                item("Rectangle", None, Tool(ToolKind::Rectangle), InProject),
                item("Circle", None, Tool(ToolKind::Circle), InProject),
                item("Ellipse", None, Tool(ToolKind::Ellipse), InProject),
                item("Polygon", None, Tool(ToolKind::Polygon), InProject),
                sep(),
                item("Project as Sketch", None, Tool(ToolKind::Project), InProject),
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
                item("Trim", None, Tool(ToolKind::Trim), InProject),
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
                item("Move/Rotate", None, Tool(ToolKind::Move), InProject),
                item("Scale", None, Tool(ToolKind::Scale), InProject),
                item("Translate", None, Tool(ToolKind::Translate), InProject),
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
                item("Fillet", None, Tool(ToolKind::Fillet), Bodies),
                item("Chamfer", None, Tool(ToolKind::Chamfer), Bodies),
                item("Extrude", None, Tool(ToolKind::Extrude), InProject),
                item("Shell", None, Tool(ToolKind::Shell), Bodies),
                item("Loft", None, Tool(ToolKind::Loft), InProject),
                sep(),
                item("Union", Some("CmdOrCtrl+U"), Tool(ToolKind::Union), TwoBodies),
                item("Subtract", Some("CmdOrCtrl+B"), Tool(ToolKind::Subtract), TwoBodies),
                item("Intersect", Some("CmdOrCtrl+I"), Tool(ToolKind::Intersect), TwoBodies),
                sep(),
                item("Split Body", None, Tool(ToolKind::SplitBody), Bodies),
                item("Revolve", None, Tool(ToolKind::Revolve), InProject),
                item("Sweep", None, Tool(ToolKind::Sweep), InProject),
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
                item("Show History Sidebar", Some("CmdOrCtrl+Alt+P"), Ui(UiAction::ToggleHistory), InProject),
                item("Show Items Sidebar", Some("CmdOrCtrl+Alt+S"), Ui(UiAction::ToggleItems), InProject),
                item("Show Properties Sidebar", None, Ui(UiAction::DrawingProps), InProject),
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
                item("Lock Grid Size", None, Ui(UiAction::LockGrid), InProject),
            ],
        ),
        (
            "Help",
            vec![
                item("Keyboard Shortcuts…", None, Ui(UiAction::Shortcuts), InProject),
                item("Navigation Presets…", None, Ui(UiAction::NavPresets), InProject),
                item("Search Commands", Some("CmdOrCtrl+F"), Ui(UiAction::Search), InProject),
                Entry::Separator,
                item("Check for Updates…", None, MenuAction::App(AppAction::CheckUpdates), When::Always),
            ],
        ),
    ]
}

#[cfg(target_os = "macos")]
mod native {
    use std::str::FromStr;

    use muda::accelerator::Accelerator;
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::{AppAction, Entry, MenuAction, MenuState, When, tree};

    pub struct MenuBar {
        _menu: Menu,
        items: Vec<(MenuItem, MenuAction, When)>,
        state: Option<MenuState>,
        /// Picked items. muda stops filling its own channel once a handler is set, so the handler forwards here.
        picked: std::sync::mpsc::Receiver<MenuEvent>,
    }

    fn fill(sub: &Submenu, entries: Vec<Entry>, items: &mut Vec<(MenuItem, MenuAction, When)>) {
        for e in entries {
            match e {
                Entry::Separator => {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                }
                Entry::Item { label, accel, action, when } => {
                    let accel = accel.and_then(|a| Accelerator::from_str(a).ok());
                    let mi = MenuItem::new(label, true, accel);
                    let _ = sub.append(&mi);
                    items.push((mi, action, when));
                }
                Entry::Sub { label, items: children } => {
                    let child = Submenu::new(label, true);
                    fill(&child, children, items);
                    let _ = sub.append(&child);
                }
            }
        }
    }

    impl MenuBar {
        /// Builds and installs the menu bar. Call on the main thread once the app exists.
        pub fn install(wake: impl Fn() + Send + Sync + 'static) -> Option<MenuBar> {
            let menu = Menu::new();
            let mut items = Vec::new();
            let app = Submenu::new("ShapeCraft", true);
            let quit = MenuItem::new("Quit ShapeCraft", true, Accelerator::from_str("CmdOrCtrl+Q").ok());
            let prefs = MenuItem::new("Preferences…", true, Accelerator::from_str("CmdOrCtrl+Comma").ok());
            let _ = app.append_items(&[
                &PredefinedMenuItem::about(Some("About ShapeCraft"), None),
                &PredefinedMenuItem::separator(),
                &prefs,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(Some("Hide ShapeCraft")),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &quit,
            ]);
            items.push((quit, MenuAction::App(AppAction::Quit), When::Always));
            items.push((prefs, MenuAction::Ui(crate::workspace::toolbar::UiAction::Preferences), When::Always));
            menu.append(&app).ok()?;
            for (title, entries) in tree() {
                let sub = Submenu::new(title, true);
                fill(&sub, entries, &mut items);
                if title == "View" {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                    let _ = sub.append(&PredefinedMenuItem::fullscreen(None));
                }
                menu.append(&sub).ok()?;
                if title == "View" {
                    let window = Submenu::new("Window", true);
                    let _ = window.append_items(&[
                        &PredefinedMenuItem::minimize(None),
                        &PredefinedMenuItem::separator(),
                        &PredefinedMenuItem::bring_all_to_front(None),
                    ]);
                    menu.append(&window).ok()?;
                }
            }
            menu.init_for_nsapp();
            let (send, picked) = std::sync::mpsc::channel();
            let send = std::sync::Mutex::new(send);
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if let Ok(send) = send.lock() {
                    let _ = send.send(e);
                }
                wake();
            }));
            Some(MenuBar { _menu: menu, items, state: None, picked })
        }

        /// Actions picked since the last frame.
        pub fn poll(&self) -> Vec<MenuAction> {
            let mut out = Vec::new();
            while let Ok(ev) = self.picked.try_recv() {
                if let Some((_, action, _)) = self.items.iter().find(|(mi, ..)| *mi.id() == ev.id) {
                    out.push(*action);
                }
            }
            out
        }

        pub fn update(&mut self, state: MenuState) {
            if self.state == Some(state) {
                return;
            }
            for (mi, _, when) in &self.items {
                mi.set_enabled(when.holds(&state));
            }
            self.state = Some(state);
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

    pub fn poll(&mut self) -> Vec<MenuAction> {
        std::mem::take(&mut self.pending)
    }

    /// The menu titles in a row from `at` (the left end of the row's center line), over whatever the screen drew
    /// there. The home screen leaves out the menus it can't use; in a project they all stay, so none of them move.
    pub fn show(&mut self, ui: &mut egui::Ui, at: egui::Pos2) {
        let menus = window_tree();
        let rect = egui::Rect::from_min_max(egui::pos2(at.x, at.y - 13.0), egui::pos2(ui.max_rect().right(), at.y + 13.0));
        let builder = egui::UiBuilder::new().id_salt("window-menu").max_rect(rect);
        let mut strip = ui.new_child(builder.layout(egui::Layout::left_to_right(egui::Align::Center)));
        strip.style_mut().override_font_id = Some(theme::body(13.0));
        strip.visuals_mut().override_text_color = Some(Tokens::text());
        egui::MenuBar::new().ui(&mut strip, |ui| {
            for (title, entries) in &menus {
                if self.state.in_project || usable(entries, &self.state) {
                    ui.menu_button(*title, |ui| self.entries(ui, entries));
                }
            }
        });
    }

    /// This frame's menu shortcuts.
    pub fn take_shortcuts(&mut self, ctx: &egui::Context) {
        for (_, entries) in &window_tree() {
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
                    if ui.add_enabled(when.holds(&self.state), egui::Button::new(*label).shortcut_text(text)).clicked() {
                        self.pending.push(*action);
                        ui.close();
                    }
                }
                Entry::Separator => {
                    ui.separator();
                }
                Entry::Sub { label, items } => {
                    ui.menu_button(*label, |ui| self.entries(ui, items));
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context, entries: &[Entry]) {
        for e in entries {
            match e {
                Entry::Item { accel: Some(a), action, when, .. } if when.holds(&self.state) => {
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
fn window_tree() -> Vec<(&'static str, Vec<Entry>)> {
    let mut menus = tree();
    if let Some((_, file)) = menus.iter_mut().find(|(t, _)| *t == "File") {
        file.push(Entry::Separator);
        file.push(item("Preferences…", Some("CmdOrCtrl+Comma"), MenuAction::Ui(UiAction::Preferences), When::Always));
        file.push(item("Quit", Some("CmdOrCtrl+Q"), MenuAction::App(AppAction::Quit), When::Always));
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

/// "CmdOrCtrl+Shift+P" as shown beside the item off macOS: "Ctrl+Shift+P".
fn shortcut_text(accel: &str) -> String {
    accel.replace("CmdOrCtrl", "Ctrl").replace("Comma", ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_menu_draws_and_takes_its_shortcuts() {
        let ctx = egui::Context::default();
        let mut m = WindowMenu::default();
        m.update(MenuState { in_project: true, bodies: 1, selection: true, sketching: false, undo: true, redo: false });
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
        m.update(MenuState { in_project: true, bodies: 1, selection: true, sketching: false, undo: true, redo: true });
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
        let mut out = ctx.run_ui(input, |ui| m.take_shortcuts(ui.ctx()));
        out.textures_delta.clear();
        m.poll()
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
        let home: Vec<_> = window_tree().into_iter().filter(|(_, e)| usable(e, &MenuState::default())).map(|(t, _)| t).collect();
        assert!(home.contains(&"File") && !home.contains(&"Sketch"), "{home:?}");
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
                    Entry::Item { accel: Some(a), .. } => assert!(shortcut(a).is_some(), "{a}"),
                    Entry::Sub { items, .. } => walk(items),
                    _ => {}
                }
            }
        }
        for (_, entries) in window_tree() {
            walk(&entries);
        }
        assert_eq!(shortcut_text("CmdOrCtrl+Shift+P"), "Ctrl+Shift+P");
    }

    #[test]
    fn every_menu_shortcut_parses_and_is_unique() {
        let mut seen = std::collections::HashSet::new();
        fn walk(entries: &[Entry], seen: &mut std::collections::HashSet<&'static str>) {
            for e in entries {
                match e {
                    Entry::Item { accel: Some(a), .. } => {
                        assert!(a.contains('+'), "{a}: bare-key shortcuts belong to the workspace, not the menu");
                        assert!(seen.insert(a), "duplicate shortcut {a}");
                    }
                    Entry::Sub { items, .. } => walk(items, seen),
                    _ => {}
                }
            }
        }
        for (_, entries) in tree() {
            walk(&entries, &mut seen);
        }
    }
}
