//! Shapr3D project files (`.shapr`), for moving projects between Shapr3D and ShapeCraft.
//!
//! Everything here was learned from files we made ourselves in Shapr3D (see `plan/shapr-format.md`, kept out
//! of the repository with the samples): a `.shapr` is a ZIP holding `.metadata` (JSON) and `workspace`, a
//! SQLite database. The model's current state is a Parasolid partition (`BodyRevisionBlocks`), the
//! parametric history is MessagePack (`HistoryTreeNodes`), imported solids are Parasolid parts
//! (`HistoryImportedPrototypes`). Shapr3D rebuilds the model from the history when the cached partition,
//! deltas and calls are missing, which is how edits are written back.

pub mod write;
pub mod zip;

pub use write::{Look, Writer};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rmpv::Value;
use shapecraft_kernel::brep::Brep;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("zip: {0}")]
    Zip(&'static str),
    #[error("not a .shapr project: {0}")]
    Format(String),
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parasolid data: {0}")]
    Xt(#[from] shapecraft_xt::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The two parts of a `.shapr` file.
#[derive(Clone, Debug)]
pub struct ShaprFile {
    pub metadata: serde_json::Value,
    pub workspace: Vec<u8>,
}

impl ShaprFile {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let entries = zip::read(bytes)?;
        let mut metadata = serde_json::Value::Null;
        let mut workspace = None;
        for (name, data) in entries {
            match name.as_str() {
                ".metadata" => metadata = serde_json::from_slice(&data).unwrap_or(serde_json::Value::Null),
                "workspace" => workspace = Some(data),
                _ => {}
            }
        }
        let workspace = workspace.ok_or_else(|| Error::Format("no workspace database".into()))?;
        if !workspace.starts_with(b"SQLite format 3\0") {
            return Err(Error::Format("workspace is not a SQLite database".into()));
        }
        Ok(Self { metadata, workspace })
    }

    pub fn read(path: &Path) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let meta = serde_json::to_vec(&self.metadata).unwrap_or_else(|_| b"{}".to_vec());
        zip::write(&[("workspace", &self.workspace), (".metadata", &meta)])
    }

    /// The workspace database, opened from a private temporary copy.
    pub fn db(&self) -> Result<Db> {
        Db::from_bytes(&self.workspace)
    }
}

/// A workspace database in a temporary file (SQLite wants a file); removed on drop.
pub struct Db {
    pub conn: rusqlite::Connection,
    path: PathBuf,
}

static SEQ: AtomicU64 = AtomicU64::new(0);

impl Db {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("shapecraft-shapr-{}-{n}.db", std::process::id()));
        std::fs::write(&path, bytes)?;
        let conn = rusqlite::Connection::open(&path)?;
        Ok(Self { conn, path })
    }

    /// The database file as it is now.
    /// Workspaces use WAL journaling: checkpoint so the main file holds everything.
    pub fn bytes(&self) -> Result<Vec<u8>> {
        self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(std::fs::read(&self.path)?)
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut p = self.path.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}

/// One step of Shapr3D's parametric history.
#[derive(Clone, Debug)]
pub struct Step {
    pub node: i64,
    /// As shown in Shapr3D's History ("Offset Face 01").
    pub name: String,
    /// The operation ("OffsetFace", "MaterializeImportedBodies", …).
    pub op: String,
    /// Parameter nodes: (node id, decoded properties).
    pub params: Vec<(i64, Value)>,
}

#[derive(Clone, Debug)]
pub struct Body {
    /// The partition that holds it (Shapr3D keeps one per body).
    pub partition: Option<i64>,
    /// The imported body it is, when it comes from an import's prototype (no partition cached).
    pub import: Option<i64>,
    /// Folder path from the Items list.
    pub folder: Vec<String>,
    /// The history step (tree node id) that created it, when known.
    pub step: Option<i64>,
    pub name: Option<String>,
    pub brep: Brep,
    pub color: Option<[f32; 3]>,
    /// Hidden in Shapr3D's Items list.
    pub hidden: bool,
    /// A facet body's triangles (millimeters), when it is a mesh rather than exact faces.
    pub mesh: Option<shapecraft_xt::Mesh>,
}

#[derive(Clone, Debug)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub hidden: bool,
    /// Raw `NamedChildren` elements: (id, type). Type 0 = folder, 6 = item (metadata id).
    pub children: Vec<(String, i64)>,
}

/// What we understand of a project.
#[derive(Clone, Debug, Default)]
pub struct Project {
    pub settings: BTreeMap<String, String>,
    pub steps: Vec<Step>,
    /// The model's current solids (from the cached partition, else the imported prototypes).
    pub bodies: Vec<Body>,
    pub folders: Vec<Folder>,
    /// Item names from the metadata table, in id order.
    pub item_names: Vec<(i64, String)>,
    /// Items (named things in the Items list) with the folder path they sit in.
    pub items: Vec<Item>,
    pub sketches: Vec<Sketch>,
    /// Extrude steps simple enough to redo here: one sketch profile pulled a distance as a new body.
    pub extrudes: Vec<ExtrudeStep>,
    /// Solids that couldn't be read (geometry not supported yet), as messages; the rest of the project still is.
    pub unreadable: Vec<String>,
    /// Our own parametric history, kept in files we write ("ShapeCraftHistory"; Shapr3D ignores the table), as JSON.
    pub own_history: Option<String>,
}

/// A Shapr3D Extrude of one sketch profile, with only the settings learned so far (distance, draft, sides; the
/// rest at their defaults).
#[derive(Clone, Debug, PartialEq)]
pub struct ExtrudeStep {
    /// The step's tree node (it anchors the body it makes).
    pub node: i64,
    /// The profile's sketch (`SketchControllers.SketchID`).
    pub sketch: i64,
    /// Millimeters along the sketch normal (per side when symmetric).
    pub distance: f64,
    /// Degrees; positive leans the walls in.
    pub draft: f64,
    pub symmetric: bool,
    /// The profile's outer loop as sketch curve ids, and whether it has holes.
    pub outline: Vec<i64>,
    pub holes: bool,
    /// The body's item (name, colour, visibility) when the step anchors one.
    pub item: Option<Item>,
    /// Whether a later step works on what this one made (a move, a fillet…): then the step alone doesn't give
    /// the body.
    pub used_later: bool,
    /// The one later step, when it is a plain Fillet, Chamfer or Offset Face nothing else builds on: the body is
    /// this extrude rounded, bevelled or pushed by it.
    pub fillet: Option<FilletStep>,
}

