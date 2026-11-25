//! Shapr3D project files (`.shapr`), for moving projects between Shapr3D and ShapeCraft.
//!
//! Everything here was learned from files we made ourselves in Shapr3D (see `plan/shapr-format.md`, kept out
//! of the repository with the samples): a `.shapr` is a ZIP holding `.metadata` (JSON) and `workspace`, a
//! SQLite database. The model's current state is a Parasolid partition (`BodyRevisionBlocks`), the
//! parametric history is MessagePack (`HistoryTreeNodes`), imported solids are Parasolid parts
//! (`HistoryImportedPrototypes`). Shapr3D rebuilds the model from the history when the cached partition,
//! deltas and calls are missing, which is how edits are written back.

mod json_nodes;
mod legacy;
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
        // Projects from the first releases (to about 2020) are the workspace database itself, not zipped.
        if bytes.starts_with(b"SQLite format 3\0") {
            return Ok(Self { metadata: serde_json::Value::Null, workspace: bytes.to_vec() });
        }
        if bytes.starts_with(b"<") {
            return Err(Error::Format("this is a web page, not a project (the download may have needed a sign-in)".into()));
        }
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
    /// The assigned Items-list name identifier, when known.
    pub item_name_id: Option<i64>,
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
    /// A folder's item reference (a metadata id, as `NamedChildren` writes it) → the item's `HistoryNames` id.
    pub item_by_meta: std::collections::BTreeMap<String, i64>,
    /// Items (named things in the Items list) with the folder path they sit in.
    pub items: Vec<Item>,
    pub sketches: Vec<Sketch>,
    /// Extrude steps simple enough to redo here: one sketch profile pulled a distance as a new body.
    pub extrudes: Vec<ExtrudeStep>,
    /// Solids that couldn't be read (geometry not supported yet), as messages; the rest of the project still is.
    pub unreadable: Vec<String>,
    /// Our own parametric history, kept in files we write ("ShapeCraftHistory"; Shapr3D ignores the table), as JSON.
    pub own_history: Option<String>,
    /// An older .shapr project with no history (no steps; solids, sketches and construction as they are). It
    /// can't take edits back into its own file: exports of it are new projects.
    pub legacy: bool,
    /// Whether edits can be written back into this file itself (the revision tables .shapr files have kept since about
    /// mid-2026). Older files are saved as a new project instead.
    pub patchable: bool,
    /// Sketch id → the name id folders list it by (older projects only).
    pub sketch_items: BTreeMap<i64, i64>,
    /// Construction planes and axes kept as their own rows (older projects only; later ones keep them as bodies).
    pub construction: Vec<Construction>,
}

/// A construction plane or axis of an older project, in millimeters.
#[derive(Clone, Debug, PartialEq)]
pub enum Construction {
    Plane {
        name: String,
        hidden: bool,
        origin: [f64; 3],
        normal: [f64; 3],
        x: [f64; 3],
        size: f64,
        item_name_id: i64,
    },
    /// From `start` along `vector` (its length is the axis's).
    Axis {
        name: String,
        hidden: bool,
        start: [f64; 3],
        vector: [f64; 3],
        item_name_id: i64,
    },
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
    /// The file's own id for it, as text: older projects (to about 2023) use text ids, which folders list.
    pub key: String,
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
    /// The imported body the item names, through the file's own chain, when it has one.
    pub import_body: Option<i64>,
    pub name: String,
    /// Folder names from the top ("box.step"), not counting the root.
    pub folder: Vec<String>,
    /// Hidden in the Items list (itself, or a folder it sits in).
    pub hidden: bool,
    /// Its material's base colour (0–1), when it isn't Shapr3D's default grey.
    pub color: Option<[f32; 3]>,
}

/// A column's value as it lies in the file. SQLite lets a TEXT cell hold any bytes (the 2024 releases keep binary
/// step results in one), and reading such a cell as a string would fail, so text and blobs both come out as bytes.
pub(crate) enum Cell {
    Bytes(Vec<u8>),
    Integer(i64),
    Real(f64),
    Null,
}

impl rusqlite::types::FromSql for Cell {
    fn column_result(v: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        use rusqlite::types::ValueRef;
        Ok(match v {
            ValueRef::Text(b) | ValueRef::Blob(b) => Cell::Bytes(b.to_vec()),
            ValueRef::Integer(i) => Cell::Integer(i),
            ValueRef::Real(r) => Cell::Real(r),
            ValueRef::Null => Cell::Null,
        })
    }
}

pub(crate) fn text(v: Cell) -> String {
    match v {
        Cell::Bytes(b) => String::from_utf8(b).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
        Cell::Integer(i) => i.to_string(),
        Cell::Real(r) => r.to_string(),
        Cell::Null => String::new(),
    }
}

pub(crate) fn blob(v: Cell) -> Vec<u8> {
    match v {
        Cell::Bytes(b) => b,
        _ => Vec::new(),
    }
}

/// Whether the file keeps its steps' cached results: `PersistedCalls`, or `ModelerEffects` in releases of 2024.
fn has_calls(c: &rusqlite::Connection) -> bool {
    has_table(c, "PersistedCalls") || has_table(c, "ModelerEffects")
}

/// Every cached step result as (key, data). A key is `[step node, [call index]]`. `ModelerEffects` rows hold what the
/// releases up to early 2025 keep in the third slot of a `PersistedCalls` row, so they are handed on in that form:
/// `[0, [0], [true, effects]]`.
fn call_rows(c: &rusqlite::Connection) -> Result<Vec<(Value, Value)>> {
    let (sql, wrap) = if has_table(c, "PersistedCalls") {
        ("SELECT CallKey, CallData FROM PersistedCalls", false)
    } else if has_table(c, "ModelerEffects") {
        ("SELECT CallKey, Effects FROM ModelerEffects", true)
    } else {
        return Ok(Vec::new());
    };
    let mut q = c.prepare(sql)?;
    let rows = q.query_map([], |r| Ok((blob(r.get(0)?), blob(r.get(1)?))))?;
    let mut out = Vec::new();
    for (k, d) in rows.flatten().take(10_000_000) {
        let (Some(k), Some(d)) = (unpack(&k), unpack(&d)) else { continue };
        let d = if wrap {
            Value::Array(vec![Value::from(0), Value::Array(vec![Value::from(0)]), Value::Array(vec![Value::from(true), d])])
        } else {
            d
        };
        out.push((k, current_layout(d)));
    }
    Ok(out)
}

