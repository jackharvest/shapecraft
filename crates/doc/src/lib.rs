//! The project document. Cheap to clone (shapes and meshes are shared), so undo is a list of snapshots.

#![forbid(unsafe_code)]

use std::sync::Arc;

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use shapecraft_kernel::{Mesh, Shape};

pub mod constraints;
pub mod date;
pub mod expr;
mod sketch;
pub mod sketch_ops;
pub mod text;
pub use constraints::{Along, Constraint, Element};
pub use expr::{VarKind, Variable};
pub use sketch::{
    RectFrame, Region, Sketch, SketchEntity, SketchPattern, SketchPlane, arc_points, bspline_points, circle_through, rectangle_frame,
    rectangle_loop, region_nesting, spline_points,
};

mod facets;
pub mod project;
pub use facets::Facets;
mod visual;
pub use visual::{
    APERTURE, BLUR, BUILTIN_ENVIRONMENTS, DECAL_SCALE, Decal, DecalImage, Ground, HdrPixels, LIGHT_INTENSITY, LIGHT_ROTATION,
    MAX_PANORAMAS, PANORAMA_MAX, PANORAMA_WORK, Panorama, Projection, SUN_ELEVATION, VisualSettings,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemId(pub u64);

#[derive(Clone)]
pub struct Body {
    pub id: ItemId,
    pub name: String,
    pub shape: Shape,
    pub mesh: Arc<Mesh>,
    pub visible: bool,
    /// How it looks in Visualization (and tints it in Modeling); `None` is the default grey.
    pub appearance: Option<Appearance>,
    /// Faces painted their own color (face index, sRGB 0..1), as the reference allows in Visualization.
    pub face_colors: Vec<(u32, [f32; 3])>,
    /// A mesh body's triangles (an STL, OBJ or .shapr mesh body): `shape` is then empty and `mesh` comes from these.
    pub facets: Option<Arc<Facets>>,
}

impl Body {
    /// A mesh body (triangles, no exact faces): picked whole, no faces or edges to work on.
    pub fn is_mesh(&self) -> bool {
        self.facets.is_some()
    }
}

/// A body's look: a color and how metallic and glossy it is (all 0..1; color in sRGB).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    pub name: String,
    /// "Glossy", "Brushed", … (shown small above the name).
    #[serde(default)]
    pub finish: String,
    pub color: [f32; 3],
    pub metal: f32,
    pub gloss: f32,
    /// A custom material's picture (a `DecalImage` with `material` set), tiled over the body by box projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<ItemId>,
    /// How big one tile of the picture is, as a fraction of the body's largest extent (custom materials).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub scale: f32,
}

impl Default for Appearance {
    /// The grey every new body has ("Standard / Default Material").
    fn default() -> Self {
        Self {
            name: "Default Material".into(),
            finish: "Standard".into(),
            color: [0.627, 0.627, 0.643],
            metal: 0.0,
            gloss: 0.35,
            texture: None,
            scale: 1.0,
        }
    }
}

fn one() -> f32 {
    1.0
}

fn is_one(v: &f32) -> bool {
    *v == 1.0
}

impl Appearance {
    /// Clamped to valid ranges (hostile input from files and commands).
    pub fn sanitized(mut self) -> Option<Self> {
        if !(self.color.iter().all(|c| c.is_finite()) && self.metal.is_finite() && self.gloss.is_finite()) {
            return None;
        }
        self.color = self.color.map(|c| c.clamp(0.0, 1.0));
        self.metal = self.metal.clamp(0.0, 1.0);
        self.gloss = self.gloss.clamp(0.0, 1.0);
        self.scale = if self.scale.is_finite() { self.scale.clamp(0.05, 20.0) } else { 1.0 };
        self.name.truncate(100);
        self.finish.truncate(100);
        Some(self)
    }