/// A constant-radius Fillet, an equal-distance Chamfer, or an Offset Face of Shapr3D's (every other setting at its
/// default).
#[derive(Clone, Debug, PartialEq)]
pub struct FilletStep {
    pub node: i64,
    /// Millimeters: the radius, the chamfer's distance, or the offset (a Total offset's whole thickness).
    pub radius: f64,
    pub kind: Finish,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Fillet,
    Chamfer,
    /// Offset Face by a distance along the faces' outward normals.
    Offset,
    /// Offset Face to a total thickness measured from a reference face.
    OffsetTotal,
}

/// A sketch as its controller and curve rows keep it (lengths here in millimeters).
#[derive(Clone, Debug, PartialEq)]
pub struct Sketch {
    pub id: i64,
    pub name: String,
    pub hidden: bool,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    /// The plane's x direction.
    pub u: [f64; 3],
    pub curves: Vec<SketchCurve>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SketchCurve {
    /// Type 2: center point in plane coordinates and radius.
    Circle { id: i64, center: [f64; 2], radius: f64, construction: bool },
    /// Type 0: a straight segment.
    Line { id: i64, a: [f64; 2], b: [f64; 2], construction: bool },
    /// Type 1: counter-clockwise from `start` to `end` around `center`.
    Arc { id: i64, center: [f64; 2], start: [f64; 2], end: [f64; 2], construction: bool },
    /// Type 4: a curve through `points` (Shapr3D's "Fit" spline); `poles` are its B-spline control points.
    Spline { id: i64, points: Vec<[f64; 2]>, poles: Vec<[f64; 2]>, construction: bool },
    /// Curve kinds not learned yet, kept as their JSON.
    Other { id: i64, data: serde_json::Value },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The `HistoryNames` id the item's metadata hangs off.
    pub name_id: i64,
    pub name: String,
    /// Folder names from the top ("box.step"), not counting the root.
    pub folder: Vec<String>,
    /// Hidden in the Items list (itself, or a folder it sits in).
    pub hidden: bool,
    /// Its material's base colour (0–1), when it isn't Shapr3D's default grey.
    pub color: Option<[f32; 3]>,
}

pub(crate) fn text(v: rusqlite::types::Value) -> String {
    match v {
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Blob(b) => String::from_utf8_lossy(&b).into_owned(),
        rusqlite::types::Value::Integer(i) => i.to_string(),
        rusqlite::types::Value::Real(r) => r.to_string(),
        rusqlite::types::Value::Null => String::new(),
    }
}

pub(crate) fn blob(v: rusqlite::types::Value) -> Vec<u8> {
    match v {
        rusqlite::types::Value::Blob(b) => b,
        rusqlite::types::Value::Text(t) => t.into_bytes(),
        _ => Vec::new(),
    }
}

/// MessagePack properties of a history node.
pub fn unpack(bytes: &[u8]) -> Option<Value> {
    rmpv::decode::read_value(&mut &bytes[..]).ok()
}

pub fn pack(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = rmpv::encode::write_value(&mut out, v);
    out
}

impl Project {
    pub fn read(file: &ShaprFile) -> Result<Self> {
        let db = file.db()?;
        let c = &db.conn;
        let mut p = Project::default();
        {
            let mut q = c.prepare("SELECT SettingName, SettingValue FROM Settings")?;
            let rows = q.query_map([], |r| Ok((text(r.get(0)?), text(r.get(1)?))))?;
            for row in rows.flatten() {
                p.settings.insert(row.0, row.1);
            }
        }
        let mut nodes: BTreeMap<i64, (i64, Value)> = BTreeMap::new();
        {
            let mut q = c.prepare("SELECT HistoryTreeNodeID, HistoryTreeNodeType, Properties FROM HistoryTreeNodes")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, blob(r.get(2)?))))?;
            for (id, ty, props) in rows.flatten() {
                if let Some(v) = unpack(&props) {
                    nodes.insert(id, (ty, v));
                }
            }
        }
        // The root (type 0) lists the steps in order: `['<root>', [ids]]`, or just `[[ids]]` in releases up to early 2025.
        let root_children = nodes
            .values()
            .find(|(ty, _)| *ty == 0)
            .and_then(|(_, v)| v.as_array()?.iter().find_map(|x| x.as_array().cloned()))
            .unwrap_or_default();
        for child in root_children {
            let Some(id) = child.as_i64() else { continue };
            let Some((_, v)) = nodes.get(&id) else { continue };
            let Some(a) = v.as_array() else { continue };
            // A step is `[_, _, name, op, [params]]`, or `[_, name, op, [params]]` in the older files: the name and
            // operation are the two strings, the parameter nodes the list after them.
            let at = a.iter().position(Value::is_str).unwrap_or(2);
            let a: Vec<Value> =
                std::iter::repeat_n(Value::Nil, 2usize.saturating_sub(at)).chain(a[at.saturating_sub(2)..].iter().cloned()).collect();
            let s = |i: usize| a.get(i).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let params = a
                .get(4)
                .and_then(|x| x.as_array())
                .map(|ps| ps.iter().filter_map(|x| x.as_i64()).filter_map(|pid| nodes.get(&pid).map(|(_, pv)| (pid, pv.clone()))).collect())
                .unwrap_or_default();
            p.steps.push(Step { node: id, name: s(2), op: s(3), params });
        }
        {
            let mut q = c.prepare("SELECT FolderID, FolderName, Hidden, NamedChildren FROM HistoryFolders")?;
            let rows = q.query_map([], |r| Ok((text(r.get(0)?), text(r.get(1)?), r.get::<_, Option<bool>>(2)?, text(r.get(3)?))))?;
            for (id, name, hidden, children) in rows.flatten() {
                let json: serde_json::Value = serde_json::from_str(&children).unwrap_or_default();
                let children = json["elements"]
                    .as_array()
                    .map(|a| a.iter().map(|e| (e["id"].as_str().unwrap_or("").to_string(), e["type"].as_i64().unwrap_or(-1))).collect())
                    .unwrap_or_default();
                p.folders.push(Folder { id, name, hidden: hidden.unwrap_or(false), children });
            }
        }
        {
            let mut q = c.prepare("SELECT MetadataID, Data FROM Metadata WHERE MetadataTypeID = 0 ORDER BY MetadataID")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?;
            p.item_names = rows.flatten().collect();
        }
        if has_table(c, "ShapeCraftHistory") {
            p.own_history = c
                .query_row("SELECT Data FROM ShapeCraftHistory WHERE Version = 1 LIMIT 1", [], |r| r.get::<_, Vec<u8>>(0))
                .ok()
                .and_then(|b| String::from_utf8(b).ok());
        }
        p.items = items(c, &p.folders)?;
        p.sketches = sketches(c)?;
        p.extrudes = extrudes(c, &p.steps, &p.items)?;
        let unreadable;
        (p.bodies, unreadable) = current_bodies(c)?;
        name_bodies(c, &p.steps, &p.items, &mut p.bodies)?;
        apply_moves(c, &p.steps, &mut p.bodies)?;
        let gone = drop_removed(c, &p.steps, &mut p.bodies)?;
        // Only bodies still in the model count as lost: a deleted body's partition stays in the file.
        p.unreadable = unreadable.into_iter().filter(|(pid, _)| pid.is_none_or(|pid| !gone.contains(&pid))).map(|(_, e)| e).collect();
        Ok(p)
    }
}

