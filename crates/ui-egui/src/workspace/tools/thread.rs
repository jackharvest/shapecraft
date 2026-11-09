use serde_json::{Value, json};
use shapecraft_doc::{ItemId, Pick};
use shapecraft_engine::{Session, threads};
use shapecraft_kernel::CylinderFace;

#[derive(Clone, Debug)]
pub struct ThreadTool {
    pub body: ItemId,
    pub face: u32,
    pub wall: CylinderFace,
    pub preset: &'static threads::ThreadPreset,
    pub length: f64,
    pub offset: f64,
    pub clearance: f64,
    pub left_hand: bool,
    pub reverse: bool,
    pub dragging: Option<(egui::Pos2, f64)>,
}

impl ThreadTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let [Pick::Face { body, face }] = s.selection.as_slice() else { return None };
        let b = s.doc.body(*body)?;
        let wall = s.kernel.cylinder_face(&b.shape, *face).ok()??;
        let series = if matches!(s.doc.units, shapecraft_doc::Units::Inch | shapecraft_doc::Units::Foot) { "UNC" } else { "Metric coarse" };
        let preset = threads::nearest(2.0 * wall.radius, wall.internal, series);
        Some(Self {
            body: *body,
            face: *face,
            wall,
            preset,
            length: wall.length.min(10.0 * preset.pitch),
            offset: 0.0,
            clearance: 0.0,
            left_hand: false,
            reverse: false,
            dragging: None,
        })
    }

    pub fn params(&self) -> Value {
        json!({ "body": self.body.0, "face": self.face, "preset": self.preset.id,
            "kind": if self.wall.internal { "internal" } else { "external" }, "length": self.length,
            "offset": self.offset, "clearance": self.clearance, "hand": if self.left_hand { "left" } else { "right" }, "reverse": self.reverse })
    }
}