/// A cached step result in the current layout. Releases up to early 2025 keep `[0, [0], [true, [partitions, frames,
/// records, 0]]]`: `partitions` lists `[partition, mark]` (a mark of -1: the step used the partition up) and each
/// record is `[index into partitions, before, after, tags, names, n, groups, lineage]`. The current layout keeps the
/// same record under its partition, `[0, [0], [true, [frames, [[partition, [true, [before, after, …]]], …], _]],
/// [false], [false], [false]]`, and calls the body itself tag 0 where the older one says 2147483647.
fn current_layout(d: Value) -> Value {
    const SELF: i64 = 2_147_483_647;
    let Some(a) = d.as_array().filter(|a| a.len() == 3) else { return d };
    let inner =
        a[2].as_array().filter(|x| x.first().and_then(Value::as_bool) == Some(true)).and_then(|x| x.get(1)).and_then(Value::as_array);
    let Some(inner) = inner else { return d };
    let (Some(parts), Some(records)) = (inner.first().and_then(Value::as_array), inner.get(2).and_then(Value::as_array)) else { return d };
    fn own(v: &Value, depth: usize) -> Value {
        match v {
            Value::Integer(i) if i.as_i64() == Some(SELF) => Value::from(0),
            Value::Array(a) if depth < 8 => Value::Array(a.iter().map(|x| own(x, depth + 1)).collect()),
            other => other.clone(),
        }
    }
    let pid = |i: usize| parts.get(i).and_then(Value::as_array).and_then(|p| p.first()).and_then(Value::as_i64);
    let no = || Value::Array(vec![Value::from(false)]);
    let mut seen = std::collections::HashSet::new();
    let mut changed = Vec::new();
    for r in records.iter().take(1_000_000) {
        let Some(r) = r.as_array() else { continue };
        let Some(pid) = r.first().and_then(Value::as_u64).and_then(|i| pid(i as usize)) else { continue };
        seen.insert(pid);
        let detail: Vec<Value> = r.iter().skip(1).enumerate().map(|(k, v)| if k >= 2 { own(v, 0) } else { v.clone() }).collect();
        changed.push(Value::Array(vec![Value::from(pid), Value::Array(vec![Value::from(true), Value::Array(detail)])]));
    }
    // A partition with no record of its own: used up by the step (mark -1), or one it read and left as it was (a
    // pattern's or a copy's source), which the current layout lists as unchanged.
    for p in parts.iter().filter_map(Value::as_array) {
        let (Some(pid), mark) = (p.first().and_then(Value::as_i64), p.get(1).and_then(Value::as_i64)) else { continue };
        if !seen.insert(pid) {
            continue;
        }
        if mark == Some(-1) {
            changed.push(Value::Array(vec![Value::from(pid), Value::Array(vec![Value::from(true), Value::Array(vec![no(), no()])])]));
        } else {
            changed.push(Value::Array(vec![Value::from(pid), no()]));
        }
    }
    let frames = inner.get(1).cloned().unwrap_or_else(|| Value::Array(Vec::new()));
    let result = Value::Array(vec![Value::from(true), Value::Array(vec![frames, Value::Array(changed), Value::Array(Vec::new())])]);
    Value::Array(vec![a[0].clone(), a[1].clone(), result, no(), no(), no()])
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

fn history_steps(c: &rusqlite::Connection) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    let mut nodes: BTreeMap<i64, (i64, Value)> = BTreeMap::new();
    {
        let mut q = c.prepare("SELECT HistoryTreeNodeID, HistoryTreeNodeType, Properties FROM HistoryTreeNodes")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, blob(r.get(2)?))))?;
        for (id, ty, props) in rows.flatten() {
            // Releases of 2024 keep each node as JSON; a step's parameter values are opaque there.
            if props.first() == Some(&b'{') {
                if let Some(v) = json_nodes::node(ty, &props) {
                    nodes.insert(id, (ty, v));
                }
            } else if let Some(v) = unpack(&props) {
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
        steps.push(Step { node: id, name: s(2), op: s(3), params });
    }
    Ok(steps)
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
        p.patchable = has_table(c, "BodyRevisionBlocks") && has_table(c, "PersistedCalls") && has_table(c, "HistoryTreeNodes");
        if legacy::is_legacy(c) {
            legacy::read(c, &mut p)?;
            return Ok(p);
        }
        p.steps = history_steps(c)?;
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
        p.items = items(c, &p.folders, &mut p.item_by_meta)?;
        p.sketches = sketches(c)?;
        p.extrudes = extrudes(c, &p.steps, &p.items)?;
        let unreadable;
        let paint = FacePaint::read(c, &p.steps)?;
        (p.bodies, unreadable) = current_bodies(c, &paint)?;
        let gone = drop_removed(c, &p.steps, &mut Vec::new())?;
        // Imported bodies the file keeps no revision data for (it keeps revision blocks only for bodies that were
        // rebuilt): they come from their prototypes, beside the cached ones, so the project opens complete.
        let (extra_bodies, extra_unreadable) = uncovered_import_bodies(c, &p.steps, &gone, &paint)?;
        p.bodies.extend(extra_bodies);
        name_bodies(c, &p.steps, &p.items, &p.folders, &gone, &mut p.bodies)?;
        kept_items(c, &mut p.bodies)?;
        // A copy shares another body's partition: there is none of its own to write changes back into.
        if place_bodies(c, &p.steps, &p.items, &mut p.bodies)? {
            p.patchable = false;
        }
        p.bodies.retain(|b| b.partition.is_none_or(|id| !gone.contains(&id)));
        // Only bodies still in the model count as lost: a deleted body's partition stays in the file.
        p.unreadable = unreadable
            .into_iter()
            .filter(|(pid, _)| pid.is_none_or(|pid| !gone.contains(&pid)))
            .map(|(_, e)| e)
            .chain(extra_unreadable.into_iter().map(|(_, e)| e))
            .collect();
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

pub(crate) const MM: f64 = 1000.0;

/// Our own table in files we write: copies of Shapr3D-made bodies, for reading the file again before Shapr3D has
/// rebuilt its solids.
pub(crate) const HISTORY_SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS \"ShapeCraftHistory\" (\"Version\" INTEGER NOT NULL, \"Data\" BLOB NOT NULL);";

pub(crate) const STASH_SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS \"ShapeCraftBodies\" (\"HistoryTreeNodeID\" INTEGER NOT NULL, \"Body\" BLOB NOT NULL);";

/// Imported bodies a later step moved or rebuilt, by item: their prototypes alone don't give them back.
pub(crate) const ITEMS_SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS \"ShapeCraftItems\" (\"ItemNameID\" INTEGER NOT NULL, \"Body\" BLOB NOT NULL);";

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
pub(crate) fn sketches(c: &rusqlite::Connection) -> Result<Vec<Sketch>> {
    let mut out = Vec::new();
    let mut q = c.prepare(
        "SELECT SketchID, Name, IsHidden, PlaneCenterX, PlaneCenterY, PlaneCenterZ, PlaneNormX, PlaneNormY, PlaneNormZ, PlaneUDirX, PlaneUDirY, PlaneUDirZ, rowid \
         FROM SketchControllers ORDER BY OrderInList, SketchID",
    )?;
    let mut keys: Vec<rusqlite::types::Value> = Vec::new();
    let rows = q.query_map([], |r| {
        let f = |i: usize| r.get::<_, Option<f64>>(i).map(|v| v.unwrap_or(0.0));
        let raw: rusqlite::types::Value = r.get(0)?;
        // Text ids (older projects) are numbered by their row, far above the numeric ids later files use.
        let id = match &raw {
            rusqlite::types::Value::Integer(n) => *n,
            _ => (1 << 40) + r.get::<_, i64>(12)?,
        };
        let key = match &raw {
            rusqlite::types::Value::Integer(n) => n.to_string(),
            rusqlite::types::Value::Text(t) => t.clone(),
            rusqlite::types::Value::Blob(b) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        };
        Ok((
            raw,
            Sketch {
                id,
                key,
                name: text(r.get(1)?),
                hidden: r.get::<_, Option<bool>>(2)?.unwrap_or(false),
                origin: [f(3)? * MM, f(4)? * MM, f(5)? * MM],
                normal: [f(6)?, f(7)?, f(8)?],
                u: [f(9)?, f(10)?, f(11)?],
                curves: Vec::new(),
            },
        ))
    })?;
    for (raw, sk) in rows.flatten().take(100_000) {
        keys.push(raw);
        out.push(sk);
    }
    let finite = |v: &serde_json::Value| v.as_f64().filter(|x| x.is_finite() && x.abs() < 1e6);
    for (sk, raw) in out.iter_mut().zip(&keys) {
        let construction: Vec<i64> = c
            .query_row("SELECT ConstructionCurves FROM ConstructionSketches WHERE SketchID = ?1", [sk.id], |r| Ok(text(r.get(0)?)))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|j| j["elements"].as_array().map(|a| a.iter().filter_map(|e| e.as_i64().or_else(|| e["id"].as_i64())).collect()))
            .unwrap_or_default();
        if !has_column(c, "SketchCurves", "Data") {
            sk.curves = typed_curves(c, raw)?;
            continue;
        }
        let mut q = c.prepare("SELECT CurveID, Data, rowid FROM SketchCurves WHERE SketchID = ?1 ORDER BY CurveID")?;
        let rows = q.query_map([raw], |r| {
            let id = match r.get::<_, rusqlite::types::Value>(0)? {
                rusqlite::types::Value::Integer(n) => n,
                _ => (1 << 40) + r.get::<_, i64>(2)?,
            };
            Ok((id, text(r.get(1)?)))
        })?;
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

/// Whether a table has a column: the schema grew column by column.
pub(crate) fn has_column(c: &rusqlite::Connection, table: &str, name: &str) -> bool {
    c.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .and_then(|mut q| q.query_map([], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().any(|n| n == name)))
        .unwrap_or(false)
}

/// A sketch's curves in the first releases' layout (2017-2018): one table per kind (`SketchLines`, `SketchArcs`,
/// `SketchCircles`, `SketchSplines`), each naming its points in `ConstraintPoints` (plane coordinates in meters).
fn typed_curves(c: &rusqlite::Connection, sketch: &rusqlite::types::Value) -> Result<Vec<SketchCurve>> {
    let mut out = Vec::new();
    if !has_table(c, "ConstraintPoints") {
        return Ok(out);
    }
    let mut points: BTreeMap<Vec<u8>, [f64; 2]> = BTreeMap::new();
    {
        let mut q = c.prepare("SELECT PointID, XPosition, YPosition FROM ConstraintPoints WHERE SketchID = ?1")?;
        let rows = q.query_map([sketch], |r| Ok((blob(r.get(0)?), r.get::<_, Option<f64>>(1)?, r.get::<_, Option<f64>>(2)?)))?;
        for (id, x, y) in rows.flatten().take(1_000_000) {
            if let (Some(x), Some(y)) = (x, y)
                && x.is_finite()
                && y.is_finite()
                && x.abs() < 1e3
                && y.abs() < 1e3
            {
                points.insert(id, [x * MM, y * MM]);
            }
        }
    }
    let at = |id: &Vec<u8>| points.get(id).copied();
    // Each kind's rows for this sketch: curve row id and its point ids.
    let rows = |table: &str, id: &str, cols: &[&str]| -> Result<Vec<(i64, Vec<Vec<u8>>)>> {
        if !has_table(c, table) {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT s.rowid, {} FROM {table} t JOIN SketchCurves s ON s.CurveID = t.{id} WHERE s.SketchID = ?1 ORDER BY s.rowid",
            cols.iter().map(|k| format!("t.{k}")).collect::<Vec<_>>().join(", ")
        );
        let mut q = c.prepare(&sql)?;
        let n = cols.len();
        let found = q.query_map([sketch], |r| {
            Ok((r.get::<_, i64>(0)?, (1..=n).map(|i| r.get::<_, Cell>(i).map(blob)).collect::<rusqlite::Result<Vec<_>>>()?))
        })?;
        Ok(found.flatten().take(100_000).collect())
    };
    let id = |row: i64| (1_i64 << 40) + row;
    for (row, p) in rows("SketchLines", "LineID", &["StartPointID", "EndPointID"])? {
        if let (Some(a), Some(b)) = (at(&p[0]), at(&p[1])) {
            out.push(SketchCurve::Line { id: id(row), a, b, construction: false });
        }
    }
    for (row, p) in rows("SketchArcs", "ArcID", &["StartPointID", "MidPointID", "EndPointID", "CenterPointID"])? {
        let (Some(start), Some(mid), Some(end), Some(center)) = (at(&p[0]), at(&p[1]), at(&p[2]), at(&p[3])) else { continue };
        // Stored counter-clockwise from start to end when the middle point lies on that side.
        let angle = |q: [f64; 2]| (q[1] - center[1]).atan2(q[0] - center[0]);
        let ccw = |from: f64, to: f64| (to - from).rem_euclid(std::f64::consts::TAU);
        let (a0, am, a1) = (angle(start), angle(mid), angle(end));
        let (start, end) = if ccw(a0, am) <= ccw(a0, a1) { (start, end) } else { (end, start) };
        out.push(SketchCurve::Arc { id: id(row), center, start, end, construction: false });
    }
    for (row, p) in rows("SketchCircles", "CircleID", &["CenterPointID", "StartPointID"])? {
        if let (Some(center), Some(on)) = (at(&p[0]), at(&p[1])) {
            let radius = (on[0] - center[0]).hypot(on[1] - center[1]);
            if radius > 1e-9 {
                out.push(SketchCurve::Circle { id: id(row), center, radius, construction: false });
            }
        }
    }
    if has_table(c, "SketchSplinePoints") {
        for (row, p) in rows("SketchSplines", "SplineID", &["SplineID"])? {
            let mut q = c.prepare("SELECT PointID FROM SketchSplinePoints WHERE SplineID = ?1 ORDER BY PointIndexInSpline")?;
            let ids = q.query_map([&p[0]], |r| r.get::<_, Cell>(0).map(blob))?;
            let pts: Vec<[f64; 2]> = ids.flatten().take(10_000).filter_map(|i| at(&i)).collect();
            if pts.len() >= 2 {
                out.push(SketchCurve::Spline { id: id(row), points: pts, poles: Vec::new(), construction: false });
            }
        }
    }
    Ok(out)
}

/// Items: a name (metadata type 0) assigned to a `HistoryNames` id; folders list an item by the id of another
/// metadata row (type 3) assigned to the same name id.
fn items(c: &rusqlite::Connection, folders: &[Folder], item_by_meta: &mut std::collections::BTreeMap<String, i64>) -> Result<Vec<Item>> {
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
    // A rename adds a name row on the anchor of the old one, and a copy of a body adds a
    // nameless row beside the named one: the item is the anchor, its display the newest name
    // on it, and its folder the tree entry of any of its rows.
    let mut by_anchor: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for name_id in by_meta.keys() {
        let anchor = names.get(name_id).and_then(|d| d["nameToAnchor"].as_i64()).unwrap_or(*name_id);
        by_anchor.entry(anchor).or_default().push(*name_id);
    }
    let mut out = Vec::new();
    for rows in by_anchor.values() {
        let metas: Vec<(i64, i64, String)> = rows.iter().flat_map(|r| by_meta.get(r).cloned().unwrap_or_default()).collect();
        for m in &metas {
            item_by_meta.insert(m.0.to_string(), rows[0]);
        }
        // The item speaks with its newest name row: the one the newest display metadata hangs off.
        let Some(display_row) = metas
            .iter()
            .filter(|m| m.1 == 0)
            .max_by_key(|m| m.0)
            .and_then(|newest| rows.iter().copied().find(|r| by_meta.get(r).is_some_and(|ms| ms.iter().any(|m| m.0 == newest.0))))
        else {
            continue;
        };
        let Some(name) = metas.iter().filter(|m| m.1 == 0).max_by_key(|m| m.0).map(|m| m.2.clone()) else { continue };
        let own_hidden = metas.iter().filter(|m| m.1 == 1).max_by_key(|m| m.0).is_some_and(|m| m.2.trim() == "1");
        let color = wrapped(display_row).and_then(|w| materials.get(&w)).and_then(|m| colour(m.1));
        let (folder, hidden) = folders
            .iter()
            .find(|f| f.children.iter().any(|(id, ty)| *ty == 6 && metas.iter().any(|m| m.0.to_string() == *id)))
            .and_then(|f| paths.get(&f.id).cloned())
            .unwrap_or_default();
        out.push(Item { name_id: display_row, import_body: None, name, folder, hidden: hidden || own_hidden, color });
    }
    Ok(out)
}

/// What a call does to whole bodies, each by its tag.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    /// Placed by this matrix of the call's, on top of where it was.
    Move(i64, usize),
    /// A new body that shows another's partition, where that one is.
    Copy(i64, i64),
    Remove(i64),
}

/// A call's placements (slot 3): `[true, [matrices, [[tag, [false] or [true, the tag copied], [false] or [true, index
/// of its matrix]], …]]]`. A body neither copied nor placed is removed.
fn placements_of(call: &[Value]) -> (Vec<Vec<f64>>, Vec<Change>) {
    let inner = call
        .get(3)
        .and_then(Value::as_array)
        .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
        .and_then(|x| x.get(1))
        .and_then(Value::as_array);
    let Some(inner) = inner else { return Default::default() };
    let matrices = inner
        .first()
        .and_then(Value::as_array)
        .map(|m| m.iter().map(|m| m.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default()).collect())
        .unwrap_or_default();
    let given = |v: Option<&Value>| -> Option<i64> {
        v.and_then(Value::as_array)
            .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
            .and_then(|x| x.get(1))
            .and_then(Value::as_i64)
    };
    let mut changes = Vec::new();
    for e in inner.get(1).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_array) {
        let Some(tag) = e.first().and_then(Value::as_i64) else { continue };
        let (copied, placed) = (given(e.get(1)), given(e.get(2)));
        changes.extend(copied.map(|from| Change::Copy(tag, from)));
        changes.extend(placed.and_then(|i| usize::try_from(i).ok()).map(|i| Change::Move(tag, i)));
        if copied.is_none() && placed.is_none() {
            changes.push(Change::Remove(tag));
        }
    }
    (matrices, changes)
}