impl Project {
    /// The step that made sketch `id` (a `MaterializeSketchPlane` naming it as [10, id]).
    pub fn sketch_step(&self, id: i64) -> Option<i64> {
        fn names(v: &Value, id: i64) -> bool {
            match v {
                Value::Array(a) => {
                    matches!(a.as_slice(), [Value::Integer(k), Value::Integer(n)] if k.as_i64() == Some(10) && n.as_i64() == Some(id))
                        || a.iter().any(|x| names(x, id))
                }
                _ => false,
            }
        }
        self.steps.iter().find(|s| s.op == "MaterializeSketchPlane" && s.params.iter().any(|(_, v)| names(v, id))).map(|s| s.node)
    }
}

const MM: f64 = 1000.0;

/// Our own table in files we write: copies of Shapr3D-made bodies, for reading the file again before Shapr3D has
/// rebuilt its solids.
pub(crate) const HISTORY_SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS \"ShapeCraftHistory\" (\"Version\" INTEGER NOT NULL, \"Data\" BLOB NOT NULL);";

pub(crate) const STASH_SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS \"ShapeCraftBodies\" (\"HistoryTreeNodeID\" INTEGER NOT NULL, \"Body\" BLOB NOT NULL);";

/// The names a step's parameter uses ([7, name] anywhere in it).
fn name_refs(v: &Value, out: &mut Vec<i64>) {
    if let Value::Array(a) = v {
        if let [Value::Integer(k), Value::Integer(n)] = a.as_slice()
            && k.as_i64() == Some(7)
        {
            out.extend(n.as_i64());
        }
        a.iter().for_each(|x| name_refs(x, out));
    }
}

/// Whether name `id` stands for something step `node` made: its call key names the step, or a name it wraps or
/// is built from does (followed a few levels deep).
fn made_by(names: &BTreeMap<i64, serde_json::Value>, id: i64, node: i64, depth: u32) -> bool {
    // An edge is named by the faces on either side (neighborFaceOriginals0/1).
    const KEYS: &[&str] = &[
        "wrappedName",
        "nameToAnchor",
        "sourceTopologyNames",
        "originals",
        "filling",
        "originalEdgeOrCurve",
        "sourceTopologyName",
        "neighborFaceOriginals0",
        "neighborFaceOriginals1",
    ];
    let Some(n) = names.get(&id) else { return false };
    if n["callKey"]["nodeID"].as_i64() == Some(node) || n["callKeyPrefix"]["nodeID"].as_i64() == Some(node) {
        return true;
    }
    if depth >= 8 {
        return false;
    }
    let mut inner = Vec::new();
    fn collect(v: &serde_json::Value, out: &mut Vec<i64>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    if KEYS.contains(&k.as_str()) {
                        match x {
                            serde_json::Value::Number(n) => out.extend(n.as_i64()),
                            serde_json::Value::Array(a) => out.extend(a.iter().filter_map(serde_json::Value::as_i64)),
                            _ => {}
                        }
                    }
                    collect(x, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
            _ => {}
        }
    }
    collect(n, &mut inner);
    inner.into_iter().take(64).any(|i| made_by(names, i, node, depth + 1))
}