    /// The built-in library shown in Visualization, by category (our own picks and colors).
    pub fn library_groups() -> Vec<(&'static str, Vec<Appearance>)> {
        let a = |finish: &str, name: &str, c: u32, metal: f32, gloss: f32| Appearance {
            name: name.into(),
            finish: finish.into(),
            color: [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0],
            metal,
            gloss,
            texture: None,
            scale: 1.0,
        };
        vec![
            (
                "Plastic",
                vec![
                    a("Glossy", "ABS", 0xc8302a, 0.0, 0.85),
                    a("Matte", "ABS", 0xc8302a, 0.0, 0.3),
                    a("Standard", "Acrylic", 0x3c3c40, 0.0, 0.9),
                    a("Glossy", "Carbon Fiber", 0x1c1c1e, 0.0, 0.8),
                    a("Matte", "Carbon Fiber", 0x202022, 0.0, 0.3),
                    a("Glossy", "Melamine", 0x8e1c2a, 0.0, 0.8),
                    a("Matte", "PVC", 0xd9a020, 0.0, 0.25),
                    a("Glossy", "White Plastic", 0xf2f2ee, 0.0, 0.7),
                    a("Matte", "Polypropylene", 0xe8662a, 0.0, 0.3),
                    a("Standard", "Rubber", 0x222222, 0.0, 0.08),
                    a("Matte", "Silicone Rubber", 0x3ec7cf, 0.0, 0.2),
                    a("Matte", "Black Plastic", 0x262628, 0.0, 0.35),
                ],
            ),
            (
                "Metal",
                vec![
                    a("Brushed", "Aluminum", 0xc9cdd2, 1.0, 0.5),
                    a("Polished", "Aluminum", 0xd8dadd, 1.0, 0.92),
                    a("Rough", "Aluminum", 0xb8bcc0, 1.0, 0.3),
                    a("Brushed", "Anodized Aluminum", 0x5a5e66, 1.0, 0.5),
                    a("Polished", "Steel", 0xb5b9be, 1.0, 0.9),
                    a("Brushed", "Steel", 0x9ea3a8, 1.0, 0.45),
                    a("Polished", "Titanium", 0xa9a49c, 1.0, 0.9),
                    a("Polished", "Chrome", 0xe6e8ea, 1.0, 0.97),
                    a("Polished", "Brass", 0xc9a548, 1.0, 0.85),
                    a("Polished", "Copper", 0xc47a4b, 1.0, 0.85),
                    a("Polished", "Gold", 0xe0b84f, 1.0, 0.9),
                    a("Brushed", "White Gold", 0xd8d4c8, 1.0, 0.55),
                ],
            ),
            (
                "Wood",
                vec![
                    a("Clearcoat", "Walnut", 0x5b3a26, 0.0, 0.7),
                    a("Sanded", "Walnut", 0x5e4030, 0.0, 0.2),
                    a("Clearcoat", "Ash", 0xd7b98c, 0.0, 0.7),
                    a("Sanded", "Ash", 0xd9bf94, 0.0, 0.2),
                    a("Clearcoat", "Oak", 0xb98a52, 0.0, 0.7),
                    a("Sanded", "Oak", 0xbb8f5a, 0.0, 0.2),
                ],
            ),
            (
                "Stone & Ceramic",
                vec![
                    a("Rough", "Concrete", 0x9a9a96, 0.0, 0.1),
                    a("Glossy", "Granite", 0x6b6b6b, 0.0, 0.8),
                    a("Glossy", "Marble - White", 0xe8e8e6, 0.0, 0.85),
                    a("Matte", "Marble - Black", 0x1c1c1e, 0.0, 0.3),
                    a("Glazed", "Porcelain", 0xefece2, 0.0, 0.9),
                    a("Unglazed", "Terracotta", 0xb35a3a, 0.0, 0.15),
                ],
            ),
            (
                "Leather",
                vec![a("Standard", "Embossed Leather", 0x3a2a20, 0.0, 0.3), a("Standard", "Flat Grain Leather", 0x2a2a2a, 0.0, 0.3)],
            ),
        ]
    }

    /// Every look in the library, flattened.
    pub fn library() -> Vec<Appearance> {
        Self::library_groups().into_iter().flat_map(|(_, v)| v).collect()
    }

    /// "Polished Aluminum", or just the name when there's no finish.
    pub fn full_name(&self) -> String {
        if self.finish.is_empty() { self.name.clone() } else { format!("{} {}", self.finish, self.name) }
    }
}

/// A construction plane: drawn as a square, sketched on, mirrored or cut across.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstructionPlane {
    pub id: ItemId,
    pub name: String,
    pub plane: SketchPlane,
    /// Side of the drawn square, in mm.
    pub size: f64,
    pub visible: bool,
}

impl ConstructionPlane {
    /// The square's corners in world space.
    pub fn corners(&self) -> [DVec3; 4] {
        let h = self.size / 2.0;
        let p = &self.plane;
        [p.to_world(v2(-h, -h)), p.to_world(v2(h, -h)), p.to_world(v2(h, h)), p.to_world(v2(-h, h))]
    }
}

/// Decoded picture pixels: width, height and RGBA rows (top row first).
pub type Pixels = (u32, u32, Vec<u8>);

/// A picture placed on a plane as a reference (Insert ▸ Image).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefImage {
    pub id: ItemId,
    pub name: String,
    /// Centered on the plane's origin, its x and y along the plane's.
    pub plane: SketchPlane,
    /// Width and height on the plane, in mm.
    pub size: DVec2,
    pub opacity: f32,
    pub visible: bool,
    /// "png" or "jpg": the file kept with the project.
    pub ext: String,
    /// The file as imported; not in project.json (it's saved beside it).
    #[serde(skip)]
    pub file: Arc<Vec<u8>>,
    /// Decoded pixels (width, height, RGBA), for drawing.
    #[serde(skip)]
    pub pixels: Arc<Pixels>,
}

impl PartialEq for RefImage {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
            && self.name == o.name
            && self.plane == o.plane
            && self.size == o.size
            && self.opacity == o.opacity
            && self.visible == o.visible
            && Arc::ptr_eq(&self.file, &o.file)
    }
}