/// Bodies a later step removed keep their partition; the cached call results say which. A call's data is
/// `[0, [0], modified, removed, _, created]`: `created` lists `[index, partition, 3, tag]`; `removed` (Delete) lists
/// `[tag, …]`; `modified` lists `[partition, [true, [[true, pmark before], after, …]]]` where `after` is `[false]` when
/// the step used the body up (the inputs of a Subtract or Union).
fn drop_removed(c: &rusqlite::Connection, steps: &[Step], bodies: &mut Vec<Body>) -> Result<std::collections::HashSet<i64>> {
    // Files from before about 2025 keep no cached calls.
    if !has_calls(c) {
        return Ok(std::collections::HashSet::new());
    }
    // A step may have several calls (`[node, [0]]`, `[node, [1]]`, …), kept in that order.
    let mut calls: BTreeMap<i64, BTreeMap<i64, Value>> = BTreeMap::new();
    {
        for (k, d) in call_rows(c)? {
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
        for change in placements_of(a).1 {
            match change {
                Change::Copy(tag, from) => {
                    if let Some(pid) = tag_of.get(&from).copied() {
                        tag_of.insert(tag, pid);
                    }
                }
                // Its partition goes with the last body that shows it.
                Change::Remove(tag) => {
                    if let Some(pid) = tag_of.remove(&tag)
                        && !tag_of.values().any(|p| *p == pid)
                    {
                        gone.insert(pid);
                    }
                }
                Change::Move(..) => {}
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
fn name_bodies(
    c: &rusqlite::Connection,
    steps: &[Step],
    items: &[Item],
    folders: &[Folder],
    gone: &std::collections::HashSet<i64>,
    bodies: &mut [Body],
) -> Result<()> {
    let calls = call_partitions(c, bodies)?;
    name_from_calls(c, steps, items, folders, gone, bodies, &calls)
}

/// Step node → the cached partitions its call made or changed.
fn call_partitions(c: &rusqlite::Connection, bodies: &[Body]) -> Result<BTreeMap<i64, Vec<i64>>> {
    // Files from before about 2025 keep no cached calls.
    if !has_calls(c) {
        return Ok(BTreeMap::new());
    }
    let known: Vec<i64> = bodies.iter().filter_map(|b| b.partition).collect();
    let mut calls: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    {
        for (k, d) in call_rows(c)? {
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
                // Sorted by their index field, so the k-th partition lines up with the k-th name anchor even when
                // the file lists them out of creation order.
                let mut indexed: Vec<(i64, i64)> = list(5)
                    .iter()
                    .flat_map(|g| g.as_array().cloned().unwrap_or_default())
                    .filter_map(|c| {
                        let c = c.as_array()?;
                        Some((c.first()?.as_i64()?, c.get(1)?.as_i64()?))
                    })
                    .collect();
                indexed.sort_unstable_by_key(|&(idx, _)| idx);
                found.extend(indexed.into_iter().map(|(_, pid)| pid));
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

/// One body as the steps left it: the partition it shows, and where.
#[derive(Clone, Debug)]
struct Instance {
    partition: i64,
    motion: glam::DMat4,
    /// The step that copied it, for a body that shows another's partition.
    copied_by: Option<i64>,
}

/// Every step's cached calls in order: (step, call).
fn ordered_calls<'a>(c: &rusqlite::Connection, steps: &'a [Step]) -> Result<Vec<(&'a Step, Value)>> {
    let mut calls: BTreeMap<i64, Vec<(Vec<i64>, Value)>> = BTreeMap::new();
    for (k, d) in call_rows(c)? {
        let key = k.as_array();
        if let Some(node) = key.and_then(|a| a.first()).and_then(Value::as_i64) {
            let sub = key.and_then(|a| a.get(1)).and_then(Value::as_array);
            let sub = sub.map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
            calls.entry(node).or_default().push((sub, d));
        }
    }
    let mut out = Vec::new();
    for step in steps {
        let mut mine = calls.remove(&step.node).unwrap_or_default();
        mine.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(mine.into_iter().map(|(_, d)| (step, d)));
    }
    Ok(out)
}

/// A column-major 4×4 in meters as a motion in millimeters.
fn motion(m: &[f64]) -> Option<glam::DMat4> {
    let (rot, shift) = rigid_matrix(m)?;
    Some(glam::DMat4::from_mat3_translation(rot, shift))
}

/// Steps whose call may list a partition for the first time because they made a body.
const MAKES_BODIES: &[&str] = &["MaterializeImportedBodies", "Extrude", "Revolve", "Loft", "Sweep"];

/// The bodies by tag, followed through the steps (#45). A step that rebuilds a body's partition bakes its placement in.
/// `known` are the partitions there is a body for: where the file names no tag for one, the tag is the partition.
fn instances(calls: &[(&Step, Value)], known: &std::collections::HashSet<i64>) -> BTreeMap<i64, Instance> {
    let mut all: BTreeMap<i64, Instance> =
        known.iter().map(|p| (*p, Instance { partition: *p, motion: glam::DMat4::IDENTITY, copied_by: None })).collect();
    let mut seen = std::collections::HashSet::new();
    for (step, call) in calls {
        let Some(a) = call.as_array() else { continue };
        let list = |slot: usize| {
            a.get(slot)
                .and_then(Value::as_array)
                .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
                .and_then(|x| x.get(1))
                .and_then(Value::as_array)
        };
        for e in list(5).into_iter().flatten().filter_map(Value::as_array).flatten().filter_map(Value::as_array) {
            let (Some(pid), Some(tag)) = (e.get(1).and_then(Value::as_i64), e.get(3).and_then(Value::as_i64)) else { continue };
            // Named by its tag from here on.
            if tag != pid && all.get(&pid).is_some_and(|i| i.partition == pid && i.copied_by.is_none()) {
                all.remove(&pid);
            }
            seen.insert(pid);
            all.insert(tag, Instance { partition: pid, motion: glam::DMat4::IDENTITY, copied_by: None });
        }
        for e in list(2).and_then(|l| l.get(1)).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_array) {
            let Some(pid) = e.first().and_then(Value::as_i64) else { continue };
            for i in all.values_mut().filter(|i| i.partition == pid) {
                i.motion = glam::DMat4::IDENTITY;
            }
            // A partition nothing made: a step that changed or used up a copy gave it one of its own, and the file doesn't
            // say whose it was. The copies so far may be it.
            let used_up = e.get(1).and_then(|x| x.as_array()?.get(1)?.as_array()?.get(1)?.as_array()?.first()?.as_bool()) == Some(false);
            if seen.insert(pid) && (used_up || !MAKES_BODIES.contains(&step.op.as_str())) {
                all.retain(|_, i| i.copied_by.is_none());
            }
        }
        let (matrices, changes) = placements_of(a);
        for change in changes {
            match change {
                Change::Copy(tag, from) => {
                    if let Some(source) = all.get(&from).cloned() {
                        all.insert(tag, Instance { copied_by: Some(step.node), ..source });
                    }
                }
                Change::Move(tag, i) => {
                    if let (Some(body), Some(m)) = (all.get_mut(&tag), matrices.get(i).and_then(|m| motion(m))) {
                        body.motion = m * body.motion;
                    }
                }
                Change::Remove(tag) => {
                    all.remove(&tag);
                }
            }
        }
    }
    all
}

/// The items of imported bodies that a later step moved or rebuilt.
pub(crate) fn changed_imports(c: &rusqlite::Connection) -> Result<std::collections::HashSet<i64>> {
    let mut out = std::collections::HashSet::new();
    let has = ["HistoryNames", "HistoryNameAnchoringPoints", "MetadataAssignments"].iter().all(|t| has_table(c, t));
    if !has || !has_calls(c) {
        return Ok(out);
    }
    let steps = history_steps(c)?;
    let slots = import_slots(c, &steps)?;
    let calls = ordered_calls(c, &steps)?;
    let placed = instances(&calls, &slots.iter().filter_map(|(_, pid)| *pid).collect());
    let mut rebuilt = std::collections::HashSet::new();
    for (_, call) in calls.iter().filter(|(s, _)| s.op != "MaterializeImportedBodies") {
        let changed = call.as_array().and_then(|a| a.get(2)?.as_array()?.get(1)?.as_array()?.get(1)?.as_array());
        rebuilt.extend(changed.into_iter().flatten().filter_map(|e| e.as_array()?.first()?.as_i64()));
    }
    let mut bodies: Vec<Body> = slots
        .iter()
        .filter(|(_, pid)| {
            pid.is_some_and(|pid| {
                let moved = |i: &Instance| !i.motion.abs_diff_eq(glam::DMat4::IDENTITY, 1e-12);
                rebuilt.contains(&pid) || placed.values().any(|i| i.partition == pid && i.copied_by.is_none() && moved(i))
            })
        })
        .map(|(import, _)| Body {
            partition: None,
            import: Some(*import),
            folder: Vec::new(),
            step: None,
            name: None,
            item_name_id: None,
            brep: Default::default(),
            color: None,
            hidden: false,
            mesh: None,
        })
        .collect();
    let items = items(c, &[], &mut Default::default())?;
    name_bodies(c, &steps, &items, &[], &Default::default(), &mut bodies)?;
    out.extend(bodies.iter().filter_map(|b| b.item_name_id));
    Ok(out)
}

/// In a file we wrote that nothing has rebuilt since: the imported bodies that aren't just their prototypes any more.
fn kept_items(c: &rusqlite::Connection, bodies: &mut [Body]) -> Result<()> {
    if bodies.iter().any(|b| b.partition.is_some()) || !has_table(c, "ShapeCraftItems") {
        return Ok(());
    }
    let mut q = c.prepare("SELECT ItemNameID, Body FROM ShapeCraftItems")?;
    let rows: Vec<(i64, Vec<u8>)> = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?))))?.flatten().take(100_000).collect();
    for (item, data) in rows {
        let Some(b) = bodies.iter_mut().find(|b| b.item_name_id == Some(item) && b.import.is_some()) else { continue };
        if let Ok(read) = shapecraft_xt::parse(&data).and_then(|f| shapecraft_xt::bodies(&f))
            && let [body] = read.as_slice()
        {
            b.brep = body.brep.clone();
            b.mesh = None;
        }
    }
    Ok(())
}

/// Puts each body where the steps left it: a move that is a body's last change is left out of its cached partition and
/// kept in the call instead (#45). Copies a step made that way are bodies of their own; returns whether there are any.
fn place_bodies(c: &rusqlite::Connection, steps: &[Step], items: &[Item], bodies: &mut Vec<Body>) -> Result<bool> {
    // Files from before about 2025 keep no cached calls.
    if !has_calls(c) {
        return Ok(false);
    }
    let calls = ordered_calls(c, steps)?;
    // A body that came from its import's prototype stands for the partition the import step would have cached.
    let slots = import_slots(c, steps)?;
    let slot_of = |b: &Body| b.partition.or_else(|| slots.iter().find(|(id, _)| Some(*id) == b.import).and_then(|(_, pid)| *pid));
    let known: std::collections::HashSet<i64> = bodies.iter().filter_map(slot_of).collect();
    let placed = instances(&calls, &known);
    // Tags the file doesn't explain: those moves go by the step that made the body.
    let explained: std::collections::HashSet<i64> = placed.keys().copied().collect();
    let mut unexplained = Vec::new();
    for (step, call) in &calls {
        if step.op != "Transform" {
            continue;
        }
        let Some(a) = call.as_array() else { continue };
        let (matrices, changes) = placements_of(a);
        let moves: Vec<(i64, usize)> =
            changes.iter().filter_map(|c| if let Change::Move(t, i) = c { Some((*t, *i)) } else { None }).collect();
        let copies = changes.iter().any(|c| matches!(c, Change::Copy(..)));
        if !copies
            && !moves.is_empty()
            && moves.iter().all(|(t, _)| !explained.contains(t))
            && let Some(m) = moves.first().and_then(|(_, i)| matrices.get(*i)).and_then(|m| rigid_matrix(m))
        {
            unexplained.push((step.node, m));
        }
    }
    // The copies first, from the shapes as the file has them: a copy's own placement starts from its source's.
    let mut copies = Vec::new();
    for i in placed.values() {
        if let Some(step) = i.copied_by
            && let Some(source) = bodies.iter().find(|b| slot_of(b) == Some(i.partition))
        {
            let mut b = source.clone();
            (b.partition, b.import, b.step, b.name, b.item_name_id, b.hidden) = (None, None, Some(step), None, None, false);
            copies.push((step, b, i.motion));
        }
    }
    // A body that was copied and then deleted leaves its partition to the copy.
    let own = |p: i64| placed.values().any(|i| i.partition == p && i.copied_by.is_none());
    let copied = |p: i64| placed.values().any(|i| i.partition == p && i.copied_by.is_some());
    bodies.retain(|b| slot_of(b).is_none_or(|p| own(p) || !copied(p)));
    for i in placed.values().filter(|i| i.copied_by.is_none() && !i.motion.abs_diff_eq(glam::DMat4::IDENTITY, 1e-12)) {
        for b in bodies.iter_mut().filter(|b| slot_of(b) == Some(i.partition)) {
            put(b, i.motion);
        }
    }
    apply_moves(c, steps, bodies, &unexplained)?;
    let any = !copies.is_empty();
    // The copy's item hangs off a name its step made.
    let mut named = names_made_by(c)?;
    for (step, mut b, m) in copies {
        put(&mut b, m);
        let item = named.get_mut(&step).and_then(|ids| {
            let at =
                ids.iter().position(|id| items.iter().any(|i| i.name_id == *id) && bodies.iter().all(|b| b.item_name_id != Some(*id)))?;
            Some(ids.remove(at))
        });
        if let Some(item) = item.and_then(|id| items.iter().find(|i| i.name_id == id)) {
            b.name = Some(item.name.clone());
            b.item_name_id = Some(item.name_id);
            b.folder = item.folder.clone();
            b.hidden = item.hidden;
            b.color = item.color.or(b.color);
        }
        bodies.push(b);
    }
    Ok(any)
}

fn put(b: &mut Body, m: glam::DMat4) {
    let (rot, shift) = (glam::DMat3::from_mat4(m), m.w_axis.truncate());
    b.brep.transform_rigid(rot, shift);
    if let Some(mesh) = &mut b.mesh {
        mesh.points.iter_mut().for_each(|p| *p = rot * *p + shift);
    }
}

/// The names each step made (`HistoryNames` 18 and 19 keyed by its call).
fn names_made_by(c: &rusqlite::Connection) -> Result<BTreeMap<i64, Vec<i64>>> {
    let mut made: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    if has_table(c, "HistoryNames") {
        let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type IN (18, 19) ORDER BY NameID")?;
        for (id, data) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?.flatten().take(1_000_000) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) else { continue };
            if let Some(node) = v["callKey"]["nodeID"].as_i64() {
                made.entry(node).or_default().push(id);
            }
        }
    }
    Ok(made)
}

/// The moves whose bodies the file doesn't name by tag, by the steps that created their targets: `moves` is (the
/// Transform, what it does). Each move with no later change to its bodies is applied.
fn apply_moves(c: &rusqlite::Connection, steps: &[Step], bodies: &mut [Body], moves: &[(i64, (glam::DMat3, glam::DVec3))]) -> Result<()> {
    if moves.is_empty() {
        return Ok(());
    }
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
    for (i, step) in steps.iter().enumerate() {
        let Some((_, motion)) = moves.iter().find(|(node, _)| *node == step.node) else { continue };
        let Some((_, target)) = step.params.first() else { continue };
        let mut refs = Vec::new();
        named(target, &mut refs);
        let targets = refs.len();
        let creators: Vec<i64> = refs.into_iter().filter_map(creator).collect();
        let later: Vec<i64> = steps[i + 1..].iter().flat_map(|s| calls.get(&s.node).cloned().unwrap_or_default()).collect();
        let hits = |b: &Body| b.step.is_some_and(|s| creators.contains(&s)) && b.partition.is_some_and(|p| !later.contains(&p));
        // An import makes many bodies and a move names a few: the step alone can't say which.
        let from_import = creators.iter().any(|n| steps.iter().any(|s| s.node == *n && s.op == "MaterializeImportedBodies"));
        if from_import && bodies.iter().filter(|b| hits(b)).count() > targets {
            continue;
        }
        for b in bodies.iter_mut().filter(|b| hits(b)) {
            put(b, glam::DMat4::from_mat3_translation(motion.0, motion.1));
        }
    }
    Ok(())
}