/// Extrude steps of one sketch profile whose other settings are the defaults observed (sample n4).
fn extrudes(c: &rusqlite::Connection, steps: &[Step], items: &[Item]) -> Result<Vec<ExtrudeStep>> {
    let names: BTreeMap<i64, serde_json::Value> = {
        let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type IN (2, 3, 19)")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?;
        rows.flatten().take(1_000_000).map(|(id, d)| (id, serde_json::from_str(&d).unwrap_or_default())).collect()
    };
    let all_names: BTreeMap<i64, serde_json::Value> = {
        let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?;
        rows.flatten().take(1_000_000).map(|(id, d)| (id, serde_json::from_str(&d).unwrap_or_default())).collect()
    };
    let curve_sketch: BTreeMap<i64, i64> = {
        let mut q = c.prepare("SELECT CurveID, SketchID FROM SketchCurves")?;
        q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?.flatten().collect()
    };
    // The one value inside a parameter ([[[tag, value]]]).
    let inner = |v: &Value| -> Option<(i64, Value)> {
        let t = v.as_array()?.first()?.as_array()?.first()?.as_array()?;
        Some((t.first()?.as_i64()?, t.get(1).cloned().unwrap_or(Value::Nil)))
    };
    let length = |v: &Value, unit: i64| -> Option<f64> {
        let (12, Value::Array(q)) = inner(v)? else { return None };
        let u = q.first()?.as_array()?;
        (u.first()?.as_i64()? == unit && u.get(1)?.as_i64()? == 1).then(|| q.get(1)?.as_f64()).flatten().filter(|x| x.is_finite())
    };
    let int = |v: &Value| match inner(v) {
        Some((2, n)) => n.as_i64(),
        _ => None,
    };
    let flag = |v: &Value| match inner(v) {
        Some((1, b)) => b.as_bool(),
        _ => None,
    };
    let empty = |v: &Value| {
        v.as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_array)
            .is_some_and(|t| t.len() == 1 && t[0].as_i64() == Some(0))
    };
    let mut out = Vec::new();
    for step in steps.iter().filter(|s| s.op == "Extrude") {
        let p: Vec<&Value> = step.params.iter().map(|(_, v)| v).collect();
        if p.len() != 15 {
            continue;
        }
        // Profile: [[[9, [[[7, wrapper]]]]]] → a loop of fins → their sketch curves.
        let Some((9, Value::Array(refs))) = inner(p[0]) else { continue };
        let [only] = refs.as_slice() else { continue };
        let Some(wrapper) = only
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_array)
            .filter(|a| a.first().and_then(Value::as_i64) == Some(7))
            .and_then(|a| a.get(1)?.as_i64())
        else {
            continue;
        };
        let Some(face) = names.get(&wrapper).and_then(|w| w["wrappedName"].as_i64()).and_then(|f| names.get(&f)) else { continue };
        let loop_curves = |fins: &serde_json::Value| -> Vec<i64> {
            fins.as_array()
                .into_iter()
                .flatten()
                .flat_map(|fin| fin["originalEdgesWithSense"].as_array().cloned().unwrap_or_default())
                .filter_map(|e| e["originalEdgeOrCurve"].as_i64())
                .filter_map(|n| names.get(&n)?["curveID"].as_i64())
                .collect()
        };
        let outline = loop_curves(&face["fins"]);
        let hole_curves: Vec<i64> =
            face["finsOfInnerLoops"].as_array().into_iter().flatten().flat_map(|l| loop_curves(l.get("fins").unwrap_or(l))).collect();
        let holes = face["finsOfInnerLoops"].as_array().is_some_and(|l| !l.is_empty());
        let curves: Vec<i64> = outline.iter().chain(&hole_curves).copied().collect();
        let sketches: Vec<i64> =
            curves.iter().filter_map(|c| curve_sketch.get(c).copied()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let ([sketch], false) = (sketches.as_slice(), curves.is_empty()) else { continue };
        let (Some(distance), Some(draft), Some(sides)) = (length(p[1], 1), length(p[2], 2), int(p[9])) else { continue };
        let defaults = empty(p[3])
            && int(p[4]) == Some(0)
            && int(p[5]) == Some(0)
            && inner(p[6]).is_some_and(|(t, v)| t == 9 && v.as_array().is_some_and(Vec::is_empty))
            && flag(p[7]) == Some(false)
            && flag(p[8]) == Some(false)
            && int(p[10]) == Some(0)
            && length(p[11], 1) == Some(0.0)
            && empty(p[12])
            && length(p[13], 1) == Some(0.0)
            && empty(p[14]);
        if !defaults || sides > 1 || distance == 0.0 {
            continue;
        }
        let anchored: Vec<i64> = {
            let mut q = c.prepare("SELECT AnchoredNameID FROM HistoryNameAnchoringPoints WHERE HistoryTreeNodeID = ?1")?;
            q.query_map([step.node], |r| r.get(0))?.flatten().collect()
        };
        let item = items.iter().find(|i| anchored.contains(&i.name_id)).cloned();
        let at = steps.iter().position(|s| s.node == step.node).unwrap_or(steps.len());
        let users = |node: i64, from: usize| -> Vec<&Step> {
            steps[from..]
                .iter()
                .filter(|later| {
                    let mut refs = Vec::new();
                    later.params.iter().for_each(|(_, v)| name_refs(v, &mut refs));
                    refs.iter().any(|r| made_by(&all_names, *r, node, 0))
                })
                .collect()
        };
        let later = users(step.node, at + 1);
        let used_later = !later.is_empty();
        // Fillet params: 0 what to round, 3 the radius; the rest as a plain constant fillet leaves them. Chamfer:
        // 0 the edges, 1 and 2 faces, 3 the distance, then [[0]], true, 0 for an equal-distance chamfer. Offset Face:
        // 0 the faces, 1 the distance, 2 its type (0 offset, 1 total), 3 [[2,3]], 4 the face a total is measured from.
        let fillet = match later.as_slice() {
            [f] if (f.op == "Fillet" && f.params.len() == 9)
                || (f.op == "Chamfer" && f.params.len() == 7)
                || (f.op == "OffsetFace" && f.params.len() == 5) =>
            {
                let q: Vec<&Value> = f.params.iter().map(|(_, v)| v).collect();
                let (kind, at) = match f.op.as_str() {
                    "Chamfer" => (Some(Finish::Chamfer), 3),
                    "OffsetFace" => match int(q[2]) {
                        Some(0) => (Some(Finish::Offset), 1),
                        Some(1) => (Some(Finish::OffsetTotal), 1),
                        _ => (None, 1),
                    },
                    _ => (Some(Finish::Fillet), 3),
                };
                let plain = match kind {
                    Some(Finish::Chamfer) => empty(q[4]) && flag(q[5]) == Some(true) && int(q[6]) == Some(0),
                    Some(Finish::Fillet) => {
                        int(q[1]) == Some(0)
                            && int(q[2]) == Some(0)
                            && empty(q[4])
                            && int(q[5]) == Some(0)
                            && flag(q[6]) == Some(true)
                            && flag(q[7]) == Some(false)
                            && int(q[8]) == Some(0)
                    }
                    Some(Finish::Offset | Finish::OffsetTotal) => int(q[3]) == Some(3),
                    None => false,
                };
                let f_at = steps.iter().position(|s| s.node == f.node).unwrap_or(steps.len());
                match (kind, length(q[at], 1)) {
                    (Some(kind), Some(r))
                        if plain && (r > 0.0 || (kind == Finish::Offset && r != 0.0)) && users(f.node, f_at + 1).is_empty() =>
                    {
                        Some(FilletStep { node: f.node, radius: r * MM, kind })
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        out.push(ExtrudeStep {
            node: step.node,
            sketch: *sketch,
            distance: distance * MM,
            draft: draft.to_degrees(),
            symmetric: sides == 1,
            outline,
            holes,
            item,
            used_later,
            fillet,
        });
    }
    Ok(out)
}

/// Sketches in Items order, with their curves. Construction curves are listed by id per sketch.
fn sketches(c: &rusqlite::Connection) -> Result<Vec<Sketch>> {
    let mut out = Vec::new();
    let mut q = c.prepare(
        "SELECT SketchID, Name, IsHidden, PlaneCenterX, PlaneCenterY, PlaneCenterZ, PlaneNormX, PlaneNormY, PlaneNormZ, PlaneUDirX, PlaneUDirY, PlaneUDirZ \
         FROM SketchControllers ORDER BY OrderInList, SketchID",
    )?;
    let rows = q.query_map([], |r| {
        let f = |i: usize| r.get::<_, Option<f64>>(i).map(|v| v.unwrap_or(0.0));
        Ok(Sketch {
            id: r.get(0)?,
            name: text(r.get(1)?),
            hidden: r.get::<_, Option<bool>>(2)?.unwrap_or(false),
            origin: [f(3)? * MM, f(4)? * MM, f(5)? * MM],
            normal: [f(6)?, f(7)?, f(8)?],
            u: [f(9)?, f(10)?, f(11)?],
            curves: Vec::new(),
        })
    })?;
    for sk in rows.flatten() {
        out.push(sk);
    }
    let finite = |v: &serde_json::Value| v.as_f64().filter(|x| x.is_finite() && x.abs() < 1e6);
    for sk in &mut out {
        let construction: Vec<i64> = c
            .query_row("SELECT ConstructionCurves FROM ConstructionSketches WHERE SketchID = ?1", [sk.id], |r| Ok(text(r.get(0)?)))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|j| j["elements"].as_array().map(|a| a.iter().filter_map(|e| e.as_i64().or_else(|| e["id"].as_i64())).collect()))
            .unwrap_or_default();
        let mut q = c.prepare("SELECT CurveID, Data FROM SketchCurves WHERE SketchID = ?1 ORDER BY CurveID")?;
        let rows = q.query_map([sk.id], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?;
        for (id, data) in rows.flatten().take(100_000) {
            let j: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            let construction = construction.contains(&id);
            let pt = |p: &serde_json::Value| Some([finite(&p["x"])? * MM, finite(&p["y"])? * MM]);
            let pts = |a: &serde_json::Value| -> Option<Vec<[f64; 2]>> { a.as_array()?.iter().take(10_000).map(pt).collect() };
            let curve = match j["type"].as_i64() {
                Some(0) => match (pt(&j["start"]), pt(&j["end"])) {
                    (Some(a), Some(b)) => Some(SketchCurve::Line { id, a, b, construction }),
                    _ => None,
                },
                Some(1) => match (pt(&j["center"]), pt(&j["start"]), pt(&j["end"])) {
                    (Some(center), Some(start), Some(end)) => Some(SketchCurve::Arc { id, center, start, end, construction }),
                    _ => None,
                },
                Some(2) => match (pt(&j["center"]), finite(&j["radius"])) {
                    (Some(center), Some(r)) if r > 0.0 => Some(SketchCurve::Circle { id, center, radius: r * MM, construction }),
                    _ => None,
                },
                Some(4) => {
                    let (points, poles) =
                        (pts(&j["interpolatingPoints"]).unwrap_or_default(), pts(&j["controlPointPositions"]).unwrap_or_default());
                    (points.len() >= 2 || poles.len() >= 2).then_some(SketchCurve::Spline { id, points, poles, construction })
                }
                _ => None,
            };
            let curve = curve.unwrap_or(SketchCurve::Other { id, data: j });
            sk.curves.push(curve);
        }
    }
    Ok(out)
}

/// Items: a name (metadata type 0) assigned to a `HistoryNames` id; folders list an item by the id of another
/// metadata row (type 3) assigned to the same name id.
fn items(c: &rusqlite::Connection, folders: &[Folder]) -> Result<Vec<Item>> {
    let mut by_meta: BTreeMap<i64, Vec<(i64, i64, String)>> = BTreeMap::new(); // name id → (metadata id, type, data)
    {
        let mut q = c.prepare("SELECT a.NameID, m.MetadataID, m.MetadataTypeID, m.Data FROM MetadataAssignments a JOIN Metadata m ON m.MetadataID = a.MetadataID")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, text(r.get(3)?))))?;
        for (name_id, mid, ty, data) in rows.flatten() {
            by_meta.entry(name_id).or_default().push((mid, ty, data));
        }
    }
    // Folder path of every folder id, walking down from the root.
    let mut paths: BTreeMap<String, (Vec<String>, bool)> = BTreeMap::new();
    if let Some(root) = folders.iter().find(|f| f.name == "root") {
        let mut stack = vec![(root.id.clone(), Vec::<String>::new(), root.hidden)];
        while let Some((id, path, hidden)) = stack.pop() {
            if paths.contains_key(&id) || paths.len() > 100_000 {
                continue;
            }
            paths.insert(id.clone(), (path.clone(), hidden));
            if let Some(f) = folders.iter().find(|f| f.id == id) {
                for (child, ty) in &f.children {
                    if *ty == 0
                        && let Some(cf) = folders.iter().find(|x| &x.id == child)
                    {
                        let mut p = path.clone();
                        p.push(cf.name.clone());
                        stack.push((child.clone(), p, hidden || cf.hidden));
                    }
                }
            }
        }
    }
    // A body's name hangs off one anchor of its creating step and its material off a sibling anchor (both wrap the
    // same name); colouring it later adds a newer name on that anchor, so the highest name id wins.
    let mut names: BTreeMap<i64, serde_json::Value> = BTreeMap::new();
    {
        let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type IN (18, 19)")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?;
        for (id, data) in rows.flatten().take(1_000_000) {
            names.insert(id, serde_json::from_str(&data).unwrap_or_default());
        }
    }
    // An import's name anchor points at the body's name itself rather than at a wrapper of it.
    let wrapped = |name: i64| {
        let target = names.get(&name)?["nameToAnchor"].as_i64()?;
        Some(names.get(&target).and_then(|a| a["wrappedName"].as_i64()).unwrap_or(target))
    };
    let mut materials: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // wrapped name → (name id, material instance)
    for (name_id, metas) in &by_meta {
        let (Some(w), Some(m)) = (wrapped(*name_id), metas.iter().find(|m| m.1 == 2).and_then(|m| m.2.trim().parse::<i64>().ok())) else {
            continue;
        };
        if materials.get(&w).is_none_or(|e| e.0 < *name_id) {
            materials.insert(w, (*name_id, m));
        }
    }
    let colour = |instance: i64| -> Option<[f32; 3]> {
        let props: Vec<u8> =
            c.query_row("SELECT Properties FROM MaterialInstances WHERE InstanceID = ?1", [instance], |r| Ok(blob(r.get(0)?))).ok()?;
        let rgba = unpack(&props)?.as_array()?.first()?.as_u64()?;
        let rgb = (rgba >> 8) as u32 & 0xFF_FFFF;
        (rgb != 0xA0_A0A4).then(|| [(rgb >> 16) as f32 / 255.0, ((rgb >> 8) & 0xFF) as f32 / 255.0, (rgb & 0xFF) as f32 / 255.0])
    };
    let mut out = Vec::new();
    for (name_id, metas) in &by_meta {
        let Some(name) = metas.iter().find(|m| m.1 == 0).map(|m| m.2.clone()) else { continue };
        let own_hidden = metas.iter().any(|m| m.1 == 1 && m.2.trim() == "1");
        let color = wrapped(*name_id).and_then(|w| materials.get(&w)).and_then(|m| colour(m.1));
        let (folder, hidden) = folders
            .iter()
            .find(|f| f.children.iter().any(|(id, ty)| *ty == 6 && metas.iter().any(|m| m.0.to_string() == *id)))
            .and_then(|f| paths.get(&f.id).cloned())
            .unwrap_or_default();
        out.push(Item { name_id: *name_id, name, folder, hidden: hidden || own_hidden, color });
    }
    Ok(out)
}