impl RefImage {
    /// A picture file as projects keep it: PNG and JPEG as they are; TIFF, BMP, GIF (its first frame) and ICO
    /// turned into PNG.
    pub fn normalize(file: Vec<u8>) -> Result<Vec<u8>, String> {
        let format =
            image::guess_format(&file).map_err(|_| "not a picture this app reads (PNG, JPEG, TIFF, BMP, GIF or ICO)".to_string())?;
        match format {
            image::ImageFormat::Png | image::ImageFormat::Jpeg => Ok(file),
            image::ImageFormat::Tiff | image::ImageFormat::Bmp | image::ImageFormat::Gif | image::ImageFormat::Ico => {
                let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&file), format);
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(16384);
                limits.max_image_height = Some(16384);
                limits.max_alloc = Some(1 << 30);
                reader.limits(limits);
                let img = reader.decode().map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                img.to_rgba8().write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
                Ok(out)
            }
            _ => Err("only PNG, JPEG, TIFF, BMP, GIF and ICO pictures can be inserted".into()),
        }
    }

    /// Decodes a PNG or JPEG file. Very large pictures are scaled down for drawing.
    pub fn decode(file: &[u8]) -> Result<(String, Pixels), String> {
        let format = image::guess_format(file).map_err(|e| e.to_string())?;
        let ext = match format {
            image::ImageFormat::Png => "png",
            image::ImageFormat::Jpeg => "jpg",
            _ => return Err("only PNG and JPEG pictures can be inserted".into()),
        };
        let img = image::load_from_memory_with_format(file, format).map_err(|e| e.to_string())?;
        let img = if img.width().max(img.height()) > 4096 { img.resize(4096, 4096, image::imageops::FilterType::Triangle) } else { img };
        let rgba = img.to_rgba8();
        Ok((ext.into(), (rgba.width(), rgba.height(), rgba.into_raw())))
    }

    /// The corners on the plane, counter-clockwise from bottom left.
    pub fn corners(&self) -> [DVec3; 4] {
        let h = self.size / 2.0;
        [DVec2::new(-h.x, -h.y), DVec2::new(h.x, -h.y), DVec2::new(h.x, h.y), DVec2::new(-h.x, h.y)].map(|p| self.plane.to_world(p))
    }
}

/// An Items sidebar folder: a named group of items (as observed: "Folder 01", items dragged onto it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: ItemId,
    pub name: String,
    pub items: Vec<ItemId>,
}

/// A construction axis: drawn as a line, used to revolve, rotate or pattern around.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstructionAxis {
    pub id: ItemId,
    pub name: String,
    pub origin: DVec3,
    /// Unit direction.
    pub dir: DVec3,
    /// Drawn this long, centered on `origin`, in mm.
    pub length: f64,
    pub visible: bool,
}

impl ConstructionAxis {
    pub fn ends(&self) -> (DVec3, DVec3) {
        let h = self.dir * (self.length / 2.0);
        (self.origin - h, self.origin + h)
    }
}

/// A drawing sheet of some bodies (the views are made from them each time it's shown or exported).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingSheet {
    pub id: ItemId,
    pub name: String,
    /// The reference bodies; empty means every visible body.
    pub bodies: Vec<ItemId>,
    /// "A4", "A3", "A2", "A1", "A0", "Letter", "Tabloid".
    pub sheet: String,
    pub landscape: bool,
    /// Sheet mm per model mm; `None` picks the largest standard scale that fits.
    pub scale: Option<f64>,
    /// First-angle projection (the reference's default) or third-angle.
    pub first_angle: bool,
    /// Front, side, top and isometric views; otherwise just the front.
    pub four_views: bool,
    #[serde(default)]
    pub style: DrawingStyle,
    /// Dimensions, centerlines and notes placed on the sheet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    /// The views placed on the sheet, parents before the views made from them; `None` (drawings saved before
    /// views could be placed) lays out the front, or the four views, by itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub views: Option<Vec<SheetView>>,
}

/// A view placed on a drawing sheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetView {
    /// Unique on its sheet; annotations and child views refer to it by name ("Front", "Section A-A", "Detail B").
    pub name: String,
    pub kind: SheetViewKind,
    /// Where the view's middle sits, in sheet millimeters from the bottom-left corner.
    pub at: DVec2,
    /// Sheet mm per model mm; `None` follows the drawing (a detail view twice its parent's). Projected views always
    /// take their parent's.
    #[serde(default)]
    pub scale: Option<f64>,
    /// Lines behind the model drawn dashed.
    #[serde(default = "yes")]
    pub hidden_lines: bool,
    /// The bodies a base view shows (its Bodies… pill); empty: the drawing's. Views made from it show the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<ItemId>,
}

fn yes() -> bool {
    true
}

/// How a sheet view looks at the model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SheetViewKind {
    /// Looking along `-toward` with `x` to the right.
    Base { toward: DVec3, x: DVec3 },
    /// One step from its parent: `dx`, `dy` in -1..=1 (screen right, up); orthographic beside it, isometric on the
    /// diagonals, following the drawing's projection angle.
    Projected { parent: String, dx: i8, dy: i8 },
    /// The model cut along the line `a`→`b` of the parent (its model mm), seen looking to the left of a→b (to the
    /// right when `flip`); `letter` names the cut.
    Section { parent: String, a: DVec2, b: DVec2, flip: bool, letter: String },
    /// The parent's lines inside a circle (its model mm), magnified.
    Detail { parent: String, center: DVec2, radius: f64, letter: String },
}

impl SheetViewKind {
    pub fn parent(&self) -> Option<&str> {
        match self {
            Self::Base { .. } => None,
            Self::Projected { parent, .. } | Self::Section { parent, .. } | Self::Detail { parent, .. } => Some(parent),
        }
    }
}

