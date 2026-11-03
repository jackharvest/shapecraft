//! DWG (AutoCAD R2000, `AC1015`) writer, written from the Open Design Alliance's freely published
//! "Open Design Specification for .dwg files". No code from any other DWG implementation is involved.
//!
//! The model is deliberately small and neutral: layers (colour, lineweight, linetype), simple dashed
//! linetypes, and model-space entities – LINE, ARC, CIRCLE, LWPOLYLINE, SPLINE, TEXT and MTEXT – in
//! millimetres. [`write`] turns it into the bytes of a complete drawing: header variables, classes, the
//! symbol tables, the named object dictionary with its layouts, the object map and every CRC.
//! [`inspect`] reads such a file back structurally (sections, CRCs, objects) for tests and self-checks.

pub mod bits;
mod check;
mod write;

pub use check::{Inspected, InspectedObject, inspect};
pub use write::write;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("bad drawing for DWG: {0}")]
    Bad(String),
    #[error("not a readable R2000 DWG: {0}")]
    Read(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A lineweight. Millimetre values snap to the nearest standard weight (0.00–2.11 mm).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LineWeight {
    #[default]
    ByLayer,
    ByBlock,
    /// The drawing default (LWDEFAULT).
    Default,
    Mm(f64),
}

/// A layer. `color` is an AutoCAD Color Index (1–255); `linetype` names a linetype ("Continuous" when `None`).
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub color: u8,
    pub lineweight: LineWeight,
    pub linetype: Option<String>,
}

impl Layer {
    pub fn new(name: &str, color: u8) -> Self {
        Self { name: name.to_string(), color, lineweight: LineWeight::Default, linetype: None }
    }
}

/// A simple linetype: dash lengths in mm, positive = pen down, negative = gap, 0 = dot.
#[derive(Clone, Debug, PartialEq)]
pub struct Linetype {
    pub name: String,
    pub description: String,
    pub dashes: Vec<f64>,
}

/// Horizontal text alignment (DXF 72).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical text alignment (DXF 73).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line {
        a: [f64; 3],
        b: [f64; 3],
    },
    /// Counter-clockwise from `start` to `end` (radians), in the XY plane.
    Arc {
        center: [f64; 3],
        radius: f64,
        start: f64,
        end: f64,
    },
    Circle {
        center: [f64; 3],
        radius: f64,
    },
    /// A 2D polyline; `bulges` is empty or one per vertex (tan of a quarter of the arc angle to the next vertex).
    Polyline {
        points: Vec<[f64; 2]>,
        bulges: Vec<f64>,
        closed: bool,
    },
    /// A NURBS curve by control points and knots (`weights` empty = non-rational).
    Spline {
        degree: u32,
        knots: Vec<f64>,
        control: Vec<[f64; 3]>,
        weights: Vec<f64>,
        closed: bool,
    },
    /// Single-line text; `at` is the insertion point (also the alignment point when not left/baseline).
    Text {
        at: [f64; 3],
        height: f64,
        rotation: f64,
        value: String,
        h_align: HAlign,
        v_align: VAlign,
    },
    /// Paragraph text; `attachment` is 1–9 (top-left … bottom-right), `width` the reference width (0 = none).
    MText {
        at: [f64; 3],
        height: f64,
        width: f64,
        attachment: u8,
        value: String,
    },
}

/// A model-space entity. `color` is an ACI (0 = ByBlock, `None` = ByLayer); `linetype` `None` = ByLayer.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub layer: String,
    pub color: Option<u8>,
    pub lineweight: LineWeight,
    pub linetype: Option<String>,
    pub shape: Shape,
}

impl Entity {
    pub fn new(layer: &str, shape: Shape) -> Self {
        Self { layer: layer.to_string(), color: None, lineweight: LineWeight::ByLayer, linetype: None, shape }
    }
}

/// A whole drawing in model space, units millimetres.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Drawing {
    pub layers: Vec<Layer>,
    pub linetypes: Vec<Linetype>,
    pub entities: Vec<Entity>,
    /// Drawing limits (the sheet), lower-left and upper-right; the extents when `None`.
    pub limits: Option<([f64; 2], [f64; 2])>,
    /// Creation date (year, month, day) for TDCREATE/TDUPDATE; 2000-01-01 when `None`.
    pub date: Option<(i64, u32, u32)>,
}

/// The Julian day number of a Gregorian date (TDCREATE's day part).
pub fn julian_day(y: i64, m: u32, d: u32) -> i64 {
    let (m, d) = (m as i64, d as i64);
    let a = (14 - m) / 12;
    let y = y + 4800 - a;
    let m = m + 12 * a - 3;
    d + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
}

#[cfg(test)]
mod tests {
    #[test]
    fn julian_days() {
        assert_eq!(super::julian_day(2000, 1, 1), 2451545);
        assert_eq!(super::julian_day(2019, 2, 17), 2458532);
    }
}