/// Bodies a later step removed keep their partition; the cached call results say which. A call's data is
/// `[0, [0], modified, removed, _, created]`: `created` lists `[index, partition, 3, tag]`; `removed` (Delete) lists
/// `[tag, …]`; `modified` lists `[partition, [true, [[true, pmark before], after, …]]]` where `after` is `[false]` when
/// the step used the body up (the inputs of a Subtract or Union).
fn drop_removed(c: &rusqlite::Connection, steps: &[Step], bodies: &mut Vec<Body>) -> Result<std::collections::HashSet<i64>> {
    // A step may have several calls (`[node, [0]]`, `[node, [1]]`, …), kept in that order.
    let mut calls: BTreeMap<i64, BTreeMap<i64, Value>> = BTreeMap::new();
    {
        let mut q = c.prepare("SELECT CallKey, CallData FROM PersistedCalls")?;
        let rows = q.query_map([], |r| Ok((blob(r.get(0)?), blob(r.get(1)?))))?;
        for (k, d) in rows.flatten() {
            let (Some(k), Some(d)) = (unpack(&k), unpack(&d)) else { continue };
            let key = k.as_array();
            if let Some(node) = key.and_then(|a| a.first()).and_then(Value::as_i64) {
                let sub = key.and_then(|a| a.get(1)).and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_i64).unwrap_or(0);
                calls.entry(node).or_default().insert(sub, d);
            }
        }
    }
    let mut tag_of = BTreeMap::new();
    let mut gone = std::collections::HashSet::new();
    for a in steps.iter().filter_map(|step| calls.get(&step.node)).flat_map(|m| m.values()) {
        let Value::Array(a) = a else { continue };
        // Releases up to early 2025 store `[0, [0], [true, [outputs, …]]]`, each output `[partition, mark]`: a mark of
        // -1 means the step used the body up or deleted it, anything else that it's there afterwards.
        if a.len() == 3 {
            let outs = a[2]
                .as_array()
                .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
                .and_then(|x| x.get(1))
                .and_then(Value::as_array)
                .and_then(|x| x.first())
                .and_then(Value::as_array);
            for o in outs.into_iter().flatten() {
                let Some(o) = o.as_array() else { continue };
                let (Some(pid), mark) = (o.first().and_then(Value::as_i64), o.get(1).and_then(Value::as_i64)) else { continue };
                if mark == Some(-1) {
                    gone.insert(pid);
                } else {
                    gone.remove(&pid);
                }
            }
            continue;
        }
        let list = |slot: usize| {
            a.get(slot)
                .and_then(Value::as_array)
                .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
                .and_then(|x| x.get(1))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        for c in list(5).iter().flat_map(|g| g.as_array().cloned().unwrap_or_default()) {
            if let Some(c) = c.as_array()
                && let (Some(pid), Some(tag)) = (c.get(1).and_then(Value::as_i64), c.get(3).and_then(Value::as_i64))
            {
                tag_of.insert(tag, pid);
                gone.remove(&pid);
            }
        }
        for r in list(3).iter().flat_map(|g| g.as_array().cloned().unwrap_or_default()) {
            if let Some(pid) = r.as_array().and_then(|x| x.first()).and_then(Value::as_i64).and_then(|t| tag_of.get(&t)) {
                gone.insert(*pid);
            }
        }
        if let Some(Value::Array(changed)) = list(2).get(1) {
            for entry in changed {
                let Some(e) = entry.as_array() else { continue };
                let Some(pid) = e.first().and_then(Value::as_i64) else { continue };
                let after = e
                    .get(1)
                    .and_then(Value::as_array)
                    .and_then(|x| x.get(1))
                    .and_then(Value::as_array)
                    .and_then(|x| x.get(1))
                    .and_then(Value::as_array);
                match after.and_then(|x| x.first()).and_then(Value::as_bool) {
                    Some(false) => {
                        gone.insert(pid);
                    }
                    Some(true) => {
                        gone.remove(&pid);
                    }
                    None => {}
                }
            }
        }
    }
    bodies.retain(|b| b.partition.is_none_or(|p| !gone.contains(&p)));
    Ok(gone)
}