/// Something drawn on a sheet besides the views. Points on a view are in that view's own model millimeters (so
/// they stay put when the scale or sheet changes); offsets and notes are in sheet millimeters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Annotation {
    /// The straight distance between two points, its dimension line `offset` mm to the left of a→b.
    Distance {
        view: String,
        a: DVec2,
        b: DVec2,
        offset: f64,
        #[serde(default, skip_serializing_if = "DimText::is_empty")]
        text: DimText,
    },
    /// A circle's radius or diameter, with the leader leaving at `angle` (radians).
    Radius {
        view: String,
        center: DVec2,
        radius: f64,
        angle: f64,
        diameter: bool,
        #[serde(default, skip_serializing_if = "DimText::is_empty")]
        text: DimText,
    },
    /// The angle at `vertex` from the direction of `a` to the direction of `b` (the smaller way round), drawn as
    /// an arc `radius` sheet mm out.
    Angle {
        view: String,
        vertex: DVec2,
        a: DVec2,
        b: DVec2,
        radius: f64,
        #[serde(default, skip_serializing_if = "DimText::is_empty")]
        text: DimText,
    },
    /// A dash-dot line between two points, run a little past them.
    Centerline { view: String, a: DVec2, b: DVec2 },
    /// A dash-dot circle (a bolt circle).
    CircularCenterline { view: String, center: DVec2, radius: f64 },
    /// A small cross where two edges' lines meet.
    IntersectionMark { view: String, at: DVec2 },
    /// A cross at a circle's center, reaching just past it.
    CenterMark { view: String, center: DVec2, radius: f64 },
    /// Text on the sheet.
    Note { at: DVec2, text: String },
    /// A picture on the sheet (a logo, a photo of the part): centred at `at`, `width` sheet mm wide, the picture a
    /// sheet picture of the project's library (`DecalImage::sheet`).
    Image { at: DVec2, width: f64, image: ItemId },
}

/// A dimension's extra text (observed s14-127): a prefix and suffix around the value, and a tolerance.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DimText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<Tolerance>,
}

impl DimText {
    pub fn is_empty(&self) -> bool {
        self.prefix.is_none() && self.suffix.is_none() && self.tolerance.is_none()
    }
}

/// A dimension's tolerance, in display units: symmetrical (±upper), a deviation (+upper / −lower), limits (the
/// value plus upper over the value minus lower), or basic (the value boxed).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tolerance {
    pub kind: ToleranceKind,
    #[serde(default)]
    pub upper: f64,
    #[serde(default)]
    pub lower: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToleranceKind {
    Symmetrical,
    Deviation,
    Limits,
    Basic,
}

/// The title-block layouts (observed s14-120).
pub const TITLE_BLOCKS: &[&str] = &["Empty Sheet", "Border Only", "Simple", "Horizontal", "Vertical", "Block", "Block with Table"];

/// The date formats a title block can use (observed, the start of the reference's list).
pub const DATE_FORMATS: &[&str] = &[
    "MM-DD-YY",
    "DD-MM-YY",
    "YYYY-MM-DD",
    "DD-MMM-YY",
    "YYYYMMDD",
    "YYMMDD",
    "YY-MM-DD",
    "MM-DD-YYYY",
    "MM/DD/YYYY",
    "MM.DD.YYYY",
    "DD-MM-YYYY",
    "DD/MM/YYYY",
    "DD.MM.YYYY",
    "YYYY/MM/DD",
    "YYYY.MM.DD",
    "MM/DD/YY",
    "MM.DD.YY",
    "DD/MM/YY",
    "DD.MM.YY",
    "YY/MM/DD",
    "YY.MM.DD",
];

/// How a drawing writes numbers and how heavy its lines are (the Properties sidebar's Dimensions and Line Widths).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawingStyle {
    /// "Decimal" or "Degrees, Minutes, Seconds".
    pub angle_format: String,
    /// Digits after the separator.
    pub length_precision: u8,
    pub angle_precision: u8,
    /// "Period" or "Comma".
    pub decimal_separator: String,
    /// Line widths in mm.
    pub visible: f64,
    pub hidden: f64,
    pub dimension: f64,
    pub centerline: f64,
    pub section: f64,
    pub detail: f64,
    /// One of [`TITLE_BLOCKS`].
    pub title_block: String,
    /// One of [`DATE_FORMATS`].
    pub date_format: String,
}

impl Default for DrawingStyle {
    fn default() -> Self {
        Self {
            angle_format: "Decimal".into(),
            length_precision: 2,
            angle_precision: 2,
            decimal_separator: "Period".into(),
            visible: 0.40,
            hidden: 0.15,
            dimension: 0.15,
            centerline: 0.15,
            section: 0.15,
            detail: 0.40,
            title_block: "Simple".into(),
            date_format: "MM/DD/YY".into(),
        }
    }
}

impl DrawingSheet {
    /// Paper size in millimeters for a sheet name, landscape when asked.
    pub fn paper(name: &str, landscape: bool) -> Option<(f64, f64)> {
        let (w, h) = match name {
            "A0" => (841.0, 1189.0),
            "A1" => (594.0, 841.0),
            "A2" => (420.0, 594.0),
            "A3" => (297.0, 420.0),
            "A4" => (210.0, 297.0),
            "Letter" => (215.9, 279.4),
            "Tabloid" => (279.4, 431.8),
            _ => return None,
        };
        Some(if landscape { (h, w) } else { (w, h) })
    }
}