fn name_from_calls(
    c: &rusqlite::Connection,
    steps: &[Step],
    items: &[Item],
    folders: &[Folder],
    gone: &std::collections::HashSet<i64>,
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
    // The item each imported body is, named through the file's own chain (#66), and which imported body each
    // partition an import step made is: an import's bodies need not line up with its anchors one by one.
    let chained = imported_items(c, items)?;
    let pairs = import_slots(c, steps)?;
    let import_of: BTreeMap<i64, i64> = pairs.iter().filter_map(|(id, pid)| pid.map(|pid| (pid, *id))).collect();
    // By position only an item the chain gives no body.
    let chained_ids: std::collections::HashSet<i64> = chained.values().map(|i| i.name_id).collect();
    let mut claimed = std::collections::HashSet::new();
    let mut weak = std::collections::HashSet::new(); // partitions an inheritance bound
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
                    weak.insert(*pid);
                    item_of.insert(*pid, (*creator, item.clone()));
                    if let Some(b) = bodies.iter_mut().find(|b| b.partition == Some(*pid)) {
                        b.step = *creator;
                        if let Some(item) = item {
                            b.name = Some(item.name.clone());
                            b.item_name_id = Some(item.name_id);
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
            let item = import_of
                .get(pid)
                .and_then(|id| chained.get(id).copied())
                .or_else(|| named.get(k).copied().filter(|i| !chained_ids.contains(&i.name_id)));
            item_of.insert(*pid, (Some(step.node), item.cloned()));
            if let Some(b) = bodies.iter_mut().find(|b| b.partition == Some(*pid)) {
                b.step = Some(step.node);
                if let Some(item) = item {
                    b.name = Some(item.name.clone());
                    b.item_name_id = Some(item.name_id);
                    b.folder = item.folder.clone();
                    b.hidden = item.hidden;
                    b.color = b.color.or(item.color);
                }
            }
            k += 1;
        }
    }
    // A Move step that copies a body names the copy itself: the names its call created belong
    // to the partitions the step brought in, matched by the size they keep from the body they
    // copied.
    let mut created: BTreeMap<i64, Vec<i64>> = BTreeMap::new(); // step node → name ids
    if has_table(c, "HistoryNames") {
        let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type IN (18, 19)")?;
        for (id, data) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?.flatten().take(1_000_000) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) else { continue };
            if let Some(node) = v["callKey"]["nodeID"].as_i64() {
                created.entry(node).or_default().push(id);
            }
        }
    }
    let mut boxes: BTreeMap<i64, [f64; 3]> = BTreeMap::new();
    for b in bodies.iter().filter_map(|b| Some((b.partition?, &b.brep))) {
        let (mut lo, mut hi) = ([f64::MAX; 3], [-f64::MAX; 3]);
        for v in &b.1.vertices {
            for (k, c) in [(0usize, v.x), (1, v.y), (2, v.z)] {
                lo[k] = lo[k].min(c);
                hi[k] = hi[k].max(c);
            }
        }
        if lo[0] <= hi[0] {
            boxes.insert(b.0, [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]);
        }
    }
    let box_of = |pid: i64| boxes.get(&pid).copied();
    let near = |a: [f64; 3], b: [f64; 3]| (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.5 && (a[2] - b[2]).abs() < 0.5;
    let mut seen = std::collections::HashSet::new();
    for step in steps {
        let Some(rows) = created.get(&step.node) else { continue };
        let Some(pids) = calls.get(&step.node) else { continue };
        // The partitions the step brought in: its call's that nothing had seen before it and
        // that the import pairing doesn't know (the delta-moves of imported bodies aren't
        // copies).
        let outputs: Vec<i64> = pids.iter().copied().filter(|pid| !seen.contains(pid) && !import_of.contains_key(pid)).collect();
        for pid in &outputs {
            seen.insert(*pid);
        }
        if outputs.is_empty() {
            continue;
        }
        let mut its: Vec<&Item> = rows.iter().filter_map(|r| items.iter().find(|i| i.name_id == *r)).collect();
        if its.is_empty() {
            continue;
        }
        // One copy per name: pair them by the size of the body each name derives from (its
        // anchor's originals → the body the step copied), nearest first.
        let Ok(mut q2) = c.prepare("SELECT Type, Data FROM HistoryNames WHERE NameID = ?1") else { continue };
        // The body a copy's name derives from: its chain hops names, anchors and derivation
        // anchors (a copy of a copy of a …) until it reaches the body's own name.
        let mut source_body = |name_id: i64| -> Option<i64> {
            let mut cur = name_id;
            for _ in 0..16 {
                let (ty, data) = q2.query_row([cur], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?)))).ok()?;
                let a: serde_json::Value = serde_json::from_str(&data).ok()?;
                cur = match ty {
                    18 => a["nameToAnchor"].as_i64()?,
                    0 => a["sourceTopologyNames"].as_array()?.first()?.as_i64()?,
                    5 => a["originals"].as_array()?.first()?.as_i64()?,
                    11 => {
                        if a.get("identifier").is_some_and(|v| !v.is_null()) {
                            return None;
                        }
                        return a["bodyID"].as_i64();
                    }
                    _ => return None,
                };
            }
            None
        };
        let mut source_box = |name_id: i64| -> Option<[f64; 3]> {
            let id = source_body(name_id)?;
            let pid = pairs.iter().find(|(k, _)| *k == id)?.1?;
            box_of(pid)
        };
        let dist = |s: Option<[f64; 3]>, to: [f64; 3]| -> f64 {
            match s {
                Some(x) => (x[0] - to[0]).abs() + (x[1] - to[1]).abs() + (x[2] - to[2]).abs(),
                None => f64::INFINITY,
            }
        };
        for pid in &outputs {
            let Some(box_) = box_of(*pid) else { continue };
            let Some(best) = its.iter().min_by(|a, b| {
                let (da, db) = (source_box(a.name_id), source_box(b.name_id));
                let (near_a, near_b) = (da.is_some_and(|x| near(x, box_)), db.is_some_and(|x| near(x, box_)));
                near_b.cmp(&near_a).then(dist(da, box_).total_cmp(&dist(db, box_)))
            }) else {
                break;
            };
            let item = *best;
            its.retain(|i| i.name_id != item.name_id);
            weak.remove(pid);
            if let Some(b) = bodies.iter_mut().find(|b| b.partition == Some(*pid)) {
                b.name = Some(item.name.clone());
                b.item_name_id = Some(item.name_id);
                b.folder = item.folder.clone();
                b.hidden = item.hidden;
                b.color = b.color.or(item.color);
            }
        }
    }
    // The bodies a Delete step removed leave their items orphaned; a copy of one of them that a
    // Move step brought in takes the orphan of its own size, the last removal first.
    let mut deleted_at: BTreeMap<i64, usize> = BTreeMap::new();
    for (i, step) in steps.iter().enumerate().filter(|(_, s)| s.op == "Delete") {
        if let Some(pids) = calls.get(&step.node) {
            for pid in pids {
                deleted_at.entry(*pid).or_insert(i);
            }
        }
    }
    if has_table(c, "HistoryImportedBodies")
        && has_table(c, "HistoryImportedPrototypes")
        && !pairs.is_empty()
        && let Ok(mut q3) = c.prepare(
            "SELECT p.BodyData FROM HistoryImportedBodies b \
             JOIN HistoryImportedPrototypes p ON p.ImportedPrototypeID = b.ImportedPrototypeID WHERE b.ImportedBodyID = ?1",
        )
    {
        let mut orphans: Vec<(usize, &Item, [f64; 3])> = Vec::new();
        for (id, pid) in pairs.iter().filter(|(_, pid)| pid.is_some_and(|p| gone.contains(&p))) {
            let Some(item) = chained.get(id) else { continue };
            let Ok((_, data)) = q3.query_row([id], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?)))) else { continue };
            let Some(s) = size_of(&data) else { continue };
            let Some(order) = pid.as_ref().and_then(|p| deleted_at.get(p)).copied() else { continue };
            orphans.push((order, item, s));
        }
        orphans.sort_by_key(|(order, _, _)| std::cmp::Reverse(*order));
        let free: Vec<i64> = weak.iter().copied().filter(|pid| !import_of.contains_key(pid)).collect();
        for pid in free {
            let Some(box_) = box_of(pid) else { continue };
            let Some((_, item, _)) = orphans.iter().find(|(_, _, s)| near(*s, box_)) else { continue };
            weak.remove(&pid);
            if let Some(b) = bodies.iter_mut().find(|b| b.partition == Some(pid)) {
                b.name = Some(item.name.clone());
                b.item_name_id = Some(item.name_id);
                b.folder = item.folder.clone();
                b.hidden = item.hidden;
                b.color = b.color.or(item.color);
            }
        }
    }

    for b in bodies.iter_mut().filter(|b| b.partition.is_none() && b.import.is_none()) {
        let Some(node) = b.step else { continue };
        let named = anchors.get(&node).and_then(|ns| ns.iter().find_map(|n| items.iter().find(|i| i.name_id == *n)));
        if let Some(item) = named {
            b.name = Some(item.name.clone());
            b.item_name_id = Some(item.name_id);
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
            let item = b
                .import
                .and_then(|id| chained.get(&id).copied())
                .or_else(|| named.get(k).copied().filter(|i| !chained_ids.contains(&i.name_id)));
            if let Some(item) = item {
                b.name = Some(item.name.clone());
                b.item_name_id = Some(item.name_id);
                b.folder = item.folder.clone();
                b.hidden = item.hidden;
                b.color = b.color.or(item.color);
            }
        }
    }
    resolve_transformed_items(c, steps, items, folders, gone, bodies)?;
    name_by_topology(c, steps, items, gone, bodies)?;
    // A copy a step made without an item of its own (a pattern's copies) is listed under the body it was copied
    // from: the one named body the same step worked on.
    for pids in calls.values() {
        let named: Vec<usize> = (0..bodies.len())
            .filter(|&i| bodies[i].item_name_id.is_some() && bodies[i].partition.is_some_and(|p| pids.contains(&p)))
            .collect();
        let mut sources: Vec<i64> = named.iter().filter_map(|&i| bodies[i].item_name_id).collect();
        sources.sort_unstable();
        sources.dedup();
        let ([_], Some(&from)) = (sources.as_slice(), named.first()) else { continue };
        let (name, item, folder, hidden, color) =
            (bodies[from].name.clone(), bodies[from].item_name_id, bodies[from].folder.clone(), bodies[from].hidden, bodies[from].color);
        for b in
            bodies.iter_mut().filter(|b| b.item_name_id.is_none() && b.name.is_none() && b.partition.is_some_and(|p| pids.contains(&p)))
        {
            b.name = name.clone();
            b.item_name_id = item;
            b.folder = folder.clone();
            b.hidden = hidden;
            b.color = b.color.or(color);
        }
    }
    Ok(())
}

/// Imported body → the item that names it, read through the file's chain instead of by position: an item's name
/// anchors (type 18, `nameToAnchor`) a name whose one source (type 0, `sourceTopologyNames`) is the imported body's
/// own name (type 11, `bodyID`). Several bodies of one file can share a display name, so the chain is the only sure
/// way. A body two items claim is left to the other ways of naming it.
fn imported_items<'a>(c: &rusqlite::Connection, items: &'a [Item]) -> Result<BTreeMap<i64, &'a Item>> {
    let mut out: BTreeMap<i64, &Item> = BTreeMap::new();
    if !has_table(c, "HistoryNames") {
        return Ok(out);
    }
    let mut q = c.prepare("SELECT Type, Data FROM HistoryNames WHERE NameID = ?1")?;
    let mut name = |id: i64| -> Option<(i64, serde_json::Value)> {
        let (ty, data) = q.query_row([id], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?)))).ok()?;
        Some((ty, serde_json::from_str(&data).ok()?))
    };
    let mut twice = std::collections::HashSet::new();
    for item in items.iter().take(1_000_000) {
        let Some((18, a)) = name(item.name_id) else { continue };
        let Some((0, anchor)) = a["nameToAnchor"].as_i64().and_then(&mut name) else { continue };
        let [source] = anchor["sourceTopologyNames"].as_array().map(Vec::as_slice).unwrap_or_default() else { continue };
        let Some((11, body)) = source.as_i64().and_then(&mut name) else { continue };
        // A face's name carries its identifier too; an item names a whole body. Files since 2026 keep an
        // `identifier` key on body names as well, null there — only a real value marks a face.
        if body.get("identifier").is_some_and(|v| !v.is_null()) {
            continue;
        }
        let Some(id) = body["bodyID"].as_i64() else { continue };
        if out.insert(id, item).is_some() {
            twice.insert(id);
        }
    }
    out.retain(|id, _| !twice.contains(id));
    Ok(out)
}