/// Each body's item: the step whose cached call first produced its partition anchors the item's name.
fn name_bodies(c: &rusqlite::Connection, steps: &[Step], items: &[Item], bodies: &mut [Body]) -> Result<()> {
    let calls = call_partitions(c, bodies)?;
    name_from_calls(c, steps, items, bodies, &calls)
}

/// Step node → the cached partitions its call made or changed.
fn call_partitions(c: &rusqlite::Connection, bodies: &[Body]) -> Result<BTreeMap<i64, Vec<i64>>> {
    let known: Vec<i64> = bodies.iter().filter_map(|b| b.partition).collect();
    let mut calls: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    {
        let mut q = c.prepare("SELECT CallKey, CallData FROM PersistedCalls")?;
        let rows = q.query_map([], |r| Ok((blob(r.get(0)?), blob(r.get(1)?))))?;
        for (k, d) in rows.flatten() {
            let (Some(k), Some(d)) = (unpack(&k), unpack(&d)) else { continue };
            let Some(node) = k.as_array().and_then(|a| a.first()).and_then(Value::as_i64) else { continue };
            // Created partitions (slot 5: [index, partition, 3, tag]) then modified ones (slot 2: [partition, …]).
            let mut found = Vec::new();
            if let Some(a) = d.as_array() {
                let list = |slot: usize| {
                    a.get(slot)
                        .and_then(Value::as_array)
                        .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
                        .and_then(|x| x.get(1))
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                };
                for c in list(5).iter().flat_map(|g| g.as_array().cloned().unwrap_or_default()) {
                    if let Some(pid) = c.as_array().and_then(|c| c.get(1)).and_then(Value::as_i64) {
                        found.push(pid);
                    }
                }
                if let Some(Value::Array(changed)) = list(2).get(1) {
                    found.extend(changed.iter().filter_map(|e| e.as_array().and_then(|e| e.first()).and_then(Value::as_i64)));
                }
            }
            found.retain(|p| known.contains(p));
            calls.entry(node).or_default().extend(found);
        }
    }
    Ok(calls)
}