/// A step of the parametric history. It keeps the commands it ran, so its parameters can be changed and the
/// model rebuilt by replaying every step from an empty document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryStep {
    pub label: String,
    /// The command that made the step ("extrude"), or "sketch" for a run of edits to one sketch.
    #[serde(default)]
    pub kind: String,
    /// The sketch a sketch step edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<ItemId>,
    /// Bookkeeping (renames, visibility, appearance, drawings): replayed, but not listed in the sidebar.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quiet: bool,
    /// Empty for steps saved before the history was parametric; those can't be replayed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<LogEntry>,
    /// Suppressed steps are skipped when the model is rebuilt.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
    /// Why the step failed the last time the model was rebuilt (it was skipped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The document's next id once the step ran, so a skipped step leaves the ids of the steps after it alone.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ids: u64,
    /// An imported Shapr3D project's own History, [name, operation] in order: shown under its Import step. The
    /// model they made comes in whole with the import; the steps themselves aren't rebuilt here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shapr_steps: Vec<[String; 2]>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// One command as it ran: its parameters and the selection and active sketch it saw.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub command: String,
    pub params: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selection: Vec<Pick>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_sketch: Option<ItemId>,
    /// What each face and edge the command pointed at looked like when it ran, so a replay can find it again after
    /// edits upstream renumber the body's faces and edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<TopoRef>,
}

/// A face or edge a history step points at, by number and by what it looks like.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TopoRef {
    pub body: ItemId,
    pub edge: bool,
    pub index: u32,
    /// "plane", "round" (has an axis), "curved"; "line", "circle", "arc", "curve".
    pub kind: String,
    pub center: [f64; 3],
    /// The center within the body's bounding box (0–1 each way): stays put when the body is stretched.
    pub rel: [f64; 3],
    /// A flat face's outward normal, a round face's or circle's axis, a line's direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<[f64; 3]>,
    /// Area or length.
    pub size: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Units {
    #[default]
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Foot,
}

impl Units {
    pub fn suffix(self) -> &'static str {
        match self {
            Units::Millimeter => "mm",
            Units::Centimeter => "cm",
            Units::Meter => "m",
            Units::Inch => "in",
            Units::Foot => "ft",
        }
    }

    /// Millimeters per unit (the model is always stored in millimeters).
    pub fn scale(self) -> f64 {
        match self {
            Units::Millimeter => 1.0,
            Units::Centimeter => 10.0,
            Units::Meter => 1000.0,
            Units::Inch => 25.4,
            Units::Foot => 304.8,
        }
    }
}

/// Something that can be selected in the viewport or the Items sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pick {
    Body {
        body: ItemId,
    },
    Face {
        body: ItemId,
        face: u32,
    },
    Edge {
        body: ItemId,
        edge: u32,
    },
    Sketch {
        sketch: ItemId,
    },
    Region {
        sketch: ItemId,
        region: u32,
    },
    SketchEntity {
        sketch: ItemId,
        entity: u32,
    },
    /// A sketch element's endpoint, corner or center (`index` as in `constraints::points`).
    SketchPoint {
        sketch: ItemId,
        entity: u32,
        index: u32,
    },
    Plane {
        plane: ItemId,
    },
    Axis {
        axis: ItemId,
    },
    Image {
        image: ItemId,
    },
}

impl Pick {
    pub fn body(&self) -> Option<ItemId> {
        match *self {
            Pick::Body { body } | Pick::Face { body, .. } | Pick::Edge { body, .. } => Some(body),
            _ => None,
        }
    }

    /// The Items-sidebar item this pick belongs to (body, sketch or plane).
    pub fn item(&self) -> Option<ItemId> {
        match *self {
            Pick::Plane { plane } => Some(plane),
            Pick::Axis { axis } => Some(axis),
            Pick::Image { image } => Some(image),
            _ => self.body().or(self.sketch()),
        }
    }

    pub fn sketch(&self) -> Option<ItemId> {
        match *self {
            Pick::Sketch { sketch }
            | Pick::Region { sketch, .. }
            | Pick::SketchEntity { sketch, .. }
            | Pick::SketchPoint { sketch, .. } => Some(sketch),
            _ => None,
        }
    }
}

/// A camera view saved with the project (View ▸ Saved Views 1–8).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    pub target: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub distance: f64,
    #[serde(default)]
    pub ortho: f64,
    #[serde(default)]
    pub roll: f64,
}