/// A body a step changes is told apart by its faces, not by its place in the call: each changed-body record lists the
/// topology names the step gave that partition (`[partition, [true, [before, after, tags, names, n, _, lineage]]]`)
/// and, under tag 0 of `lineage`, the bodies in the same call it carries on from (a Subtract's target, both halves of
/// a Union). An item's anchor lists the face names of the body it means, so the partition holding those names is
/// that item's body, and the newest name and visibility set on it are the ones that count.
fn name_by_topology(
    c: &rusqlite::Connection,
    steps: &[Step],
    items: &[Item],
    gone: &std::collections::HashSet<i64>,
    bodies: &mut [Body],
) -> Result<()> {
    use std::collections::{HashMap, HashSet};
    if !has_table(c, "HistoryNames") || !has_calls(c) || !has_table(c, "MetadataAssignments") {
        return Ok(());
    }
    let order: HashMap<i64, usize> = steps.iter().enumerate().map(|(i, s)| (s.node, i)).collect();
    let mut calls: Vec<(usize, i64, Value)> = Vec::new();
    {
        for (k, d) in call_rows(c)? {
            let Some(key) = k.as_array() else { continue };
            let Some(at) = key.first().and_then(Value::as_i64).and_then(|n| order.get(&n)) else { continue };
            let sub = key.get(1).and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_i64).unwrap_or(0);
            calls.push((*at, sub, d));
        }
    }
    calls.sort_by_key(|(at, sub, _)| (*at, *sub));
    let mut carried: HashMap<i64, HashSet<i64>> = HashMap::new(); // partition → topology names on its faces
    for (_, _, data) in &calls {
        let changed = data
            .as_array()
            .and_then(|a| a.get(2))
            .and_then(Value::as_array)
            .filter(|a| a.first().and_then(Value::as_bool) == Some(true))
            .and_then(|a| a.get(1))
            .and_then(Value::as_array)
            .and_then(|a| a.get(1))
            .and_then(Value::as_array);
        let Some(changed) = changed else { continue };
        let pids: Vec<Option<i64>> = changed.iter().map(|e| e.as_array().and_then(|a| a.first()).and_then(Value::as_i64)).collect();
        let mut gained: Vec<(i64, HashSet<i64>)> = Vec::new();
        for (e, pid) in changed.iter().zip(&pids) {
            let Some(pid) = *pid else { continue };
            let Some(detail) =
                e.as_array().and_then(|a| a.get(1)).and_then(Value::as_array).and_then(|a| a.get(1)).and_then(Value::as_array)
            else {
                continue;
            };
            let mut names: HashSet<i64> = detail
                .get(3)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).take(100_000).collect())
                .unwrap_or_default();
            // Files since 2026 stamp the names in other slots: an import keeps them under tag 0 (`[0, name, …]`)
            // and a transform keeps `[name, [name], …]` triples, where the tag's own marker carries a 0.
            for el in detail.iter().filter_map(Value::as_array) {
                if el.first() == Some(&Value::Integer(0.into())) {
                    for n in el.iter().skip(1).filter_map(Value::as_i64) {
                        names.insert(n);
                    }
                } else {
                    for n in el.iter().filter_map(|t| t.as_array().and_then(|t| t.first()).and_then(Value::as_i64)) {
                        if n > 0 {
                            names.insert(n);
                        }
                    }
                }
            }
            // Tag 0 is the body itself: `[0, [own], [[index of a body in this call, [0]], …]]`.
            let whole = detail
                .get(6)
                .and_then(Value::as_array)
                .and_then(|l| l.iter().filter_map(Value::as_array).find(|el| el.first().and_then(Value::as_i64) == Some(0)));
            for from in whole.and_then(|el| el.get(2)).and_then(Value::as_array).into_iter().flatten() {
                let index = from.as_array().and_then(|f| f.first()).and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok());
                if let Some(names_of) = index.and_then(|i| pids.get(i).copied().flatten()).and_then(|p| carried.get(&p)) {
                    names.extend(names_of.iter().copied());
                }
            }
            gained.push((pid, names));
        }
        for (pid, names) in gained {
            carried.entry(pid).or_default().extend(names);
        }
    }
    let alive: HashSet<i64> = bodies.iter().filter_map(|b| b.partition).filter(|p| !gone.contains(p)).collect();
    let mut holders: HashMap<i64, Vec<i64>> = HashMap::new(); // topology name → the current partitions carrying it
    for (pid, names) in carried.iter().filter(|(pid, _)| alive.contains(pid)) {
        for n in names {
            holders.entry(*n).or_default().push(*pid);
        }
    }
    if holders.is_empty() {
        return Ok(());
    }
    // Names and visibility, newest last, by the name they were set on.
    let mut set_on: BTreeMap<i64, Vec<(i64, i64, String)>> = BTreeMap::new();
    {
        let mut q = c.prepare(
            "SELECT a.NameID, m.MetadataID, m.MetadataTypeID, m.Data FROM MetadataAssignments a JOIN Metadata m ON m.MetadataID = a.MetadataID \
             WHERE m.MetadataTypeID IN (0, 1) ORDER BY m.MetadataID",
        )?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, text(r.get(3)?))))?;
        for (name, mid, ty, data) in rows.flatten().take(1_000_000) {
            set_on.entry(name).or_default().push((mid, ty, data));
        }
    }
    let mut row = c.prepare("SELECT Type, Data FROM HistoryNames WHERE NameID = ?1")?;
    let mut json = |id: i64| -> Option<(i64, serde_json::Value)> {
        let (ty, data) = row.query_row([id], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?)))).ok()?;
        Some((ty, serde_json::from_str(&data).ok()?))
    };
    // A step's own names by (step, qualifier), with their entropy: what a call-prefix name stands for.
    type Entropies = Vec<(Vec<i64>, i64)>;
    let mut by_call: HashMap<(i64, i64), Entropies> = HashMap::new();
    {
        let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type = 0")?;
        for (id, data) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?.flatten().take(10_000_000) {
            let Ok(d) = serde_json::from_str::<serde_json::Value>(&data) else { continue };
            let (Some(node), Some(qualifier)) = (d["callKey"]["nodeID"].as_i64(), d["qualifier"].as_i64()) else { continue };
            let entropy =
                d["callKey"]["entropy"].as_array().map(|e| e.iter().filter_map(serde_json::Value::as_i64).collect()).unwrap_or_default();
            by_call.entry((node, qualifier)).or_default().push((entropy, id));
        }
    }
    // partition → (newest name's metadata id, the name it hangs off), (newest visibility's metadata id, hidden)
    type Newest = (Option<(i64, i64)>, Option<(i64, bool)>);
    let mut newest: HashMap<i64, Newest> = HashMap::new();
    for (name_id, metas) in &set_on {
        let Some((18, anchor)) = json(*name_id) else { continue };
        let Some((ty, target)) = anchor["nameToAnchor"].as_i64().and_then(&mut json) else { continue };
        // A call-prefix name (a body a step made, named by the step itself) stands for the step's names that start
        // with it: the partitions carrying those are that body.
        let prefixed: Vec<i64> = if ty == 1 {
            let (node, qualifier) = (target["callKeyPrefix"]["nodeID"].as_i64(), target["qualifier"].as_i64());
            let prefix: Vec<i64> = target["callKeyPrefix"]["entropyPrefix"]
                .as_array()
                .map(|e| e.iter().filter_map(serde_json::Value::as_i64).collect())
                .unwrap_or_default();
            node.zip(qualifier)
                .and_then(|k| by_call.get(&k))
                .map(|v| v.iter().filter(|(e, _)| e.starts_with(&prefix)).map(|(_, id)| *id).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let faces: Vec<i64> = match ty {
            8 => target["faceNames"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_i64).take(100_000).collect(),
            5 => target["originals"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_i64).take(100_000).collect(),
            1 => prefixed,
            _ => continue,
        };
        let mut votes: HashMap<i64, usize> = HashMap::new();
        for n in faces {
            for pid in holders.get(&n).into_iter().flatten() {
                *votes.entry(*pid).or_default() += 1;
            }
        }
        // Only a clear answer counts: one partition holding more of the item's faces than any other.
        let best = votes.values().copied().max().unwrap_or(0);
        let mut winners = votes.iter().filter(|(_, v)| **v == best).map(|(p, _)| *p);
        let (Some(pid), None) = (winners.next(), winners.next()) else { continue };
        let entry = newest.entry(pid).or_default();
        for (mid, ty, data) in metas {
            if *ty == 0 && entry.0.is_none_or(|v| *mid > v.0) {
                entry.0 = Some((*mid, *name_id));
            }
            if *ty == 1 && entry.1.is_none_or(|v| *mid > v.0) {
                entry.1 = Some((*mid, data.trim() == "1"));
            }
        }
    }
    for b in bodies.iter_mut() {
        let Some((named, shown)) = b.partition.and_then(|p| newest.get(&p)) else { continue };
        if let Some(item) = named.and_then(|(_, name_id)| items.iter().find(|i| i.name_id == name_id)) {
            b.name = Some(item.name.clone());
            b.item_name_id = Some(item.name_id);
            b.folder = item.folder.clone();
            b.hidden = item.hidden;
            b.color = item.color.or(b.color);
        }
        if let Some((_, hidden)) = shown {
            b.hidden = *hidden || b.hidden && named.is_none();
        }
    }
    Ok(())
}

/// A transform's explicit topology selector can identify an item created after the partition's first call.
/// Only use it when the call modifies exactly one current partition and the selector identifies one item.
fn resolve_transformed_items(
    c: &rusqlite::Connection,
    steps: &[Step],
    items: &[Item],
    folders: &[Folder],
    gone: &std::collections::HashSet<i64>,
    bodies: &mut [Body],
) -> Result<()> {
    if !has_table(c, "HistoryNames") || !has_calls(c) || !has_table(c, "MetadataAssignments") {
        return Ok(());
    }
    let mut names = BTreeMap::new();
    let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type IN (18, 19)")?;
    for (id, data) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?.flatten().take(1_000_000) {
        names.insert(id, serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default());
    }
    let root = |mut id: i64| {
        for _ in 0..16 {
            let Some(next) = names.get(&id).and_then(|n| n["nameToAnchor"].as_i64().or_else(|| n["wrappedName"].as_i64())) else { break };
            if next == id {
                break;
            }
            id = next;
        }
        id
    };
    // Metadata assignments on sibling anchors are successive edits to the same topology name.
    type LatestMetadata = (Option<(i64, i64)>, Option<(i64, bool)>);
    let mut latest: BTreeMap<i64, LatestMetadata> = BTreeMap::new();
    let mut q = c.prepare("SELECT a.NameID, m.MetadataID, m.MetadataTypeID, m.Data FROM MetadataAssignments a JOIN Metadata m ON m.MetadataID = a.MetadataID WHERE m.MetadataTypeID IN (0, 1)")?;
    for (id, mid, ty, data) in
        q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, text(r.get(3)?))))?.flatten().take(1_000_000)
    {
        let entry = latest.entry(root(id)).or_default();
        if ty == 0 && entry.0.is_none_or(|v| mid > v.0) {
            entry.0 = Some((mid, id));
        }
        if ty == 1 && entry.1.is_none_or(|v| mid > v.0) {
            entry.1 = Some((mid, data.trim() == "1"));
        }
    }
    let mut modified: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut unchanged: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut surviving: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for (key, data) in call_rows(c)? {
        let Some(node) = key.as_array().and_then(|a| a.first()).and_then(Value::as_i64) else { continue };
        let changed = data
            .as_array()
            .and_then(|a| a.get(2))
            .and_then(Value::as_array)
            .filter(|a| a.first().and_then(Value::as_bool) == Some(true))
            .and_then(|a| a.get(1))
            .and_then(Value::as_array)
            .and_then(|a| a.get(1))
            .and_then(Value::as_array);
        if let Some(changed) = changed {
            for e in changed {
                let Some(a) = e.as_array() else { continue };
                let Some(pid) = a.first().and_then(Value::as_i64) else { continue };
                match a.get(1).and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_bool) {
                    Some(true) => {
                        modified.entry(node).or_default().push(pid);
                        let after = a
                            .get(1)
                            .and_then(Value::as_array)
                            .and_then(|a| a.get(1))
                            .and_then(Value::as_array)
                            .and_then(|a| a.get(1))
                            .and_then(Value::as_array)
                            .and_then(|a| a.first())
                            .and_then(Value::as_bool);
                        if after == Some(true) {
                            surviving.entry(node).or_default().push(pid);
                        }
                    }
                    Some(false) => unchanged.entry(node).or_default().push(pid),
                    None => {}
                }
            }
        }
    }
    fn selectors(v: &Value, out: &mut Vec<i64>, depth: usize) {
        if depth >= 64 || out.len() >= 1024 {
            return;
        }
        if let Value::Array(a) = v {
            if let [Value::Integer(k), Value::Integer(n)] = a.as_slice()
                && k.as_i64() == Some(7)
            {
                out.extend(n.as_i64());
            }
            for v in a {
                selectors(v, out, depth + 1);
            }
        }
    }
    let mut outputs: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut q = c.prepare("SELECT NameID, Data FROM HistoryNames WHERE Type = 1")?;
    for (id, data) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?.flatten().take(1_000_000) {
        let data: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
        if data["qualifier"].as_i64() == Some(4)
            && let Some(node) = data["callKeyPrefix"]["nodeID"].as_i64()
            && latest.get(&id).is_some_and(|v| v.0.is_some())
        {
            outputs.entry(node).or_default().push(id);
        }
    }
    let mut associations = Vec::new();
    // A Move/Rotate with its Copy switch on (its sixth parameter, `[1, true]`) leaves the source where it is and
    // makes a new body, as Mirror does.
    let copies = |step: &Step| {
        step.op == "Transform"
            && step.params.get(5).is_some_and(|(_, v)| {
                fn flag(v: &Value, depth: usize) -> Option<bool> {
                    match v.as_array()?.as_slice() {
                        [Value::Integer(k), Value::Boolean(b)] if k.as_i64() == Some(1) => Some(*b),
                        [inner] if depth < 4 => flag(inner, depth + 1),
                        _ => None,
                    }
                }
                flag(v, 0) == Some(true)
            })
    };
    for step in steps.iter().filter(|s| matches!(s.op.as_str(), "Transform" | "Rotate" | "Mirror")) {
        let makes_new = step.op == "Mirror" || copies(step);
        let pids = if makes_new { &unchanged } else { &modified };
        if let (Some(pids), Some((_, target))) = (pids.get(&step.node), step.params.first()) {
            let mut pids = pids.clone();
            pids.sort_unstable();
            pids.dedup();
            let mut refs = Vec::new();
            selectors(target, &mut refs, 0);
            let mut refs: Vec<i64> = refs.into_iter().map(root).filter(|id| latest.get(id).is_some_and(|v| v.0.is_some())).collect();
            refs.sort_unstable();
            refs.dedup();
            if let ([pid], [id]) = (pids.as_slice(), refs.as_slice()) {
                associations.push((*pid, *id, step.node));
            }
        }
        // Mirror (and a copying move) has one unchanged source and one modified result. The latter has its own
        // call-prefix name.
        if makes_new && let (Some(pids), Some(ids)) = (modified.get(&step.node), outputs.get(&step.node)) {
            let mut pids = pids.clone();
            pids.sort_unstable();
            pids.dedup();
            if let ([pid], [id]) = (pids.as_slice(), ids.as_slice()) {
                associations.push((*pid, *id, step.node));
            }
        }
    }
    // A Boolean can retain one partition while consuming several inputs. When only one of its retained
    // partitions is still current, a single newly anchored, named result identifies that partition.
    let mut first_anchor: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for (&anchor, data) in &names {
        let (Some(target), Some(node)) = (data["nameToAnchor"].as_i64(), data["callKey"]["nodeID"].as_i64()) else { continue };
        let target = root(target);
        if first_anchor.get(&target).is_none_or(|(first, _)| anchor < *first) {
            first_anchor.insert(target, (anchor, node));
        }
    }
    let mut boolean_outputs: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut q = c.prepare("SELECT NameID FROM HistoryNames WHERE Type = 5")?;
    for id in q.query_map([], |r| r.get::<_, i64>(0))?.flatten().take(1_000_000) {
        if let Some(&(anchor, node)) = first_anchor.get(&id)
            && latest.get(&id).and_then(|v| v.0).is_some_and(|(_, name_id)| name_id == anchor)
        {
            boolean_outputs.entry(node).or_default().push(id);
        }
    }
    let current: std::collections::HashSet<i64> = bodies.iter().filter_map(|b| b.partition).filter(|id| !gone.contains(id)).collect();
    let mut topology = c.prepare("SELECT Type, Data FROM HistoryNames WHERE NameID = ?1")?;
    for step in steps.iter().filter(|s| s.op == "Intersect") {
        let Some(pids) = surviving.get(&step.node) else { continue };
        let mut pids: Vec<_> = pids.iter().copied().filter(|p| current.contains(p)).collect();
        pids.sort_unstable();
        pids.dedup();
        let ids = boolean_outputs.get(&step.node).cloned().unwrap_or_default();
        let mut target = Vec::new();
        if let Some((_, value)) = step.params.first() {
            selectors(value, &mut target, 0);
        }
        target = target.into_iter().map(root).filter(|id| latest.get(id).is_some_and(|v| v.0.is_some())).collect();
        target.sort_unstable();
        target.dedup();
        let selected = if ids.len() == 1 {
            ids.first().copied()
        } else if let [id] = target.as_slice() {
            let target_mid = latest.get(id).and_then(|v| v.0).map(|v| v.0);
            let newest_output = ids.iter().filter_map(|id| latest.get(id).and_then(|v| v.0).map(|v| v.0)).max();
            let references = ids
                .iter()
                .take(64)
                .filter(|output| {
                    let data = topology
                        .query_row([*output], |r| Ok(text(r.get(1)?)))
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
                    data.as_ref().and_then(|d| d["originals"].as_array()).is_some_and(|originals| {
                        originals.iter().take(64).filter_map(serde_json::Value::as_i64).any(|original| {
                            if root(original) == *id {
                                return true;
                            }
                            let node = topology.query_row([original], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?)))).ok();
                            let Some((0, data)) = node else { return false };
                            serde_json::from_str::<serde_json::Value>(&data)
                                .ok()
                                .and_then(|d| d["sourceTopologyNames"].as_array().cloned())
                                .is_some_and(|sources| {
                                    sources.iter().take(64).filter_map(serde_json::Value::as_i64).any(|source| root(source) == *id)
                                })
                        })
                    })
                })
                .count();
            (target_mid > newest_output && references == 1 && ids.len() <= 64).then_some(*id)
        } else {
            None
        };
        if let ([pid], Some(id)) = (pids.as_slice(), selected) {
            associations.push((*pid, id, step.node));
        }
    }
    for (pid, id, node) in associations {
        let Some((Some((_, name_id)), hidden)) = latest.get(&id) else { continue };
        let Some(item) = items.iter().find(|i| i.name_id == *name_id) else { continue };
        if let Some(body) = bodies.iter_mut().find(|b| b.partition == Some(pid)) {
            // The name the step created for its own result outranks the selector's: the selector
            // identifies the body the step transformed, and the result carries the name the step
            // gave it.
            if body.item_name_id.is_some_and(|nid| names.get(&nid).and_then(|n| n["callKey"]["nodeID"].as_i64()) == Some(node)) {
                continue;
            }
            body.name = Some(item.name.clone());
            body.item_name_id = Some(item.name_id);
            body.folder = item.folder.clone();
            let mut folder = folders.iter().find(|f| f.name == "root");
            let mut folder_hidden = folder.is_some_and(|f| f.hidden);
            for name in &body.folder {
                folder = folder.and_then(|f| {
                    f.children
                        .iter()
                        .filter(|(_, ty)| *ty == 0)
                        .find_map(|(id, _)| folders.iter().find(|child| child.id == *id && child.name == *name))
                });
                folder_hidden |= folder.is_some_and(|f| f.hidden);
            }
            body.hidden = folder_hidden || hidden.is_some_and(|v| v.1);
            body.color = body.color.or(item.color);
        }
    }
    Ok(())
}

/// Whether the workspace has a table: the schema changes between Shapr3D releases.
pub(crate) fn has_table(c: &rusqlite::Connection, name: &str) -> bool {
    c.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
}

/// Solids that couldn't be read, with their partition (when they have one) and why.
type Unreadable = Vec<(Option<i64>, String)>;

/// The solids of the cached partitions; the imported prototypes when no partition is cached.
fn current_bodies(c: &rusqlite::Connection, paint: &FacePaint) -> Result<(Vec<Body>, Unreadable)> {
    let mut parts: BTreeMap<i64, Vec<(i64, Vec<u8>)>> = BTreeMap::new();
    if has_table(c, "BodyRevisionBlocks") {
        let mut q = c.prepare("SELECT PartitionID, ChunkIndex, Block FROM BodyRevisionBlocks WHERE IsDeleted = 0 AND Phase = 2")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, blob(r.get(2)?))))?;
        for (pid, chunk, b) in rows.flatten() {
            parts.entry(pid).or_default().push((chunk, b));
        }
    } else if has_table(c, "BodyRevisionPartitions") {
        // Files saved up to early 2026 keep each partition whole, in one row; later ones of them
        // keep rows of deleted partitions (IsDeleted, no block) and several phases of a row (the most settled wins).
        let sql = if has_column(c, "BodyRevisionPartitions", "IsDeleted") {
            "SELECT PartitionID, Block FROM BodyRevisionPartitions WHERE IsDeleted = 0 AND Block IS NOT NULL ORDER BY PartitionID, Phase DESC"
        } else {
            "SELECT PartitionID, Block FROM BodyRevisionPartitions WHERE Block IS NOT NULL"
        };
        let mut q = c.prepare(sql)?;
        for (pid, b) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?))))?.flatten() {
            parts.entry(pid).or_insert_with(|| vec![(0, b)]);
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
                item_name_id: None,
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
            paint.apply(&mut body, import.or_else(|| partition.and_then(|pid| paint.import_of.get(&pid).copied())));
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
                item_name_id: None,
                brep: body.brep,
                color: body.color,
                hidden: false,
                mesh: body.mesh,
            });
        }
    }
    Ok((out, unreadable))
}

/// Colours set on single faces of imported bodies (a STEP file's per-face colours come in this way). Each one is a
/// material set on a name the import step gave the face, `[11] {bodyID, identifier}`: the imported body and the
/// face's node id in it, which the body keeps in the partitions made from it. The newest setting of a face wins.
#[derive(Default)]
struct FacePaint {
    /// Imported body → face node id → colour (0–1).
    colours: BTreeMap<i64, BTreeMap<i64, [f32; 3]>>,
    /// Partition the import step made → its imported body.
    import_of: BTreeMap<i64, i64>,
}