/// Shapr3D keeps a Move/Rotate that is a body's last change out of its cached partition (the move lives in the
/// partition's delta); a later step that changes the body bakes it in. So each move with no later change to its
/// bodies is applied here, from the matrix its cached call keeps.
fn apply_moves(c: &rusqlite::Connection, steps: &[Step], bodies: &mut [Body]) -> Result<()> {
    let calls = call_partitions(c, bodies)?;
    let names: BTreeMap<i64, serde_json::Value> = {
        let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type IN (1, 19)")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?;
        rows.flatten().take(1_000_000).map(|(id, d)| (id, serde_json::from_str(&d).unwrap_or_default())).collect()
    };
    // [7, wrapper of the creator's body prefix] → the creating step.
    let creator = |name: i64| -> Option<i64> {
        let inner = names.get(&name)?["wrappedName"].as_i64()?;
        names.get(&inner)?["callKeyPrefix"]["nodeID"].as_i64()
    };
    fn named(v: &Value, out: &mut Vec<i64>) {
        if let Value::Array(a) = v {
            if let [Value::Integer(k), Value::Integer(n)] = a.as_slice()
                && k.as_i64() == Some(7)
            {
                out.extend(n.as_i64());
            }
            a.iter().for_each(|x| named(x, out));
        }
    }
    // A Transform's result call is keyed [node, [2]].
    let mut results: BTreeMap<i64, Value> = BTreeMap::new();
    {
        let mut q = c.prepare("SELECT CallKey, CallData FROM PersistedCalls")?;
        for (k, d) in q.query_map([], |r| Ok((blob(r.get(0)?), blob(r.get(1)?))))?.flatten() {
            let Some(k) = unpack(&k) else { continue };
            let key = k.as_array().map(|a| {
                (a.first().and_then(Value::as_i64), a.get(1).and_then(Value::as_array).and_then(|e| e.first()).and_then(Value::as_i64))
            });
            if let (Some((Some(node), Some(2))), Some(d)) = (key, unpack(&d)) {
                results.insert(node, d);
            }
        }
    }
    for (i, step) in steps.iter().enumerate().filter(|(_, s)| s.op == "Transform") {
        let Some((_, target)) = step.params.first() else { continue };
        let mut refs = Vec::new();
        named(target, &mut refs);
        let creators: Vec<i64> = refs.into_iter().filter_map(creator).collect();
        let Some(data) = results.get(&step.node) else { continue };
        // [_, _, _, [true, [[matrix], …]], …]: the 4×4 (column-major, meters).
        let matrix = Some(data).and_then(|d| d.as_array()?.get(3)?.as_array()?.get(1)?.as_array()?.first()?.as_array()?.first().cloned());
        let Some(motion) = matrix
            .as_ref()
            .and_then(|m| m.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()))
            .and_then(|m| rigid_matrix(&m))
        else {
            continue;
        };
        let later: Vec<i64> = steps[i + 1..].iter().flat_map(|s| calls.get(&s.node).cloned().unwrap_or_default()).collect();
        for b in bodies.iter_mut().filter(|b| b.step.is_some_and(|s| creators.contains(&s))) {
            if b.partition.is_some_and(|p| !later.contains(&p)) {
                b.brep.transform_rigid(motion.0, motion.1);
                if let Some(m) = &mut b.mesh {
                    m.points.iter_mut().for_each(|p| *p = motion.0 * *p + motion.1);
                }
            }
        }
    }
    Ok(())
}

fn name_from_calls(
    c: &rusqlite::Connection,
    steps: &[Step],
    items: &[Item],
    bodies: &mut [Body],
    calls: &BTreeMap<i64, Vec<i64>>,
) -> Result<()> {
    let mut anchors: BTreeMap<i64, Vec<i64>> = BTreeMap::new(); // step node → item name ids
    {
        let mut q = c.prepare("SELECT AnchoredNameID, HistoryTreeNodeID FROM HistoryNameAnchoringPoints ORDER BY AnchoredNameID")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)))?;
        for (name, node) in rows.flatten() {
            if let Some(node) = node {
                anchors.entry(node).or_default().push(name);
            }
        }
    }
    let mut claimed = std::collections::HashSet::new();
    let mut item_of: BTreeMap<i64, (Option<i64>, Option<Item>)> = BTreeMap::new(); // partition → (creating step, item)
    for step in steps {
        let Some(pids) = calls.get(&step.node) else { continue };
        // A partition a modifying step brings in (a Subtract's result) takes the item of the first partition
        // beside it that already has one (the target body).
        if pids.iter().any(|p| claimed.contains(p)) {
            let inherited = pids.iter().find_map(|p| item_of.get(p).cloned());
            for pid in pids {
                if claimed.insert(*pid)
                    && let Some((creator, item)) = &inherited
                {
                    item_of.insert(*pid, (*creator, item.clone()));
                    if let Some(b) = bodies.iter_mut().find(|b| b.partition == Some(*pid)) {
                        b.step = *creator;
                        if let Some(item) = item {
                            b.name = Some(item.name.clone());
                            b.folder = item.folder.clone();
                            b.hidden = item.hidden;
                            b.color = b.color.or(item.color);
                        }
                    }
                }
            }
            continue;
        }
        let named: Vec<&Item> = anchors
            .get(&step.node)
            .map(|ns| ns.iter().filter_map(|n| items.iter().find(|i| i.name_id == *n)).collect())
            .unwrap_or_default();
        let mut k = 0;
        for pid in pids {
            if !claimed.insert(*pid) {
                continue;
            }
            item_of.insert(*pid, (Some(step.node), named.get(k).map(|i| (*i).clone())));
            if let Some(b) = bodies.iter_mut().find(|b| b.partition == Some(*pid)) {
                b.step = Some(step.node);
                if let Some(item) = named.get(k) {
                    b.name = Some(item.name.clone());
                    b.folder = item.folder.clone();
                    b.hidden = item.hidden;
                    b.color = b.color.or(item.color);
                }
            }
            k += 1;
        }
    }
    // Bodies we kept a copy of: their step anchors their item.
    for b in bodies.iter_mut().filter(|b| b.partition.is_none() && b.import.is_none()) {
        let Some(node) = b.step else { continue };
        let named = anchors.get(&node).and_then(|ns| ns.iter().find_map(|n| items.iter().find(|i| i.name_id == *n)));
        if let Some(item) = named {
            b.name = Some(item.name.clone());
            b.folder = item.folder.clone();
            b.hidden = item.hidden;
            b.color = b.color.or(item.color);
        }
    }
    // Bodies from imports' prototypes: the import step naming the body ([11, id]) made it, and its items, in
    // order, are the bodies it brings in.
    fn imported(v: &Value, out: &mut Vec<i64>) {
        if let Value::Array(a) = v {
            if let [Value::Integer(k), Value::Integer(n)] = a.as_slice()
                && k.as_i64() == Some(11)
            {
                out.extend(n.as_i64());
            }
            a.iter().for_each(|x| imported(x, out));
        }
    }
    for step in steps.iter().filter(|s| s.op == "MaterializeImportedBodies") {
        let mut ids = Vec::new();
        step.params.iter().for_each(|(_, v)| imported(v, &mut ids));
        let named: Vec<&Item> = anchors
            .get(&step.node)
            .map(|ns| ns.iter().filter_map(|n| items.iter().find(|i| i.name_id == *n)).collect())
            .unwrap_or_default();
        for b in bodies.iter_mut().filter(|b| b.partition.is_none()) {
            let Some(k) = b.import.and_then(|id| ids.iter().position(|&i| i == id)) else { continue };
            b.step = Some(step.node);
            if let Some(item) = named.get(k) {
                b.name = Some(item.name.clone());
                b.folder = item.folder.clone();
                b.hidden = item.hidden;
                b.color = b.color.or(item.color);
            }
        }
    }
    Ok(())
}