/// What a project opened from a Shapr3D file keeps of it: the file itself (saved beside the project as
/// `source.shapr`) and, for each body that came from it, the Shapr3D step that made it and what it looked like
/// then, so an export can tell changed bodies from untouched ones.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShaprLink {
    #[serde(skip)]
    pub file: Arc<Vec<u8>>,
    pub bodies: Vec<ShaprBody>,
    /// Sketches that came from the file: (ours, Shapr3D's sketch id, its circles then as (center, radius)).
    #[serde(default)]
    pub sketches: Vec<ShaprSketch>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaprSketch {
    pub sketch: ItemId,
    pub id: i64,
    pub circles: Vec<([f64; 2], f64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaprBody {
    pub body: ItemId,
    /// The history step (tree node id) that created the body in Shapr3D.
    pub step: i64,
    /// The anchored item name that distinguishes bodies made by the same step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_name_id: Option<i64>,
    /// Its name in Shapr3D's Items list when it came over.
    pub name: String,
    /// Volume, area, face count, centroid and bounding box (min, max) when it came over.
    pub fingerprint: Vec<f64>,
    /// The fingerprint's volume, area and centroid came from the body's triangles (measuring it exactly took too
    /// long, #26); it is compared the same way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mesh_fingerprint: bool,
    /// Its colour then (`None` for Shapr3D's default grey).
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// Whether it was hidden then.
    #[serde(default)]
    pub hidden: bool,
    /// When our own step remade it from a Shapr3D Extrude: that step and the Extrude it stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extrude: Option<ShaprExtrude>,
}

/// An extrude step of ours standing for a Shapr3D Extrude: editing its distance or draft here edits Shapr3D's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaprExtrude {
    /// The Extrude's tree node in the Shapr3D project.
    pub node: i64,
    /// Our step's label ("Extrude 01").
    pub step: String,
    /// The Shapr3D Fillet, Chamfer or Offset Face on the extrude, and our step standing for it ("Fillet 01",
    /// "Chamfer 01", "Offset Face 01").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fillet: Option<(i64, String)>,
    /// For a Shapr3D Offset Face measured as a total thickness: what to add to our offset to get that total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
}

impl ShaprBody {
    /// How far a body with `now` as its fingerprint moved, when it only moved (same shape, no turn).
    pub fn moved_by(&self, now: &[f64]) -> Option<[f64; 3]> {
        let (a, b) = (&self.fingerprint, now);
        if a.len() != 12 || b.len() != 12 {
            return None;
        }
        let same = |x: f64, y: f64| (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0);
        if !(0..3).all(|i| same(a[i], b[i])) {
            return None;
        }
        let t = [b[3] - a[3], b[4] - a[4], b[5] - a[5]];
        let scale = a[6..].iter().chain(&b[6..]).fold(1.0f64, |m, v| m.max(v.abs()));
        let close = |x: f64, y: f64| (x - y).abs() <= 1e-6 * scale;
        // A turn moves the bounding box differently from the centroid.
        ((0..3).all(|k| close(b[6 + k] - a[6 + k], t[k]) && close(b[9 + k] - a[9 + k], t[k]))).then_some(t)
    }

    /// Whether a body with `now` as its fingerprint is still the shape that came over (and in the same place).
    pub fn unchanged(&self, now: &[f64]) -> bool {
        self.fingerprint.len() == now.len()
            && self.fingerprint.iter().zip(now).all(|(a, b)| (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0))
    }
}

#[derive(Clone, Default)]
pub struct Document {
    pub name: String,
    pub units: Units,
    pub bodies: Vec<Body>,
    pub sketches: Vec<Sketch>,
    pub planes: Vec<ConstructionPlane>,
    pub axes: Vec<ConstructionAxis>,
    pub drawings: Vec<DrawingSheet>,
    /// Named values that typed fields and history steps can use.
    pub variables: Vec<Variable>,
    pub images: Vec<RefImage>,
    /// Visualization: the decal library, the decals on bodies, and the environment and camera settings.
    pub decal_images: Vec<DecalImage>,
    pub decals: Vec<Decal>,
    pub visual: VisualSettings,
    /// Items sidebar folders; a folder's items can be folders too.
    pub folders: Vec<Folder>,
    /// Items dragged to a place of their own in the Items list: their sort key (see [`Document::item_key`]).
    pub item_ranks: Vec<(ItemId, f64)>,
    /// Saved Views 1–8.
    pub saved_views: [Option<SavedView>; 8],
    pub history: Vec<HistoryStep>,
    /// While a sketch is edited in the past: the steps after it, replayed when the sketch is left.
    pub future: Vec<HistoryStep>,
    /// The Shapr3D project this one came from, so exporting back adds to that project's own history.
    pub shapr: Option<ShaprLink>,
    /// Sketches on faces use the frame older projects had (see [`SketchPlane::on_face`]).
    pub legacy_face_axes: bool,
    next_id: u64,
    body_counter: u32,
    sketch_counter: u32,
    plane_counter: u32,
    axis_counter: u32,
}

impl Document {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), next_id: 1, ..Default::default() }
    }

    pub fn alloc_id(&mut self) -> ItemId {
        let id = ItemId(self.next_id.max(1));
        self.next_id = id.0 + 1;
        id
    }

    /// "Body 01", "Body 02" … like the reference app.
    pub fn next_body_name(&mut self) -> String {
        // Skip names bodies brought in from files already have.
        loop {
            self.body_counter += 1;
            let name = format!("Body {:02}", self.body_counter);
            if !self.bodies.iter().any(|b| b.name == name) || self.body_counter > 100_000 {
                return name;
            }
        }
    }

    /// A copy's name, as observed (379): "Body 01" → "Body 01 (1)", then "(2)"… whichever is free.
    pub fn copy_name(&self, of: &str) -> String {
        let base = match of.rsplit_once(" (") {
            Some((b, n)) if n.strip_suffix(')').is_some_and(|d| d.parse::<u32>().is_ok()) => b,
            _ => of,
        };
        (1..).map(|i| format!("{base} ({i})")).find(|n| !self.bodies.iter().any(|b| b.name == *n)).unwrap_or_else(|| format!("{base} (1)"))
    }

    pub fn next_sketch_name(&mut self) -> String {
        self.sketch_counter += 1;
        format!("Sketch {:02}", self.sketch_counter)
    }

    pub fn next_plane_name(&mut self) -> String {
        self.plane_counter += 1;
        format!("Plane {:02}", self.plane_counter)
    }

    pub fn next_axis_name(&mut self) -> String {
        self.axis_counter += 1;
        format!("Axis {:02}", self.axis_counter)
    }

    pub fn drawing(&self, id: ItemId) -> Option<&DrawingSheet> {
        self.drawings.iter().find(|d| d.id == id)
    }

    pub fn drawing_mut(&mut self, id: ItemId) -> Option<&mut DrawingSheet> {
        self.drawings.iter_mut().find(|d| d.id == id)
    }

    pub fn axis(&self, id: ItemId) -> Option<&ConstructionAxis> {
        self.axes.iter().find(|a| a.id == id)
    }

    pub fn axis_mut(&mut self, id: ItemId) -> Option<&mut ConstructionAxis> {
        self.axes.iter_mut().find(|a| a.id == id)
    }

    pub fn folder(&self, id: ItemId) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    pub fn folder_mut(&mut self, id: ItemId) -> Option<&mut Folder> {
        self.folders.iter_mut().find(|f| f.id == id)
    }

    /// The folder an item is in.
    pub fn folder_of(&self, item: ItemId) -> Option<ItemId> {
        self.folders.iter().find(|f| f.items.contains(&item)).map(|f| f.id)
    }

    /// Everything in a folder and the folders inside it, folders left out.
    pub fn folder_items_deep(&self, folder: ItemId) -> Vec<ItemId> {
        let mut out = Vec::new();
        let mut todo = vec![folder];
        let mut seen = std::collections::HashSet::new();
        while let Some(f) = todo.pop() {
            if !seen.insert(f) {
                continue;
            }
            for i in self.folder(f).map(|f| f.items.as_slice()).unwrap_or_default() {
                if self.folder(*i).is_some() { todo.push(*i) } else { out.push(*i) }
            }
        }
        out
    }

    /// Whether `item` is `folder` or somewhere inside it.
    pub fn is_within(&self, item: ItemId, folder: ItemId) -> bool {
        let mut at = Some(item);
        for _ in 0..10_000 {
            match at {
                Some(i) if i == folder => return true,
                Some(i) => at = self.folder_of(i),
                None => return false,
            }
        }
        false
    }

    /// Where an item sorts in the Items list (smaller first): its dragged-to place, else newest first.
    pub fn item_key(&self, id: ItemId) -> f64 {
        self.item_ranks.iter().find(|r| r.0 == id).map_or(-(id.0 as f64), |r| r.1)
    }

    pub fn set_item_key(&mut self, id: ItemId, key: f64) {
        self.item_ranks.retain(|r| r.0 != id);
        self.item_ranks.push((id, key));
    }

    /// "Folder 01", "Folder 02" … the first number not taken.
    pub fn next_folder_name(&self) -> String {
        (1..).map(|n| format!("Folder {n:02}")).find(|n| !self.folders.iter().any(|f| f.name == *n)).unwrap_or_else(|| "Folder".into())
    }

    /// Whether an item (body, sketch, plane, axis or picture) exists.
    pub fn has_item(&self, id: ItemId) -> bool {
        self.body(id).is_some()
            || self.sketch(id).is_some()
            || self.plane(id).is_some()
            || self.axes.iter().any(|a| a.id == id)
            || self.image(id).is_some()
    }

    pub fn image(&self, id: ItemId) -> Option<&RefImage> {
        self.images.iter().find(|i| i.id == id)
    }

    pub fn image_mut(&mut self, id: ItemId) -> Option<&mut RefImage> {
        self.images.iter_mut().find(|i| i.id == id)
    }

    pub fn plane(&self, id: ItemId) -> Option<&ConstructionPlane> {
        self.planes.iter().find(|p| p.id == id)
    }

    pub fn plane_mut(&mut self, id: ItemId) -> Option<&mut ConstructionPlane> {
        self.planes.iter_mut().find(|p| p.id == id)
    }

    pub fn body(&self, id: ItemId) -> Option<&Body> {
        self.bodies.iter().find(|b| b.id == id)
    }

    pub fn body_mut(&mut self, id: ItemId) -> Option<&mut Body> {
        self.bodies.iter_mut().find(|b| b.id == id)
    }

    pub fn sketch(&self, id: ItemId) -> Option<&Sketch> {
        self.sketches.iter().find(|s| s.id == id)
    }

    pub fn sketch_mut(&mut self, id: ItemId) -> Option<&mut Sketch> {
        self.sketches.iter_mut().find(|s| s.id == id)
    }

    pub fn item_name(&self, id: ItemId) -> Option<&str> {
        self.body(id)
            .map(|b| b.name.as_str())
            .or_else(|| self.sketch(id).map(|s| s.name.as_str()))
            .or_else(|| self.plane(id).map(|p| p.name.as_str()))
            .or_else(|| self.axis(id).map(|a| a.name.as_str()))
            .or_else(|| self.image(id).map(|i| i.name.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty() && self.sketches.is_empty() && self.planes.is_empty() && self.axes.is_empty() && self.images.is_empty()
    }

    pub fn remove_item(&mut self, id: ItemId) -> bool {
        // A folder goes with everything in it.
        if let Some(pos) = self.folders.iter().position(|f| f.id == id) {
            let folder = self.folders.remove(pos);
            for item in folder.items {
                self.remove_item(item);
            }
            return true;
        }
        for f in &mut self.folders {
            f.items.retain(|i| *i != id);
        }
        self.item_ranks.retain(|r| r.0 != id);
        let count = |d: &Self| d.bodies.len() + d.sketches.len() + d.planes.len() + d.axes.len() + d.images.len();
        let before = count(self);
        self.bodies.retain(|b| b.id != id);
        self.sketches.retain(|s| s.id != id);
        self.planes.retain(|p| p.id != id);
        self.axes.retain(|a| a.id != id);
        self.images.retain(|i| i.id != id);
        // Decals go with their body.
        let bodies = &self.bodies;
        self.decals.retain(|d| bodies.iter().any(|b| b.id == d.body));
        before != count(self)
    }

    /// Restores the id and name counters after loading.
    pub(crate) fn set_counters(&mut self, next_id: u64, bodies: u32, sketches: u32, planes: u32, axes: u32) {
        self.next_id = next_id.max(1);
        self.body_counter = bodies;
        self.sketch_counter = sketches;
        self.plane_counter = planes;
        self.axis_counter = axes;
    }

    /// The id the next new item gets.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Makes sure no id below `next` is handed out again (a skipped history step had used them).
    pub fn reserve_ids(&mut self, next: u64) {
        self.next_id = self.next_id.max(next);
    }

    pub(crate) fn counters(&self) -> (u64, u32, u32, u32, u32) {
        (self.next_id, self.body_counter, self.sketch_counter, self.plane_counter, self.axis_counter)
    }

    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut min = DVec3::splat(f64::INFINITY);
        let mut max = DVec3::splat(f64::NEG_INFINITY);
        let mut any = false;
        for b in self.bodies.iter().filter(|b| b.visible) {
            for p in &b.mesh.positions {
                let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                min = min.min(p);
                max = max.max(p);
                any = true;
            }
        }
        for s in self.sketches.iter().filter(|s| s.visible) {
            for p in s.outline_points() {
                min = min.min(p);
                max = max.max(p);
                any = true;
            }
        }
        any.then_some((min, max))
    }
}