impl FacePaint {
    fn read(c: &rusqlite::Connection, steps: &[Step]) -> Result<Self> {
        let mut paint = FacePaint::default();
        if !has_table(c, "MaterialInstances") || !has_table(c, "HistoryNames") || !has_table(c, "MetadataAssignments") {
            return Ok(paint);
        }
        paint.import_of = import_slots(c, steps)?.into_iter().filter_map(|(id, pid)| Some((pid?, id))).collect();
        let mut palette: BTreeMap<String, Option<[f32; 3]>> = BTreeMap::new();
        {
            let mut q = c.prepare("SELECT InstanceID, Properties FROM MaterialInstances")?;
            for (id, props) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?))))?.flatten().take(100_000) {
                let Some(rgba) = unpack(&props).and_then(|v| v.as_array()?.first()?.as_u64()) else { continue };
                let rgb = (rgba >> 8) as u32 & 0xFF_FFFF;
                // The default grey is no colour: the body's own shows.
                let colour = (rgb != 0xA0_A0A4)
                    .then(|| [(rgb >> 16) as f32 / 255.0, ((rgb >> 8) & 0xFF) as f32 / 255.0, (rgb & 0xFF) as f32 / 255.0]);
                palette.insert(id.to_string(), colour);
            }
        }
        let mut name = c.prepare("SELECT Type, Data FROM HistoryNames WHERE NameID = ?1")?;
        let mut json = |id: i64| -> Option<(i64, serde_json::Value)> {
            let (ty, data) = name.query_row([id], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?)))).ok()?;
            Some((ty, serde_json::from_str(&data).ok()?))
        };
        let mut q = c.prepare(
            "SELECT a.NameID, m.Data FROM MetadataAssignments a JOIN Metadata m ON m.MetadataID = a.MetadataID \
             WHERE m.MetadataTypeID = 2 ORDER BY m.MetadataID",
        )?;
        let rows: Vec<(i64, String)> = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, text(r.get(1)?))))?.flatten().take(1_000_000).collect();
        for (anchor, instance) in rows {
            let Some(colour) = palette.get(instance.trim()) else { continue };
            let Some((18, a)) = json(anchor) else { continue };
            let Some((0, face)) = a["nameToAnchor"].as_i64().and_then(&mut json) else { continue };
            let [source] = face["sourceTopologyNames"].as_array().map(Vec::as_slice).unwrap_or_default() else { continue };
            let Some((11, imported)) = source.as_i64().and_then(&mut json) else { continue };
            let (Some(body), Some(id)) = (imported["bodyID"].as_i64(), imported["identifier"].as_i64()) else { continue };
            let faces = paint.colours.entry(body).or_default();
            match colour {
                Some(c) => faces.insert(id, *c),
                None => faces.remove(&id),
            };
        }
        Ok(paint)
    }

    /// Paints the faces of a body read from `import`'s prototype or from a partition made from it.
    fn apply(&self, body: &mut shapecraft_xt::Body, import: Option<i64>) {
        let Some(colours) = import.and_then(|i| self.colours.get(&i)) else { return };
        for (face, id) in body.brep.faces.iter_mut().zip(&body.face_ids) {
            if let Some(colour) = colours.get(id) {
                face.color = Some(*colour);
            }
        }
    }
}

/// The imports whose body the file keeps no revision data for, so it has to come from its prototype: the file keeps
/// revision blocks only for bodies that were rebuilt, and an import nothing touched afterwards has none.
///
/// An import step's k-th created partition (by creation index) belongs to its k-th import. Only a partition that
/// shows up nowhere counts: not in the revision tables under any state, and not among the removed ones (`gone`). A
/// body deleted later, or edited later into a newer partition, still has its first partition there, and bringing
/// its prototype back would resurrect or duplicate it. An import with no partition recorded is left alone too.
///
/// Only calls in the newer layout (more than three slots) are read; files in the older one keep every imported
/// body in `BodyRevisionPartitions`.
fn uncovered_imports(c: &rusqlite::Connection, steps: &[Step], gone: &std::collections::HashSet<i64>) -> Result<Vec<i64>> {
    if !has_calls(c) {
        return Ok(Vec::new());
    }
    let mut known: std::collections::HashSet<i64> = gone.clone();
    for table in ["BodyRevisionBlocks", "BodyRevisionPartitions"] {
        if has_table(c, table) {
            let mut q = c.prepare(&format!("SELECT DISTINCT PartitionID FROM {table}"))?;
            known.extend(q.query_map([], |r| r.get::<_, i64>(0))?.flatten().take(10_000_000));
        }
    }
    Ok(import_slots(c, steps)?.into_iter().filter(|(_, pid)| pid.is_some_and(|pid| !known.contains(&pid))).map(|(id, _)| id).collect())
}

/// Each import's imported body and the partition the import step made for it, when the step's call records one: the
/// step's k-th created partition (by creation index) belongs to its k-th import. Only calls in the newer layout (more
/// than three slots) record them.
/// The size of the box of a body's vertices: the signature of a shape that a Move moves and a
/// copy keeps.
fn size_of(data: &[u8]) -> Option<[f64; 3]> {
    let f = shapecraft_xt::parse(data).ok()?;
    let b = shapecraft_xt::bodies(&f).ok()?.first()?.clone();
    let (mut lo, mut hi) = ([f64::MAX; 3], [-f64::MAX; 3]);
    for v in &b.brep.vertices {
        for (k, c) in [(0usize, v.x), (1, v.y), (2, v.z)] {
            lo[k] = lo[k].min(c);
            hi[k] = hi[k].max(c);
        }
    }
    (lo[0] <= hi[0]).then_some([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]])
}

fn import_slots(c: &rusqlite::Connection, steps: &[Step]) -> Result<Vec<(i64, Option<i64>)>> {
    if !has_calls(c) {
        return Ok(Vec::new());
    }
    let mut step_parts: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    {
        for (k, d) in call_rows(c)? {
            let Some(node) = k.as_array().and_then(|a| a.first()).and_then(Value::as_i64) else { continue };
            let Some(a) = d.as_array() else { continue };
            if a.len() <= 3 {
                continue;
            }
            let created = a
                .get(5)
                .and_then(Value::as_array)
                .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
                .and_then(|x| x.get(1))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut indexed: Vec<(i64, i64)> = created
                .iter()
                .flat_map(|g| g.as_array().cloned().unwrap_or_default())
                .filter_map(|c| {
                    let c = c.as_array()?;
                    Some((c.first()?.as_i64()?, c.get(1)?.as_i64()?))
                })
                .collect();
            if !indexed.is_empty() {
                indexed.sort_unstable_by_key(|&(idx, _)| idx);
                step_parts.entry(node).or_default().extend(indexed.into_iter().map(|(_, pid)| pid));
            } else if let Some(changed) = a
                .get(2)
                .and_then(Value::as_array)
                .filter(|x| x.first().and_then(Value::as_bool) == Some(true))
                .and_then(|x| x.get(1))
                .and_then(Value::as_array)
                .and_then(|gs| gs.get(1))
                .and_then(Value::as_array)
            {
                // Files since 2026 list an import's bodies as modified instead: slot 5 is `[false]`, and the
                // call's second modified group lists the partitions it brought in, in output order — the same
                // order the step lists its imported bodies in.
                step_parts
                    .entry(node)
                    .or_default()
                    .extend(changed.iter().filter_map(|e| e.as_array().and_then(|e| e.first()).and_then(Value::as_i64)));
            }
        }
    }
    fn import_id(v: &Value, out: &mut Vec<i64>) {
        if let Value::Array(a) = v {
            if let [Value::Integer(k), Value::Integer(n)] = a.as_slice()
                && k.as_i64() == Some(11)
            {
                out.extend(n.as_i64());
            }
            a.iter().for_each(|x| import_id(x, out));
        }
    }
    let mut out = Vec::new();
    for step in steps.iter().filter(|s| s.op == "MaterializeImportedBodies") {
        let Some(partitions) = step_parts.get(&step.node) else { continue };
        let mut ids = Vec::new();
        step.params.iter().for_each(|(_, v)| import_id(v, &mut ids));
        for (k, id) in ids.into_iter().enumerate() {
            out.push((id, partitions.get(k).copied()));
        }
    }
    // The imports list their bodies in another order than the partitions their call made when
    // bodies share a shape: the body a partition sizes to is another one's. Swap the pairs that
    // size to each other's, and let the removed partitions keep the bodies they removed. The
    // sizes come from the cached partitions and the prototypes: without both tables, the pairing
    // stays as the lists order it.
    if !has_table(c, "BodyRevisionBlocks") || !has_table(c, "HistoryImportedBodies") || !has_table(c, "HistoryImportedPrototypes") {
        return Ok(out);
    }
    let mut part_size: BTreeMap<i64, Option<[f64; 3]>> = BTreeMap::new();
    {
        let mut q = c.prepare("SELECT PartitionID, ChunkIndex, Block FROM BodyRevisionBlocks WHERE IsDeleted = 0 AND Phase = 2")?;
        let mut chunks: BTreeMap<i64, Vec<(i64, Vec<u8>)>> = BTreeMap::new();
        for (pid, chunk, b) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, blob(r.get(2)?))))?.flatten() {
            chunks.entry(pid).or_default().push((chunk, b));
        }
        for (pid, mut cs) in chunks {
            cs.sort_by_key(|c| c.0);
            let data: Vec<u8> = cs.into_iter().flat_map(|c| c.1).collect();
            part_size.insert(pid, size_of(&data));
        }
    }
    let mut proto_size: BTreeMap<i64, Option<[f64; 3]>> = BTreeMap::new();
    {
        let mut q = c.prepare(
            "SELECT b.ImportedBodyID, p.BodyData FROM HistoryImportedBodies b \
             JOIN HistoryImportedPrototypes p ON p.ImportedPrototypeID = b.ImportedPrototypeID",
        )?;
        for (id, data) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?))))?.flatten() {
            proto_size.insert(id, size_of(&data));
        }
    }
    let near = |a: [f64; 3], b: [f64; 3]| (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.5 && (a[2] - b[2]).abs() < 0.5;
    // Only the mis-sized pairs swap: two pairs that already size consistently would keep
    // trading partitions for ever (the imports repeat shapes across families).
    for _ in 0..64 {
        let bad: Vec<usize> = (0..out.len())
            .filter(|&k| {
                out.get(k).is_some_and(|(id, pid)| {
                    let (Some(w), Some(h)) =
                        (proto_size.get(id).copied().flatten(), pid.as_ref().and_then(|p| part_size.get(p)).copied().flatten())
                    else {
                        return false;
                    };
                    !near(w, h)
                })
            })
            .collect();
        let mut swapped = false;
        'outer: for a in 0..bad.len() {
            for b in a + 1..bad.len() {
                let (i, j) = (bad[a], bad[b]);
                let ((idi, pidi), (idj, pidj)) = (out[i], out[j]);
                let (Some(wi), Some(hj)) =
                    (proto_size.get(&idi).copied().flatten(), pidj.as_ref().and_then(|p| part_size.get(p)).copied().flatten())
                else {
                    continue;
                };
                let (Some(wj), Some(hi)) =
                    (proto_size.get(&idj).copied().flatten(), pidi.as_ref().and_then(|p| part_size.get(p)).copied().flatten())
                else {
                    continue;
                };
                if near(wi, hj) && near(wj, hi) {
                    out[i].1 = pidj;
                    out[j].1 = pidi;
                    swapped = true;
                    break 'outer;
                }
            }
        }
        if !swapped {
            break;
        }
    }
    Ok(out)
}