/// Whether the workspace has a table: the schema changes between Shapr3D releases.
fn has_table(c: &rusqlite::Connection, name: &str) -> bool {
    c.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
}

/// Solids that couldn't be read, with their partition (when they have one) and why.
type Unreadable = Vec<(Option<i64>, String)>;

/// The solids of the cached partitions; the imported prototypes when no partition is cached.
fn current_bodies(c: &rusqlite::Connection) -> Result<(Vec<Body>, Unreadable)> {
    let mut parts: BTreeMap<i64, Vec<(i64, Vec<u8>)>> = BTreeMap::new();
    if has_table(c, "BodyRevisionBlocks") {
        let mut q = c.prepare("SELECT PartitionID, ChunkIndex, Block FROM BodyRevisionBlocks WHERE IsDeleted = 0 AND Phase = 2")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, blob(r.get(2)?))))?;
        for (pid, chunk, b) in rows.flatten() {
            parts.entry(pid).or_default().push((chunk, b));
        }
    } else if has_table(c, "BodyRevisionPartitions") {
        // Files saved by Shapr3D releases up to early 2025 keep each partition whole, in one row.
        let mut q = c.prepare("SELECT PartitionID, Block FROM BodyRevisionPartitions")?;
        for (pid, b) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?))))?.flatten() {
            parts.entry(pid).or_default().push((0, b));
        }
    }
    let mut blobs: Vec<(Option<i64>, Vec<u8>)> = parts
        .into_iter()
        .map(|(pid, mut chunks)| {
            chunks.sort_by_key(|c| c.0);
            (Some(pid), chunks.into_iter().flat_map(|c| c.1).collect())
        })
        .collect();
    // Without cached partitions: each import's prototype, placed by the import's transform.
    let mut placed: Vec<Option<(Vec<u8>, i64)>> = vec![None; blobs.len()];
    let parts_empty = blobs.is_empty();
    if parts_empty && has_table(c, "HistoryImportedPrototypes") {
        let mut q = c.prepare(
            "SELECT p.BodyData, b.Transform, b.ImportedBodyID FROM HistoryImportedBodies b JOIN HistoryImportedPrototypes p ON p.ImportedPrototypeID = b.ImportedPrototypeID \
             ORDER BY b.ImportedBodyID",
        )?;
        for (data, t, id) in q.query_map([], |r| Ok((blob(r.get(0)?), blob(r.get(1)?), r.get::<_, i64>(2)?)))?.flatten() {
            blobs.push((None, data));
            placed.push(Some((t, id)));
        }
    }
    // Bodies we kept a copy of when we wrote the file (only read while Shapr3D hasn't rebuilt it).
    let mut stashed: Vec<(i64, Vec<u8>)> = Vec::new();
    if parts_empty {
        // Files written before the rename (as Shayper3D) keep the copies in "ShayperBodies".
        let table = ["ShapeCraftBodies", "ShayperBodies"].into_iter().find(|t| {
            c.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [t], |r| r.get::<_, i64>(0)).unwrap_or(0)
                > 0
        });
        if let Some(table) = table {
            let mut q = c.prepare(&format!("SELECT HistoryTreeNodeID, Body FROM {table}"))?;
            stashed = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?))))?.flatten().take(10_000).collect();
        }
    }
    let mut out = Vec::new();
    let mut unreadable = Vec::new();
    // A solid with geometry not supported yet is left out (and reported), not the whole project.
    let mut read = |b: &[u8], partition: Option<i64>| match shapecraft_xt::parse(b).and_then(|f| shapecraft_xt::bodies(&f)) {
        Ok(bodies) => bodies,
        Err(e) => {
            unreadable.push((partition, e.to_string()));
            Vec::new()
        }
    };
    for (node, b) in stashed {
        for body in read(&b, None) {
            out.push(Body {
                partition: None,
                import: None,
                folder: Vec::new(),
                step: Some(node),
                name: body.name,
                brep: body.brep,
                color: body.color,
                hidden: false,
                mesh: body.mesh,
            });
        }
    }
    for ((partition, b), t) in blobs.into_iter().zip(placed) {
        let (t, import) = t.map_or((None, None), |(t, id)| (Some(t), Some(id)));
        let motion = t.as_deref().and_then(rigid);
        for mut body in read(&b, partition) {
            if let Some((rot, shift)) = motion {
                body.brep.transform_rigid(rot, shift);
                if let Some(m) = &mut body.mesh {
                    m.points.iter_mut().for_each(|p| *p = rot * *p + shift);
                }
            }
            out.push(Body {
                partition,
                import,
                folder: Vec::new(),
                step: None,
                name: body.name,
                brep: body.brep,
                color: body.color,
                hidden: false,
                mesh: body.mesh,
            });
        }
    }
    Ok((out, unreadable))
}

/// An import's placement (MessagePack, 16 doubles, column-major 4×4 in meters) as a rotation and a shift in
/// millimeters; None for the identity, or for anything that isn't a rigid motion.
fn rigid(t: &[u8]) -> Option<(glam::DMat3, glam::DVec3)> {
    let m: Vec<f64> = unpack(t)?.as_array()?.iter().filter_map(Value::as_f64).collect();
    rigid_matrix(&m)
}

/// A column-major 4×4 in meters as a rotation and a shift in millimeters; None for the identity or a non-rigid one.
fn rigid_matrix(m: &[f64]) -> Option<(glam::DMat3, glam::DVec3)> {
    if m.len() != 16 || m.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let rot = glam::DMat3::from_cols_array(&[m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]]);
    let shift = glam::DVec3::new(m[12], m[13], m[14]) * MM;
    let orthonormal = (rot.transpose() * rot).abs_diff_eq(glam::DMat3::IDENTITY, 1e-9) && (rot.determinant() - 1.0).abs() < 1e-9;
    let identity = rot.abs_diff_eq(glam::DMat3::IDENTITY, 1e-12) && shift.length() < 1e-9;
    (orthonormal && !identity && shift.length() < 1e9).then_some((rot, shift))
}