/// Formats a length the way the reference app labels dimensions ("11.9159 mm").
/// View ▸ Foot & Inch Format ▸ Architectural (1' - 4 1/2") rather than Decimal (1.375'), and View ▸ Degree Format ▸
/// Fractional (27°15') rather than Decimal (27.25°): how values show, app-wide.
static ARCHITECTURAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FRACTIONAL_DEGREES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_display_formats(architectural: bool, fractional_degrees: bool) {
    ARCHITECTURAL.store(architectural, std::sync::atomic::Ordering::Relaxed);
    FRACTIONAL_DEGREES.store(fractional_degrees, std::sync::atomic::Ordering::Relaxed);
}

pub fn display_formats() -> (bool, bool) {
    (ARCHITECTURAL.load(std::sync::atomic::Ordering::Relaxed), FRACTIONAL_DEGREES.load(std::sync::atomic::Ordering::Relaxed))
}

/// Feet and inches to the nearest 1/16": 1' - 4 1/2".
fn architectural(mm: f64) -> String {
    let sixteenths = (mm.abs() / 25.4 * 16.0).round() as i64;
    let sign = if mm < 0.0 && sixteenths > 0 { "-" } else { "" };
    let (feet, rest) = (sixteenths / (12 * 16), sixteenths % (12 * 16));
    let (inches, frac) = (rest / 16, rest % 16);
    let fraction = if frac == 0 {
        String::new()
    } else {
        let g = gcd(frac, 16);
        format!(" {}/{}", frac / g, 16 / g)
    };
    format!("{sign}{feet}' - {inches}{fraction}\"")
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// An angle in degrees as set in View ▸ Degree Format: "27.25°" or "27°15'".
pub fn format_angle(deg: f64) -> String {
    if display_formats().1 {
        let total = (deg.abs() * 60.0).round() as i64;
        let sign = if deg < 0.0 && total > 0 { "-" } else { "" };
        let (d, m) = (total / 60, total % 60);
        return if m == 0 { format!("{sign}{d}°") } else { format!("{sign}{d}°{m}'") };
    }
    let mut s = format!("{deg:.4}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    format!("{s}°")
}

pub fn format_length(mm: f64, units: Units) -> String {
    if units == Units::Foot && display_formats().0 {
        return architectural(mm);
    }
    let v = mm / units.scale();
    let mut s = format!("{v:.4}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    if s == "-0" {
        s = "0".into();
    }
    format!("{s} {}", units.suffix())
}

pub fn v2(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

#[cfg(test)]
mod tests {
    #[test]
    fn other_picture_formats_become_png() {
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        for format in [image::ImageFormat::Bmp, image::ImageFormat::Tiff, image::ImageFormat::Gif] {
            let mut bytes = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut bytes), format).unwrap();
            let png = super::RefImage::normalize(bytes).unwrap();
            assert_eq!(image::guess_format(&png).unwrap(), image::ImageFormat::Png, "{format:?}");
            let (ext, (w, h, _)) = super::RefImage::decode(&png).unwrap();
            assert_eq!((ext.as_str(), w, h), ("png", 3, 2));
        }
        assert!(super::RefImage::normalize(b"not a picture".to_vec()).is_err());
    }

    use super::*;

    #[test]
    fn lengths_trim_trailing_zeros() {
        assert_eq!(format_length(11.91590, Units::Millimeter), "11.9159 mm");
        assert_eq!(format_length(10.0, Units::Millimeter), "10 mm");
        assert_eq!(format_length(25.4, Units::Inch), "1 in");
        assert_eq!(format_length(-0.00001, Units::Millimeter), "0 mm");
        assert_eq!(architectural(16.5 * 25.4), "1' - 4 1/2\"");
        assert_eq!(architectural(12.0 * 25.4), "1' - 0\"");
    }

    #[test]
    fn names_count_up() {
        let mut d = Document::new("x");
        assert_eq!(d.next_body_name(), "Body 01");
        assert_eq!(d.next_body_name(), "Body 02");
        assert_eq!(d.next_sketch_name(), "Sketch 01");
        assert_ne!(d.alloc_id(), d.alloc_id());
    }
}