/// The bodies of [`uncovered_imports`], each from its prototype and placed by the import's transform.
fn uncovered_import_bodies(
    c: &rusqlite::Connection,
    steps: &[Step],
    gone: &std::collections::HashSet<i64>,
    paint: &FacePaint,
) -> Result<(Vec<Body>, Unreadable)> {
    if !has_table(c, "HistoryImportedPrototypes") || !has_table(c, "HistoryImportedBodies") {
        return Ok((Vec::new(), Vec::new()));
    }
    let wanted = uncovered_imports(c, steps, gone)?;
    if wanted.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    // ImportedBodyID → (transform bytes, body bytes)
    let mut proto: BTreeMap<i64, (Vec<u8>, Vec<u8>)> = BTreeMap::new();
    {
        let mut q = c.prepare(
            "SELECT b.ImportedBodyID, b.Transform, p.BodyData \
             FROM HistoryImportedBodies b JOIN HistoryImportedPrototypes p \
             ON p.ImportedPrototypeID = b.ImportedPrototypeID",
        )?;
        for (id, t, data) in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, blob(r.get(1)?), blob(r.get(2)?))))?.flatten() {
            proto.insert(id, (t, data));
        }
    }
    let mut out = Vec::new();
    let mut unreadable: Unreadable = Vec::new();
    for import_id in wanted {
        let Some((t, data)) = proto.get(&import_id) else { continue };
        let motion = rigid(t);
        match shapecraft_xt::parse(data).and_then(|f| shapecraft_xt::bodies(&f)) {
            Ok(bodies) => {
                for mut body in bodies {
                    paint.apply(&mut body, Some(import_id));
                    if let Some((rot, shift)) = motion {
                        body.brep.transform_rigid(rot, shift);
                        if let Some(m) = &mut body.mesh {
                            m.points.iter_mut().for_each(|p| *p = rot * *p + shift);
                        }
                    }
                    out.push(Body {
                        partition: None,
                        import: Some(import_id),
                        folder: Vec::new(),
                        step: None,
                        name: body.name,
                        item_name_id: None,
                        brep: body.brep,
                        color: body.color,
                        hidden: false,
                        mesh: body.mesh,
                    });
                }
            }
            Err(e) => unreadable.push((None, e.to_string())),
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

#[cfg(test)]
mod identity_tests {
    use super::*;

    /// An import comes from its prototype only when its partition is nowhere in the file: not when the body was
    /// deleted later, replaced by an edit, or is simply there.
    #[test]
    fn only_imports_without_any_revision_data_fall_back_to_their_prototypes() -> Result<()> {
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch(
            "CREATE TABLE PersistedCalls (CallKey BLOB, CallData BLOB);
             CREATE TABLE BodyRevisionBlocks (PartitionID INTEGER, ChunkIndex INTEGER, Block BLOB, Phase INTEGER, IsDeleted BOOL);
             INSERT INTO BodyRevisionBlocks VALUES (300, 0, x'00', 2, 1), (400, 0, x'00', 2, 0), (500, 0, x'00', 0, 0);",
        )?;
        let int = |n: i64| Value::Integer(n.into());
        let entry = |index: i64, partition: i64| Value::Array(vec![int(index), int(partition), int(3), int(index)]);
        // Listed out of creation order: imports 1 to 5 made partitions 100 to 500.
        let created = vec![entry(2, 300), entry(0, 100), entry(4, 500), entry(1, 200), entry(3, 400)];
        let no = || Value::Array(vec![Value::Boolean(false)]);
        let data = Value::Array(vec![
            int(0),
            Value::Array(vec![int(0)]),
            no(),
            no(),
            int(0),
            Value::Array(vec![Value::Boolean(true), Value::Array(vec![Value::Array(created)])]),
        ]);
        c.execute("INSERT INTO PersistedCalls VALUES (?1, ?2)", (pack(&Value::Array(vec![int(10)])), pack(&data)))?;
        let imports = Value::Array((1..=5).map(|id| Value::Array(vec![int(11), int(id)])).collect());
        let steps = vec![Step { node: 10, name: "Import".into(), op: "MaterializeImportedBodies".into(), params: vec![(11, imports)] }];
        // 100: no revision data. 200: removed by a later step. 300: a deleted block (an edit made a newer partition).
        // 400: there. 500: a block still being written.
        let gone = std::collections::HashSet::from([200]);
        assert_eq!(uncovered_imports(&c, &steps, &gone)?, [1]);
        // A file in the older layout keeps whole partitions in their own table.
        c.execute_batch("CREATE TABLE BodyRevisionPartitions (PartitionID INTEGER, Block BLOB); INSERT INTO BodyRevisionPartitions VALUES (100, x'00');")?;
        assert!(uncovered_imports(&c, &steps, &gone)?.is_empty());
        Ok(())
    }

    /// A colour set on one face of an imported body reaches that face (by its node id), the newest setting winning and
    /// the default grey taking the colour away again; a body made from another import is left alone.
    #[test]
    fn face_colours_reach_the_faces_they_were_set_on() -> Result<()> {
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch(
            "CREATE TABLE MaterialInstances (InstanceID INTEGER, DefinitionID INTEGER, Properties BLOB);
             CREATE TABLE HistoryNames (NameID INTEGER, Type INT, Data BLOB, Version INT);
             CREATE TABLE MetadataAssignments (RowID INTEGER, NameID INTEGER, MetadataID INTEGER);
             CREATE TABLE Metadata (MetadataID INTEGER, MetadataTypeID INT, Data BLOB);",
        )?;
        for (id, rgba) in [(5u64, 0x0000_00FFu64), (6, 0xA0A0_A4FF), (7, 0xFF00_00FF)] {
            c.execute("INSERT INTO MaterialInstances VALUES (?1, 0, ?2)", (id as i64, pack(&Value::Array(vec![Value::from(rgba)]))))?;
        }
        // Two faces of imported body 9 (node ids 42 and 43), each named by the import and anchored.
        for (base, face) in [(300, 42), (310, 43)] {
            let names = [
                (base, 11, format!(r#"{{"bodyID":9,"identifier":{face}}}"#)),
                (base + 1, 0, format!(r#"{{"qualifier":0,"sourceTopologyNames":[{base}]}}"#)),
                (base + 2, 18, format!(r#"{{"nameToAnchor":{}}}"#, base + 1)),
            ];
            for (id, ty, data) in names {
                c.execute("INSERT INTO HistoryNames VALUES (?1, ?2, ?3, 1)", (id, ty, data))?;
            }
        }
        // Red, then black, on face 42; red, then the default grey, on face 43.
        for (mid, instance, anchor) in [(1000, "7", 302), (1001, "5", 302), (1002, "7", 312), (1003, "6", 312)] {
            c.execute("INSERT INTO Metadata VALUES (?1, 2, ?2)", (mid, instance))?;
            c.execute("INSERT INTO MetadataAssignments VALUES (?1, ?2, ?1)", (mid, anchor))?;
        }
        let paint = FacePaint::read(&c, &[])?;
        let face = || shapecraft_kernel::brep::Face { surface: 0, flip: false, loops: vec![], color: None };
        let body = |face_ids: Vec<i64>| shapecraft_xt::Body {
            node: 1,
            brep: Brep { faces: vec![face(), face(), face()], ..Brep::default() },
            name: None,
            color: None,
            attributes: vec![],
            face_ids,
            face_materials: Default::default(),
            mesh: None,
        };
        let mut ours = body(vec![42, 43, 44]);
        paint.apply(&mut ours, Some(9));
        assert_eq!(ours.brep.faces.iter().map(|f| f.color).collect::<Vec<_>>(), vec![Some([0.0, 0.0, 0.0]), None, None]);
        let mut other = body(vec![42, 43, 44]);
        paint.apply(&mut other, Some(8));
        assert!(other.brep.faces.iter().all(|f| f.color.is_none()));
        Ok(())
    }

    /// #66: an import's bodies are named through the file's chain (item name → anchor → imported body), not by
    /// position: two items sharing a display name still land on their own bodies, with their own folders, and a
    /// body with no item stays loose.
    #[test]
    fn imported_bodies_are_named_through_the_chain_not_by_position() -> Result<()> {
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch(
            "CREATE TABLE HistoryNameAnchoringPoints (AnchoredNameID INTEGER, HistoryTreeNodeID INTEGER);
             INSERT INTO HistoryNameAnchoringPoints VALUES (100, 10), (101, 10), (102, 10);
             CREATE TABLE HistoryNames (NameID INTEGER, Type INT, Data BLOB, Version INT);",
        )?;
        // Item 100 is body 9, item 101 body 7; item 102 anchors a face's name, which is no body.
        for (item, anchor, source, data) in
            [(100, 200, 300, r#"{"bodyID":9}"#), (101, 201, 301, r#"{"bodyID":7}"#), (102, 202, 302, r#"{"bodyID":8,"identifier":42}"#)]
        {
            for (id, ty, json) in [
                (item, 18, format!(r#"{{"nameToAnchor":{anchor}}}"#)),
                (anchor, 0, format!(r#"{{"qualifier":0,"sourceTopologyNames":[{source}]}}"#)),
                (source, 11, data.to_string()),
            ] {
                c.execute("INSERT INTO HistoryNames VALUES (?1, ?2, ?3, 1)", (id, ty, json))?;
            }
        }
        let refs = Value::Array([7, 8, 9].into_iter().map(|n| Value::Array(vec![Value::from(11), Value::from(n)])).collect());
        let steps = vec![Step { node: 10, name: "Import".into(), op: "MaterializeImportedBodies".into(), params: vec![(1, refs)] }];
        let item = |name_id: i64, folder: &str, hidden: bool| Item {
            import_body: None,
            name_id,
            name: "Top Cover (3)".into(),
            folder: if folder.is_empty() { vec![] } else { vec![folder.into()] },
            hidden,
            color: None,
        };
        let items = vec![item(100, "v1", true), item(101, "", false), item(102, "v2", false)];
        let mut bodies: Vec<Body> = [7, 8, 9]
            .into_iter()
            .map(|import| Body {
                partition: None,
                import: Some(import),
                folder: vec![],
                step: None,
                name: None,
                item_name_id: None,
                brep: Brep::default(),
                color: None,
                hidden: false,
                mesh: None,
            })
            .collect();
        name_from_calls(&c, &steps, &items, &[], &Default::default(), &mut bodies, &BTreeMap::new())?;
        let got: Vec<(Option<i64>, Option<i64>, Vec<String>, bool)> =
            bodies.iter().map(|b| (b.import, b.item_name_id, b.folder.clone(), b.hidden)).collect();
        assert_eq!(
            got,
            vec![
                (Some(7), Some(101), vec![], false),
                // No chain reaches body 8, and the item its place would give it is body 7's: it keeps its own name.
                (Some(8), None, vec![], false),
                (Some(9), Some(100), vec!["v1".to_string()], true),
            ]
        );
        Ok(())
    }

    #[test]
    fn imported_siblings_keep_distinct_item_name_ids() -> Result<()> {
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE HistoryNameAnchoringPoints (AnchoredNameID INTEGER, HistoryTreeNodeID INTEGER); INSERT INTO HistoryNameAnchoringPoints VALUES (100, 10), (101, 10);")?;
        let steps = vec![Step { node: 10, name: "Import".into(), op: "MaterializeImportedBodies".into(), params: vec![] }];
        let items: Vec<Item> = [100, 101]
            .into_iter()
            .map(|name_id| Item { name_id, import_body: None, name: format!("Body {name_id}"), folder: vec![], hidden: false, color: None })
            .collect();
        let mut bodies: Vec<Body> = [20, 21]
            .into_iter()
            .map(|partition| Body {
                partition: Some(partition),
                import: None,
                folder: vec![],
                step: None,
                name: None,
                item_name_id: None,
                brep: Brep::default(),
                color: None,
                hidden: false,
                mesh: None,
            })
            .collect();
        let calls = BTreeMap::from([(10, vec![20, 21])]);
        name_from_calls(&c, &steps, &items, &[], &Default::default(), &mut bodies, &calls)?;
        assert_eq!(bodies.iter().map(|b| (b.step, b.item_name_id)).collect::<Vec<_>>(), vec![(Some(10), Some(100)), (Some(10), Some(101))]);
        Ok(())
    }
    #[test]
    fn latest_metadata_assignment_updates_item_name_and_visibility() -> Result<()> {
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch(
            r#"CREATE TABLE MetadataAssignments (NameID INTEGER, MetadataID INTEGER);
            CREATE TABLE Metadata (MetadataID INTEGER, MetadataTypeID INTEGER, Data TEXT);
            CREATE TABLE HistoryNames (NameID INTEGER, Type INTEGER, Data TEXT);
            CREATE TABLE MaterialInstances (InstanceID INTEGER, Properties BLOB);
            INSERT INTO HistoryNames VALUES (10,18,'{"nameToAnchor":30}'), (11,18,'{"nameToAnchor":30}');
            INSERT INTO Metadata VALUES (1,0,'Old'), (2,1,'1'), (3,0,'New'), (4,1,'0');
            INSERT INTO MetadataAssignments VALUES (10,1), (10,2), (10,3), (10,4);"#,
        )?;
        let mut by_meta = std::collections::BTreeMap::new();
        let result = items(&c, &[], &mut by_meta)?;
        let old_anchor = result.iter().find(|i| i.name_id == 10).ok_or_else(|| Error::Format("missing item".into()))?;
        assert_eq!(old_anchor.name, "New");
        assert!(!old_anchor.hidden);
        Ok(())
    }

    #[test]
    fn late_topology_identity_replaces_inherited_item() -> Result<()> {
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch(
            r#"CREATE TABLE HistoryNameAnchoringPoints (AnchoredNameID INTEGER, HistoryTreeNodeID INTEGER);
            CREATE TABLE HistoryNames (NameID INTEGER, Type INTEGER, Data TEXT);
            CREATE TABLE MetadataAssignments (NameID INTEGER, MetadataID INTEGER);
            CREATE TABLE Metadata (MetadataID INTEGER, MetadataTypeID INTEGER, Data TEXT);
            CREATE TABLE PersistedCalls (CallKey BLOB, CallData BLOB);
            INSERT INTO HistoryNames VALUES (10,18,'{"nameToAnchor":50}'), (11,18,'{"nameToAnchor":50}'),
                (12,19,'{"wrappedName":50}');
            INSERT INTO Metadata VALUES (1,0,'Current'), (2,1,'1'), (3,1,'0');
            INSERT INTO MetadataAssignments VALUES (10,1), (10,2), (11,3);"#,
        )?;
        let encode = |v: &Value| -> Result<Vec<u8>> {
            let mut bytes = Vec::new();
            rmpv::encode::write_value(&mut bytes, v).map_err(|e| Error::Format(e.to_string()))?;
            Ok(bytes)
        };
        let key = Value::Array(vec![Value::from(20), Value::Array(vec![])]);
        let data = Value::Array(vec![
            Value::Nil,
            Value::Nil,
            Value::Array(vec![
                Value::from(true),
                Value::Array(vec![
                    Value::Nil,
                    Value::Array(vec![Value::Array(vec![Value::from(100), Value::Array(vec![Value::from(true)])])]),
                ]),
            ]),
        ]);
        c.execute("INSERT INTO PersistedCalls VALUES (?1, ?2)", rusqlite::params![encode(&key)?, encode(&data)?])?;
        let step = Step {
            node: 20,
            name: "Rotate".into(),
            op: "Rotate".into(),
            params: vec![(1, Value::Array(vec![Value::from(7), Value::from(12)]))],
        };
        let item = Item { name_id: 10, import_body: None, name: "Current".into(), folder: vec![], hidden: true, color: None };
        let mut bodies = vec![Body {
            partition: Some(100),
            import: None,
            folder: vec![],
            step: Some(1),
            name: Some("Inherited".into()),
            item_name_id: None,
            brep: Brep::default(),
            color: None,
            hidden: true,
            mesh: None,
        }];
        resolve_transformed_items(&c, &[step], std::slice::from_ref(&item), &[], &Default::default(), &mut bodies)?;
        assert_eq!(bodies[0].name.as_deref(), Some("Current"));
        assert!(!bodies[0].hidden);
        c.execute_batch(
            r#"INSERT INTO HistoryNames VALUES
            (60,1,'{"callKeyPrefix":{"nodeID":21},"qualifier":4}'), (61,18,'{"nameToAnchor":60}');
            INSERT INTO Metadata VALUES (4,0,'Mirror'), (5,1,'1');
            INSERT INTO MetadataAssignments VALUES (61,4), (61,5);"#,
        )?;
        let key = Value::Array(vec![Value::from(21), Value::Array(vec![])]);
        let data = Value::Array(vec![
            Value::Nil,
            Value::Nil,
            Value::Array(vec![
                Value::from(true),
                Value::Array(vec![
                    Value::Nil,
                    Value::Array(vec![
                        Value::Array(vec![Value::from(100), Value::Array(vec![Value::from(false)])]),
                        Value::Array(vec![Value::from(101), Value::Array(vec![Value::from(true)])]),
                    ]),
                ]),
            ]),
        ]);
        c.execute("INSERT INTO PersistedCalls VALUES (?1, ?2)", rusqlite::params![encode(&key)?, encode(&data)?])?;
        let mirror = Step {
            node: 21,
            name: "Mirror".into(),
            op: "Mirror".into(),
            params: vec![(1, Value::Array(vec![Value::from(7), Value::from(12)]))],
        };
        let mut copy = bodies[0].clone();
        copy.partition = Some(101);
        bodies.push(copy);
        let output = Item { name_id: 61, import_body: None, name: "Mirror".into(), folder: vec![], hidden: true, color: None };
        resolve_transformed_items(&c, &[mirror], &[item, output], &[], &Default::default(), &mut bodies)?;
        assert_eq!((bodies[0].name.as_deref(), bodies[0].hidden), (Some("Current"), false));
        assert_eq!((bodies[1].name.as_deref(), bodies[1].hidden), (Some("Mirror"), true));

        c.execute_batch(
            r#"INSERT INTO HistoryNames VALUES (70,5,'{"originals":[72]}'),
            (71,18,'{"callKey":{"nodeID":22},"nameToAnchor":70}'),
            (72,0,'{"sourceTopologyNames":[50]}'), (80,5,'{"originals":[999]}'),
            (81,18,'{"callKey":{"nodeID":22},"nameToAnchor":80}');
            INSERT INTO Metadata VALUES (6,0,'Boolean result'), (7,1,'1'), (8,0,'Other output');
            INSERT INTO MetadataAssignments VALUES (71,6), (71,7), (81,8);"#,
        )?;
        let entry = |pid, after| {
            Value::Array(vec![
                Value::from(pid),
                Value::Array(vec![
                    Value::from(true),
                    Value::Array(vec![Value::Array(vec![Value::from(true), Value::from(1)]), Value::Array(vec![Value::from(after)])]),
                ]),
            ])
        };
        let data = Value::Array(vec![
            Value::Nil,
            Value::Nil,
            Value::Array(vec![
                Value::from(true),
                Value::Array(vec![Value::Nil, Value::Array(vec![entry(200, true), entry(201, true), entry(202, false)])]),
            ]),
        ]);
        let key = Value::Array(vec![Value::from(22), Value::Array(vec![])]);
        c.execute("INSERT INTO PersistedCalls VALUES (?1, ?2)", rusqlite::params![encode(&key)?, encode(&data)?])?;
        let boolean = Step {
            node: 22,
            name: "Intersect".into(),
            op: "Intersect".into(),
            params: vec![(1, Value::Array(vec![Value::from(7), Value::from(12)]))],
        };
        let mut boolean_bodies: Vec<_> = [200, 201, 202]
            .into_iter()
            .map(|pid| {
                let mut body = bodies[0].clone();
                body.partition = Some(pid);
                body
            })
            .collect();
        let result = Item { name_id: 71, import_body: None, name: "Boolean result".into(), folder: vec![], hidden: true, color: None };
        let input = Item { name_id: 10, import_body: None, name: "Current".into(), folder: vec![], hidden: false, color: None };
        resolve_transformed_items(
            &c,
            std::slice::from_ref(&boolean),
            &[input, result.clone()],
            &[],
            &std::collections::HashSet::from([201, 202]),
            &mut boolean_bodies,
        )?;
        assert_eq!((boolean_bodies[0].name.as_deref(), boolean_bodies[0].hidden), (Some("Current"), false));
        assert_eq!(boolean_bodies[1].name.as_deref(), Some("Current"));
        c.execute_batch(
            "INSERT INTO Metadata VALUES (9,0,'Renamed'), (10,1,'0'); INSERT INTO MetadataAssignments VALUES (11,9), (11,10);",
        )?;
        let renamed = Item { name_id: 11, import_body: None, name: "Renamed".into(), folder: vec![], hidden: false, color: None };
        resolve_transformed_items(
            &c,
            &[boolean],
            &[renamed, result],
            &[],
            &std::collections::HashSet::from([201, 202]),
            &mut boolean_bodies,
        )?;
        assert_eq!((boolean_bodies[0].name.as_deref(), boolean_bodies[0].hidden), (Some("Renamed"), false));
        Ok(())
    }
}

#[cfg(test)]
mod older_layout_tests {
    use super::*;

    /// A workspace database built from SQL, as file bytes.
    fn workspace(sql: &str) -> Vec<u8> {
        let dir = std::env::temp_dir().join(format!("shapr-older-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        let c = rusqlite::Connection::open(&dir).unwrap();
        c.execute_batch(sql).unwrap();
        drop(c);
        let bytes = std::fs::read(&dir).unwrap();
        let _ = std::fs::remove_file(&dir);
        bytes
    }

    #[test]
    fn a_bare_database_and_a_web_page_are_told_apart() {
        let db = workspace("CREATE TABLE Settings (SettingName BLOB, SettingValue BLOB);");
        assert!(ShaprFile::parse(&db).is_ok_and(|f| f.workspace == db));
        let page = ShaprFile::parse(b"<!DOCTYPE html><html>sign in</html>").unwrap_err().to_string();
        assert!(page.contains("web page"), "{page}");
    }

    /// The first releases' layout: text ids, typed curve tables with points in meters, shape groups, no Hidden.
    #[test]
    fn the_first_releases_layout_reads() -> Result<()> {
        let db = workspace(
            "CREATE TABLE Settings (SettingName BLOB, SettingValue BLOB);
             CREATE TABLE Shapes (ShapeID BLOB NOT NULL PRIMARY KEY, ShapeData BLOB, ShapeGroupID BLOB);
             CREATE TABLE ShapeGroups (ShapeGroupID BLOB NOT NULL PRIMARY KEY, ShapeGroupName BLOB, Hidden INT, OrderInList INT);
             CREATE TABLE ConstructionGeometryPlanes (GeometryID BLOB NOT NULL PRIMARY KEY, Name BLOB, IsHidden BOOL, CenterXPosition DOUBLE,
               CenterYPosition DOUBLE, CenterZPosition DOUBLE, NormalXValue DOUBLE, NormalYValue DOUBLE, NormalZValue DOUBLE, Size DOUBLE, OrderInList INT);
             CREATE TABLE SketchControllers (SketchID BLOB NOT NULL PRIMARY KEY, Name BLOB, IsHidden BOOL, OrderInList INT, PlaneCenterX DOUBLE,
               PlaneCenterY DOUBLE, PlaneCenterZ DOUBLE, PlaneNormX DOUBLE, PlaneNormY DOUBLE, PlaneNormZ DOUBLE, PlaneUDirX DOUBLE, PlaneUDirY DOUBLE, PlaneUDirZ DOUBLE);
             CREATE TABLE SketchCurves (CurveID BLOB NOT NULL PRIMARY KEY, SketchID BLOB NOT NULL, CurveType INT);
             CREATE TABLE ConstraintPoints (PointID BLOB NOT NULL PRIMARY KEY, SketchID BLOB, XPosition DOUBLE, YPosition DOUBLE, Type INTEGER);
             CREATE TABLE SketchLines (LineID BLOB NOT NULL PRIMARY KEY, StartPointID BLOB NOT NULL, EndPointID BLOB NOT NULL);
             CREATE TABLE SketchCircles (CircleID BLOB NOT NULL PRIMARY KEY, CenterPointID BLOB NOT NULL, StartPointID BLOB NOT NULL);
             CREATE TABLE SketchArcs (ArcID BLOB NOT NULL PRIMARY KEY, StartPointID BLOB, MidPointID BLOB, EndPointID BLOB, CenterPointID BLOB,
               RadiusID BLOB, StartAngleID BLOB, EndAngleID BLOB);
             INSERT INTO ShapeGroups VALUES (x'6731', 'Group of shapes', 0, 0);
             INSERT INTO ConstructionGeometryPlanes VALUES (x'7031', 'Plane', 0, 0, 0, 0.01, 0, 0, 1, 0.05, 0);
             INSERT INTO SketchControllers VALUES (x'6b31', 'Sketch', 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0);
             INSERT INTO SketchCurves VALUES (x'6c31', x'6b31', 0), (x'6331', x'6b31', 2), (x'6131', x'6b31', 1);
             INSERT INTO ConstraintPoints VALUES (x'01', x'6b31', 0, 0, 0), (x'02', x'6b31', 0.01, 0, 0), (x'03', x'6b31', 0.002, 0.003, 0),
               (x'04', x'6b31', 0.005, 0.003, 0), (x'05', x'6b31', 0.01, 0.01, 0), (x'06', x'6b31', 0, 0.01, 0), (x'07', x'6b31', -0.00707106781, -0.00707106781, 0);
             INSERT INTO SketchLines VALUES (x'6c31', x'01', x'02');
             INSERT INTO SketchCircles VALUES (x'6331', x'03', x'04');
             INSERT INTO SketchArcs VALUES (x'6131', x'02', x'07', x'06', x'01', x'00', x'00', x'00');",
        );
        let p = Project::read(&ShaprFile::parse(&db)?)?;
        assert!(p.legacy && !p.patchable);
        assert_eq!(p.sketches.len(), 1);
        let curves = &p.sketches[0].curves;
        assert!(
            curves.contains(&SketchCurve::Line { id: (1 << 40) + 1, a: [0.0, 0.0], b: [10.0, 0.0], construction: false }),
            "{curves:?}"
        );
        let circle = curves.iter().find_map(|c| match c {
            SketchCurve::Circle { center, radius, .. } => Some((*center, *radius)),
            _ => None,
        });
        assert!(
            circle.is_some_and(|(c, r)| (c[0] - 2.0).abs() < 1e-9 && (c[1] - 3.0).abs() < 1e-9 && (r - 3.0).abs() < 1e-9),
            "{circle:?}"
        );
        // The arc from (10, 0) through (-7.07, -7.07) to (0, 10) goes the long way, clockwise: kept counter-clockwise
        // from (0, 10) to (10, 0).
        let arc = curves.iter().find_map(|c| match c {
            SketchCurve::Arc { start, end, .. } => Some((*start, *end)),
            _ => None,
        });
        assert_eq!(arc, Some(([0.0, 10.0], [10.0, 0.0])));
        assert!(matches!(p.construction.as_slice(), [Construction::Plane { origin, .. }] if (origin[2] - 10.0).abs() < 1e-9));
        assert!(p.folders.iter().any(|f| f.name == "Group of shapes"));
        Ok(())
    }

    /// Mid-2024 to early-2026 files: no cached calls, deleted partition rows and empty blocks are skipped.
    /// #45: which bodies a move places is in the call itself. An import of three; the first moved, the second and third
    /// moved together, the second again; the first copied, the copy moved on; the second rebuilt by a later step; the
    /// third deleted.
    #[test]
    fn moves_copies_and_deletes_follow_the_calls_tags() -> Result<()> {
        let no = || Value::Array(vec![Value::from(false)]);
        let yes = |v: Value| Value::Array(vec![Value::from(true), v]);
        let arr = Value::Array;
        let int = |i: i64| Value::from(i);
        let up = |mm: f64| {
            let mut m: Vec<f64> = glam::DMat4::IDENTITY.to_cols_array().to_vec();
            m[14] = mm / 1000.0;
            arr(m.into_iter().map(Value::from).collect())
        };
        let entry = |tag: i64, from: Option<i64>, matrix: Option<i64>| {
            let given = |x: Option<i64>| x.map_or_else(no, |i| yes(int(i)));
            arr(vec![int(tag), given(from), given(matrix)])
        };
        let call = |modified: Value, placed: Value, created: Value| arr(vec![int(0), arr(vec![int(0)]), modified, placed, no(), created]);
        let none = || yes(arr(vec![arr(vec![]), arr(vec![]), arr(vec![])]));
        let places = |m: Vec<Value>, e: Vec<Value>| call(none(), yes(arr(vec![arr(m), arr(e)])), no());
        let created = yes(arr(vec![arr((0..3).map(|i| arr(vec![int(i), int(10 + i), int(3), int(i)])).collect())]));
        let after = yes(arr(vec![yes(int(6)), yes(int(7))]));
        let rebuilt = yes(arr(vec![arr(vec![]), arr(vec![arr(vec![int(11), after])]), arr(vec![])]));
        let data = vec![
            call(no(), no(), created),
            places(vec![up(20.0)], vec![entry(0, None, Some(0))]),
            places(vec![up(5.0), up(6.0)], vec![entry(1, None, Some(0)), entry(2, None, Some(1))]),
            places(vec![up(17.0)], vec![entry(1, None, Some(0))]),
            places(vec![up(3.0)], vec![entry(7, Some(0), Some(0))]),
            call(rebuilt, no(), no()),
            places(vec![up(1.0)], vec![entry(1, None, Some(0))]),
            places(vec![], vec![entry(2, None, None)]),
        ];
        let steps: Vec<Step> =
            (0..8).map(|i| Step { node: 100 + i, name: String::new(), op: "Transform".into(), params: vec![] }).collect();
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE PersistedCalls (CallKey BLOB, CallData BLOB)")?;
        for (s, d) in steps.iter().zip(&data) {
            let key = pack(&arr(vec![int(s.node), arr(vec![int(0)])]));
            c.execute("INSERT INTO PersistedCalls VALUES (?1, ?2)", (key, pack(d)))?;
        }
        let gone = drop_removed(&c, &steps, &mut Vec::new())?;
        assert_eq!(gone.into_iter().collect::<Vec<_>>(), [12], "only the deleted one, not the moved ones");
        let placed = instances(&ordered_calls(&c, &steps)?, &[10, 11, 12].into_iter().collect());
        let z = |tag: i64| placed.get(&tag).map(|i| (i.partition, (i.motion.w_axis.z * 1000.0).round() / 1000.0, i.copied_by));
        assert_eq!(z(0), Some((10, 20.0, None)));
        assert_eq!(z(1), Some((11, 1.0, None)), "the moves before it was rebuilt are in its shape already");
        assert_eq!(z(2), None);
        assert_eq!(z(7), Some((10, 23.0, Some(104))), "the copy starts where its source is");
        assert_eq!(placed.len(), 3);
        Ok(())
    }

    /// A later step that uses a copy up gives it a partition of its own without saying whose it was.
    #[test]
    fn a_copy_a_later_step_used_up_is_not_left_standing() {
        let int = |i: i64| Value::from(i);
        let arr = Value::Array;
        let yes = |v: Value| Value::Array(vec![Value::from(true), v]);
        let no = || Value::Array(vec![Value::from(false)]);
        let copy = yes(arr(vec![arr(vec![]), arr(vec![arr(vec![int(4), yes(int(1)), no()])])]));
        let copy = arr(vec![int(0), Value::Nil, no(), copy, no(), no()]);
        let used = |pid: i64, after: Value| arr(vec![int(pid), yes(arr(vec![yes(int(4)), after]))]);
        let union = yes(arr(vec![arr(vec![]), arr(vec![used(3086, yes(int(6))), used(3087, no())]), arr(vec![])]));
        let union = arr(vec![int(0), Value::Nil, union, no(), no(), no()]);
        let step = |node: i64, op: &str| Step { node, name: String::new(), op: op.into(), params: vec![] };
        let (pattern, join) = (step(1, "LinearPattern"), step(2, "Union"));
        let known = [1].into_iter().collect();
        assert_eq!(instances(&[(&pattern, copy.clone())], &known).len(), 2);
        assert_eq!(instances(&[(&pattern, copy), (&join, union)], &known).len(), 1);
    }

    /// Where a file names no tags, the tag is the partition; junk is passed over.
    #[test]
    fn a_tag_the_file_doesnt_name_is_the_partition() {
        let m = glam::DMat4::from_translation(glam::DVec3::new(0.0, 0.0, 0.007)).to_cols_array();
        let m = Value::Array(m.into_iter().map(Value::from).collect());
        let yes = |v: Value| Value::Array(vec![Value::from(true), v]);
        let no = || Value::Array(vec![Value::from(false)]);
        let entry = |tag: i64, i: i64| Value::Array(vec![Value::from(tag), no(), yes(Value::from(i))]);
        let placed = yes(Value::Array(vec![
            Value::Array(vec![m]),
            Value::Array(vec![entry(6199, 0), entry(6199, 9), entry(6199, -1), Value::from(3), entry(5, 0)]),
        ]));
        let call = Value::Array(vec![Value::from(0), Value::Nil, no(), placed, no(), no()]);
        let step = Step { node: 1, name: String::new(), op: "Transform".into(), params: vec![] };
        let all = instances(&[(&step, call)], &[6199].into_iter().collect());
        assert_eq!(all.len(), 1);
        assert!((all[&6199].motion.w_axis.z - 7.0).abs() < 1e-9);
    }

    #[test]
    fn deleted_partition_rows_are_not_bodies() -> Result<()> {
        let c = rusqlite::Connection::open_in_memory()?;
        c.execute_batch(
            "CREATE TABLE BodyRevisionPartitions (PartitionID INTEGER NOT NULL, Block BLOB, Deltas BLOB, Phase INTEGER NOT NULL DEFAULT 0,
               IsDeleted BOOL NOT NULL DEFAULT 0);
             INSERT INTO BodyRevisionPartitions VALUES (1, NULL, NULL, 0, 1), (2, NULL, NULL, 0, 1);",
        )?;
        let (bodies, unreadable) = current_bodies(&c, &FacePaint::default())?;
        assert!(bodies.is_empty() && unreadable.is_empty(), "{unreadable:?}");
        assert!(drop_removed(&c, &[], &mut Vec::new())?.is_empty());
        Ok(())
    }
}
