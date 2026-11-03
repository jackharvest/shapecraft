//! Writing `.shapr` projects: a fresh workspace, or an existing one patched with new history steps.
//!
//! Shapr3D rebuilds a project from its history when the cached partitions, deltas and calls are missing, so a
//! writer only adds steps: `MaterializeImportedBodies` for a body (a Parasolid part), `Delete` for one that
//! changed or went away. Steps Shapr3D made stay as they are, parametric.

use std::time::{SystemTime, UNIX_EPOCH};

use rmpv::Value;

use crate::{Db, Error, Result, ShaprFile, Sketch, SketchCurve, pack, unpack};

const SCHEMA: &str = include_str!("schema.sql");
/// Version stamps a workspace carries (as observed); Shapr3D upgrades older workspaces itself.
const HISTORY_VERSION: i64 = 106;
/// Every key must be there or Shapr3D refuses the file (observed); build hashes may be zeros.
const SETTINGS: &[(&str, &str)] = &[
    ("WorkspaceSchemaVersion", "278"),
    ("Create_ParasolidVersion", "39.0.161"),
    ("Create_SolverType", "dcm"),
    ("Create_SolverVersion", "79.0.0"),
    ("Create_CommitHash", "0000000000000000000000000000000000000000"),
    ("SchemaVersion", "319000"),
    ("HistoryVersion", "106"),
    ("CurrentVisualEnvironmentInstanceID", "1"),
    ("Persistence_CommitHash", "0000000000000000000000000000000000000000"),
    ("Persistence_ProjectVersion", "319000"),
    ("Latest_ParasolidVersion", "39.0.161"),
    ("Latest_SolverType", "dcm"),
    ("Latest_SolverVersion", "79.0.0"),
    ("Latest_CommitHash", "0000000000000000000000000000000000000000"),
];

/// Shapr3D's default grey, as the RGBA its material instances start with.
const DEFAULT_RGBA: u32 = 0xA0A0_A4FF;

/// A material instance's properties: the base colour (RGBA) and the rest as Shapr3D's default material has them.
fn material(rgba: u32) -> Value {
    Value::Array(vec![
        Value::from(rgba),
        Value::F64(1337.0),
        Value::F64(1337.0),
        Value::F64(1337.0),
        Value::F64(1337.0),
        Value::Boolean(false),
        Value::Array(vec![Value::F64(0.0), Value::F64(0.0), Value::F64(0.0)]),
    ])
}

fn list(v: Vec<Value>) -> Value {
    Value::Array(v)
}

/// A parameter node's value: one tagged value, wrapped twice ([[[tag, value]]]).
fn param(tagged: Value) -> Value {
    list(vec![list(vec![tagged])])
}

/// Where a Move/Rotate frame sits: at a named topology's centre, or at a position (m; the target frame's is an
/// offset from the start frame).
enum Centre {
    Of(i64),
    At([f64; 3]),
}

/// A gizmo frame, z up and x as the reference direction, as Move/Rotate keeps one.
fn frame(centre: Centre) -> Value {
    let tagged = |fields: Vec<(&str, Value)>| {
        list(vec![list(vec![Value::from(4), list(fields.into_iter().flat_map(|(k, v)| [Value::from(k), v]).collect())])])
    };
    let vector = |v: [f64; 3]| list(vec![list(vec![Value::from(5), list(v.into_iter().map(Value::F64).collect())])]);
    let kind = |k: i64| list(vec![list(vec![Value::from(2), Value::from(k)])]);
    let direction = |v: [f64; 3]| tagged(vec![("coordinates", vector(v)), ("type", kind(0))]);
    list(vec![tagged(vec![
        ("axis", direction([0.0, 0.0, 1.0])),
        (
            "center",
            match centre {
                Centre::Of(name) => tagged(vec![
                    (
                        "topologiesArray",
                        list(vec![list(vec![Value::from(9), list(vec![list(vec![list(vec![Value::from(7), Value::from(name)])])])])]),
                    ),
                    ("type", kind(12)),
                ]),
                Centre::At(p) => tagged(vec![("position", vector(p)), ("type", kind(3))]),
            },
        ),
        ("referenceDirection", direction([1.0, 0.0, 0.0])),
    ])])
}

/// How a body shows in Shapr3D: its colour (`None` for the default grey) and whether it's hidden.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    pub color: Option<[f32; 3]>,
    pub hidden: bool,
}

/// A colour (0–1 per channel) as an opaque RGBA word; out-of-range or NaN channels clamp.
pub fn rgba(c: [f32; 3]) -> u32 {
    let byte = |x: f32| if x.is_finite() { (x.clamp(0.0, 1.0) * 255.0).round() as u32 } else { 0 };
    byte(c[0]) << 24 | byte(c[1]) << 16 | byte(c[2]) << 8 | 0xFF
}

pub struct Writer {
    db: Db,
    next_node: i64,
    next_name: i64,
    next_meta: i64,
    next_import: i64,
    seq: u64,
}

fn max_id(db: &Db, table: &str, column: &str) -> Result<i64> {
    Ok(db.conn.query_row(&format!("SELECT COALESCE(MAX(\"{column}\"), 0) FROM \"{table}\""), [], |r| r.get::<_, i64>(0))?)
}

fn json(v: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&v).unwrap_or_default()
}

/// Seconds since 1970 as an ISO date-time ("2026-10-29T06:52:56Z").
fn iso_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since the epoch (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem / 60 % 60, rem % 60)
}

impl Writer {
    /// An empty project.
    pub fn fresh() -> Result<Self> {
        let db = Db::from_bytes(&[])?;
        db.conn.execute_batch(SCHEMA)?;
        db.conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        for (k, v) in SETTINGS {
            // Numbers are stored as integers, like Shapr3D does.
            match v.parse::<i64>() {
                Ok(n) => db.conn.execute("INSERT INTO Settings (SettingName, SettingValue) VALUES (?1, ?2)", (k, n))?,
                Err(_) => db.conn.execute("INSERT INTO Settings (SettingName, SettingValue) VALUES (?1, ?2)", (k, v))?,
            };
        }
        for key in ["Create_Date", "Latest_Date", "Persistence_SaveTimestamp"] {
            db.conn.execute("INSERT INTO Settings (SettingName, SettingValue) VALUES (?1, ?2)", (key, iso_now()))?;
        }
        // Default look: the grey material and the studio environment.
        db.conn.execute("INSERT INTO MaterialInstances VALUES (1, 0, ?1)", [pack(&material(DEFAULT_RGBA))])?;
        let env = Value::Array(vec![Value::from(0), Value::F64(0.0), Value::F64(56.5), Value::F64(1.0)]);
        db.conn.execute("INSERT INTO VisualEnvironmentInstances VALUES (1, 0, ?1)", [pack(&env)])?;
        db.conn.execute(
            "INSERT INTO HistoryTreeNodes VALUES (1, 0, ?1)",
            [pack(&Value::Array(vec![Value::from("<root>"), Value::Array(Vec::new())]))],
        )?;
        let mut w = Self { db, next_node: 2, next_name: 1, next_meta: 1, next_import: 0, seq: 0 };
        let nonce = w.uuid();
        w.db.conn.execute("INSERT INTO Settings (SettingName, SettingValue) VALUES ('ProjectStateNonce', ?1)", [nonce])?;
        let id = w.folder_id();
        w.db.conn.execute(
            "INSERT INTO HistoryFolders (FolderID, FolderName, Hidden, NamedChildren) VALUES (?1, 'root', 0, ?2)",
            (id, String::from_utf8_lossy(&json(serde_json::json!({ "elements": [] }))).into_owned()),
        )?;
        Ok(w)
    }

    /// A project Shapr3D wrote, ready for more steps. Its caches go, so Shapr3D replays the whole history.
    pub fn patch(base: &ShaprFile) -> Result<Self> {
        let db = base.db()?;
        db.conn.execute_batch("DELETE FROM BodyRevisionBlocks; DELETE FROM BodyRevisionDeltas; DELETE FROM PersistedCalls;")?;
        adopt_old_stash(&db.conn)?;
        let next_node = max_id(&db, "HistoryTreeNodes", "HistoryTreeNodeID")? + 1;
        let next_name = max_id(&db, "HistoryNames", "NameID")? + 1;
        let next_meta = max_id(&db, "Metadata", "MetadataID")? + 1;
        let next_import = (max_id(&db, "HistoryImportedPrototypes", "ImportedPrototypeID")? + 1)
            .max(max_id(&db, "HistoryImportedBodies", "ImportedBodyID")? + 1);
        Ok(Self { db, next_node, next_name, next_meta, next_import, seq: 0 })
    }

    fn folder_id(&mut self) -> String {
        // 16 random-looking bytes (splitmix64 over the clock and a counter).
        self.seq += 1;
        let mut x = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
            ^ self.seq.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut out = String::new();
        for _ in 0..2 {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            out.push_str(&format!("{z:016x}"));
        }
        out
    }

    fn uuid(&mut self) -> String {
        let id = self.folder_id();
        format!("{}-{}-4{}-a{}-{}", &id[0..8], &id[8..12], &id[13..16], &id[17..20], &id[20..32])
    }

    fn node(&mut self, ty: i64, props: Value) -> Result<i64> {
        let id = self.next_node;
        self.next_node += 1;
        self.db.conn.execute("INSERT INTO HistoryTreeNodes VALUES (?1, ?2, ?3)", (id, ty, pack(&props)))?;
        Ok(id)
    }

    fn name(&mut self, ty: i64, data: serde_json::Value) -> Result<i64> {
        let id = self.next_name;
        self.next_name += 1;
        self.db.conn.execute("INSERT INTO HistoryNames VALUES (?1, ?2, ?3, ?4)", (id, ty, json(data), HISTORY_VERSION))?;
        Ok(id)
    }

    fn meta(&mut self, ty: i64, data: &str, name: i64) -> Result<i64> {
        let id = self.next_meta;
        self.next_meta += 1;
        self.db.conn.execute("INSERT INTO Metadata VALUES (?1, ?2, ?3)", (id, ty, data))?;
        self.db.conn.execute("INSERT INTO MetadataAssignments (NameID, MetadataID) VALUES (?1, ?2)", (name, id))?;
        Ok(id)
    }

    /// Appends a step to the root's list.
    fn append_step(&mut self, step: i64) -> Result<()> {
        let (id, props): (i64, Vec<u8>) = self.db.conn.query_row(
            "SELECT HistoryTreeNodeID, Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeType = 0 LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut v = unpack(&props).ok_or_else(|| Error::Format("unreadable history root".into()))?;
        match &mut v {
            Value::Array(a) => match a.get_mut(1) {
                Some(Value::Array(children)) => children.push(Value::from(step)),
                _ => return Err(Error::Format("history root without a step list".into())),
            },
            _ => return Err(Error::Format("history root is not a list".into())),
        }
        self.db.conn.execute("UPDATE HistoryTreeNodes SET Properties = ?1 WHERE HistoryTreeNodeID = ?2", (pack(&v), id))?;
        Ok(())
    }

    /// Adds `child` (folder id, type 0, or item metadata id, type 6) to the folder at `path` (made as needed).
    fn file_under(&mut self, path: &[String], child: &str, ty: i64) -> Result<()> {
        let mut parent: String =
            self.db.conn.query_row("SELECT FolderID FROM HistoryFolders WHERE FolderName = 'root' LIMIT 1", [], |r| r.get(0))?;
        for name in path {
            let children = self.children(&parent)?;
            let existing = children.iter().filter(|(_, t)| *t == 0).find_map(|(id, _)| {
                let n: Option<String> =
                    self.db.conn.query_row("SELECT FolderName FROM HistoryFolders WHERE FolderID = ?1", [id], |r| r.get(0)).ok();
                (n.as_deref() == Some(name.as_str())).then(|| id.clone())
            });
            parent = match existing {
                Some(id) => id,
                None => {
                    let id = self.folder_id();
                    self.db.conn.execute(
                        "INSERT INTO HistoryFolders (FolderID, FolderName, Hidden, NamedChildren) VALUES (?1, ?2, 0, '{\"elements\":[]}')",
                        (&id, name),
                    )?;
                    let mut ch = children;
                    ch.push((id.clone(), 0));
                    self.set_children(&parent, &ch)?;
                    id
                }
            };
        }
        let mut ch = self.children(&parent)?;
        ch.push((child.to_string(), ty));
        self.set_children(&parent, &ch)
    }

    fn children(&self, folder: &str) -> Result<Vec<(String, i64)>> {
        let text: String =
            self.db.conn.query_row("SELECT NamedChildren FROM HistoryFolders WHERE FolderID = ?1", [folder], |r| r.get(0))?;
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        Ok(v["elements"]
            .as_array()
            .map(|a| a.iter().map(|e| (e["id"].as_str().unwrap_or("").to_string(), e["type"].as_i64().unwrap_or(0))).collect())
            .unwrap_or_default())
    }

    fn set_children(&self, folder: &str, ch: &[(String, i64)]) -> Result<()> {
        let elements: Vec<serde_json::Value> = ch.iter().map(|(id, t)| serde_json::json!({ "id": id, "type": t })).collect();
        let text = String::from_utf8_lossy(&json(serde_json::json!({ "elements": elements }))).into_owned();
        self.db.conn.execute("UPDATE HistoryFolders SET NamedChildren = ?1 WHERE FolderID = ?2", (text, folder))?;
        Ok(())
    }

    fn step_count(&self, op: &str) -> Result<usize> {
        let mut q = self.db.conn.prepare("SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeType = 2")?;
        let rows = q.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        Ok(rows
            .flatten()
            .filter_map(|b| unpack(&b))
            .filter(|v| v.as_array().and_then(|a| a.get(3)).and_then(Value::as_str) == Some(op))
            .count())
    }

    /// The material instance with colour `color` (the default grey for `None`), added when there's none yet.
    fn material_for(&mut self, color: Option<[f32; 3]>) -> Result<i64> {
        let want = color.map_or(DEFAULT_RGBA, rgba);
        let rows: Vec<(i64, Vec<u8>)> = {
            let mut q = self.db.conn.prepare("SELECT InstanceID, Properties FROM MaterialInstances ORDER BY InstanceID")?;
            q.query_map([], |r| Ok((r.get(0)?, crate::blob(r.get(1)?))))?.flatten().collect()
        };
        let first = |v: &Value| v.as_array().and_then(|a| a.first()).and_then(Value::as_u64);
        if let Some((id, _)) = rows.iter().find(|(_, p)| unpack(p).as_ref().and_then(first) == Some(u64::from(want))) {
            return Ok(*id);
        }
        // A copy of the default material (Shapr3D's own when the project has it) in the new colour.
        let mut props = rows
            .iter()
            .find(|(id, _)| *id == 1)
            .and_then(|(_, p)| unpack(p))
            .filter(|v| first(v).is_some())
            .unwrap_or_else(|| material(DEFAULT_RGBA));
        if let Value::Array(a) = &mut props
            && let Some(c) = a.first_mut()
        {
            *c = Value::from(want);
        }
        let id = rows.iter().map(|r| r.0).max().unwrap_or(0) + 1;
        self.db.conn.execute("INSERT INTO MaterialInstances VALUES (?1, 0, ?2)", (id, pack(&props)))?;
        Ok(id)
    }

    /// A body as an imported Parasolid part, named, coloured, shown or hidden and filed in the Items list.
    /// Returns the step's node id.
    pub fn import_body(&mut self, name: &str, xt: &[u8], folder: &[String], look: Look) -> Result<i64> {
        let body = self.next_import;
        self.next_import += 1;
        self.db.conn.execute("INSERT INTO HistoryImportedPrototypes VALUES (?1, ?2)", (body, xt))?;
        let identity: Vec<Value> = (0..16).map(|i| Value::F64(if i % 5 == 0 { 1.0 } else { 0.0 })).collect();
        self.db.conn.execute("INSERT INTO HistoryImportedBodies VALUES (?1, ?1, ?2)", (body, pack(&Value::Array(identity))))?;
        // [[[9, [[[11, body]]]]]]: the imported body to materialize.
        let arg = Value::Array(vec![Value::Array(vec![Value::Array(vec![
            Value::from(9),
            Value::Array(vec![Value::Array(vec![Value::Array(vec![Value::from(11), Value::from(body)])])]),
        ])])]);
        let param = self.node(3, arg)?;
        let label = format!("Import \"{}\"", name.replace('"', "'"));
        let step = self.node(
            2,
            Value::Array(vec![
                Value::from(HISTORY_VERSION),
                Value::from(HISTORY_VERSION),
                Value::from(label),
                Value::from("MaterializeImportedBodies"),
                Value::Array(vec![Value::from(param)]),
            ]),
        )?;
        self.append_step(step)?;
        let key = serde_json::json!({ "nodeID": step, "entropy": [0] });
        let topo = self.name(11, serde_json::json!({ "bodyID": body, "identifier": null }))?;
        let named = self.name(0, serde_json::json!({ "callKey": key, "qualifier": 4, "sourceTopologyNames": [topo] }))?;
        let anchor = self.name(18, serde_json::json!({ "callKey": key, "nameToAnchor": named }))?;
        let wrapped = self.name(19, serde_json::json!({ "wrappedName": named, "kind": 1 }))?;
        let anchor2 = self.name(18, serde_json::json!({ "callKey": key, "nameToAnchor": wrapped }))?;
        self.db.conn.execute("INSERT INTO HistoryNameAnchoringPoints VALUES (?1, ?2)", (anchor, step))?;
        self.db.conn.execute("INSERT INTO HistoryNameAnchoringPoints VALUES (?1, ?2)", (anchor2, step))?;
        self.meta(0, name, anchor)?;
        self.meta(1, if look.hidden { "1" } else { "0" }, anchor)?;
        let item = self.meta(3, "", anchor)?;
        let instance = self.material_for(look.color)?;
        self.meta(2, &instance.to_string(), anchor2)?;
        self.file_under(folder, &item.to_string(), 6)?;
        Ok(step)
    }

    /// A parameter naming the body step `creator` made: [[[9, [[[7, name]]]]]].
    fn body_param(&mut self, creator: i64) -> Result<i64> {
        let prefix = self.name(1, serde_json::json!({ "callKeyPrefix": { "nodeID": creator, "entropyPrefix": [] }, "qualifier": 4 }))?;
        let wrapped = self.name(19, serde_json::json!({ "wrappedName": prefix, "kind": 0 }))?;
        self.node(3, param(list(vec![Value::from(9), list(vec![list(vec![list(vec![Value::from(7), Value::from(wrapped)])])])])))
    }

    /// A step named after its kind and number ("Delete 02") at the end of the history.
    fn add_step(&mut self, label: &str, op: &str, params: Vec<i64>) -> Result<i64> {
        let n = self.step_count(op)? + 1;
        let props = list(vec![
            Value::from(HISTORY_VERSION),
            Value::from(HISTORY_VERSION),
            Value::from(format!("{label} {n:02}")),
            Value::from(op),
            list(params.into_iter().map(Value::from).collect()),
        ]);
        let step = self.node(2, props)?;
        self.append_step(step)?;
        Ok(step)
    }

    /// Deletes the body made by step `creator` (as Shapr3D's Delete does).
    pub fn delete_body(&mut self, creator: i64) -> Result<()> {
        let body = self.body_param(creator)?;
        self.add_step("Delete", "Delete", vec![body]).map(|_| ())
    }

    /// Moves the body made by step `creator` by `by` (mm) with a Move/Rotate step, as Shapr3D's Move does: the
    /// target frame's position is an offset from the start frame.
    pub fn move_body(&mut self, creator: i64, by: [f64; 3]) -> Result<bool> {
        if by.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
            return Ok(false);
        }
        let exists: bool = self.db.conn.query_row(
            "SELECT COUNT(*) FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1 AND HistoryTreeNodeType = 2",
            [creator],
            |r| r.get::<_, i64>(0),
        )? > 0;
        if !exists {
            return Ok(false);
        }
        // The body as its step's own name for it ([7, wrapper]) and, for the start frame, its centre: the body-level
        // name the step made (qualifier 4), wrapped as an "originals" topology.
        let names: Vec<(i64, i64, serde_json::Value)> = {
            let mut q = self.db.conn.prepare("SELECT NameID, Type, Data FROM HistoryNames WHERE Type IN (0, 1, 19) ORDER BY NameID")?;
            q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, crate::text(r.get(2)?))))?
                .flatten()
                .map(|(id, t, d)| (id, t, serde_json::from_str(&d).unwrap_or_default()))
                .collect()
        };
        let body_name = names
            .iter()
            .find(|(_, t, j)| *t == 0 && j["callKey"]["nodeID"].as_i64() == Some(creator) && j["qualifier"].as_i64() == Some(4))
            .map(|n| n.0);
        let Some(body_name) = body_name else { return Ok(false) };
        let prefix = names
            .iter()
            .filter(|(_, t, j)| *t == 1 && j["callKeyPrefix"]["nodeID"].as_i64() == Some(creator) && j["qualifier"].as_i64() == Some(4))
            .map(|n| n.0)
            .collect::<Vec<_>>();
        let wrapper = names
            .iter()
            .find(|(_, t, j)| *t == 19 && j["kind"].as_i64() == Some(0) && j["wrappedName"].as_i64().is_some_and(|w| prefix.contains(&w)))
            .map(|n| n.0);
        let body = match wrapper {
            Some(w) => {
                self.node(3, param(list(vec![Value::from(9), list(vec![list(vec![list(vec![Value::from(7), Value::from(w)])])])])))?
            }
            None => self.body_param(creator)?,
        };
        let centre = self.name(5, serde_json::json!({ "originals": [body_name] }))?;
        let none = |w: &mut Self| w.node(3, param(list(vec![Value::from(9), list(Vec::new())])));
        let (faces, edges) = (none(self)?, none(self)?);
        let from = self.node(3, frame(Centre::Of(centre)))?;
        let to = self.node(3, frame(Centre::At(by.map(|v| v / 1000.0))))?;
        let copy = self.node(3, param(list(vec![Value::from(1), Value::Boolean(false)])))?;
        self.add_step("Move/Rotate", "Transform", vec![body, faces, edges, from, to, copy])?;
        Ok(true)
    }

    /// Renames the item anchored to step `creator`; false when it has none.
    pub fn rename_item(&mut self, creator: i64, name: &str) -> Result<bool> {
        let n = self.db.conn.execute(
            "UPDATE Metadata SET Data = ?1 WHERE MetadataTypeID = 0 AND MetadataID IN (SELECT a.MetadataID FROM MetadataAssignments a JOIN HistoryNameAnchoringPoints p ON p.AnchoredNameID = a.NameID WHERE p.HistoryTreeNodeID = ?2)",
            (name, creator),
        )?;
        Ok(n > 0)
    }

    /// Shows or hides the item anchored to step `creator` (its hidden flag sits beside its name); false when the
    /// step has no item.
    pub fn set_hidden(&mut self, creator: i64, hidden: bool) -> Result<bool> {
        let flag = if hidden { "1" } else { "0" };
        let n = self.db.conn.execute(
            "UPDATE Metadata SET Data = ?1 WHERE MetadataTypeID = 1 AND MetadataID IN (SELECT a.MetadataID FROM MetadataAssignments a JOIN HistoryNameAnchoringPoints p ON p.AnchoredNameID = a.NameID WHERE p.HistoryTreeNodeID = ?2)",
            (flag, creator),
        )?;
        if n > 0 {
            return Ok(true);
        }
        // Shapr3D's own bodies carry no flag until they're first hidden.
        let anchor: Option<i64> = self
            .db
            .conn
            .query_row(
                "SELECT a.NameID FROM MetadataAssignments a JOIN Metadata m ON m.MetadataID = a.MetadataID JOIN HistoryNameAnchoringPoints p ON p.AnchoredNameID = a.NameID WHERE m.MetadataTypeID = 0 AND p.HistoryTreeNodeID = ?1 LIMIT 1",
                [creator],
                |r| r.get(0),
            )
            .ok();
        let Some(anchor) = anchor else { return Ok(false) };
        self.meta(1, flag, anchor)?;
        Ok(true)
    }

    /// Recolours the body step `creator` made (`None` for the default grey), as Shapr3D does: a new anchor on the
    /// body's material name at the last step, carrying the material. False when the step has no material name.
    pub fn set_color(&mut self, creator: i64, color: Option<[f32; 3]>) -> Result<bool> {
        let names: Vec<(i64, serde_json::Value)> = {
            let mut q = self.db.conn.prepare("SELECT n.NameID, n.Data FROM HistoryNames n JOIN HistoryNameAnchoringPoints p ON p.AnchoredNameID = n.NameID WHERE n.Type = 18 AND p.HistoryTreeNodeID = ?1")?;
            q.query_map([creator], |r| Ok((r.get::<_, i64>(0)?, crate::text(r.get(1)?))))?
                .flatten()
                .map(|(id, t)| (id, serde_json::from_str(&t).unwrap_or_default()))
                .collect()
        };
        // The material name is the anchored target that wraps the body's name as kind 1.
        let target = names.iter().filter_map(|(_, j)| j["nameToAnchor"].as_i64()).find(|t| {
            let data: Option<String> = self
                .db
                .conn
                .query_row("SELECT Data FROM HistoryNames WHERE NameID = ?1 AND Type = 19", [t], |r| Ok(crate::text(r.get(0)?)))
                .ok();
            data.and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok()).is_some_and(|j| j["kind"].as_i64() == Some(1))
        });
        let Some(target) = target else { return Ok(false) };
        let (_, root): (i64, Vec<u8>) = self.db.conn.query_row(
            "SELECT HistoryTreeNodeID, Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeType = 0 LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let last = unpack(&root).and_then(|v| v.as_array()?.get(1)?.as_array()?.last()?.as_i64()).unwrap_or(creator);
        let anchor = self.name(18, serde_json::json!({ "callKey": { "nodeID": last, "entropy": [] }, "nameToAnchor": target }))?;
        self.db.conn.execute("INSERT INTO HistoryNameAnchoringPoints VALUES (?1, ?2)", (anchor, last))?;
        let instance = self.material_for(color)?;
        self.meta(2, &instance.to_string(), anchor)?;
        Ok(true)
    }

    /// Sets an Extrude step's distance (mm) and draft (degrees); false when `node` isn't an Extrude.
    pub fn set_extrude(&mut self, node: i64, distance: f64, draft: f64) -> Result<bool> {
        if !distance.is_finite() || !draft.is_finite() || distance.abs() > 1e9 || draft.abs() >= 90.0 {
            return Ok(false);
        }
        let Ok(props) = self.db.conn.query_row(
            "SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1 AND HistoryTreeNodeType = 2",
            [node],
            |r| r.get::<_, Vec<u8>>(0),
        ) else {
            return Ok(false);
        };
        let Some(step) = unpack(&props) else { return Ok(false) };
        let params: Vec<i64> = step
            .as_array()
            .filter(|a| a.get(3).and_then(Value::as_str) == Some("Extrude"))
            .and_then(|a| a.get(4)?.as_array().map(|p| p.iter().filter_map(Value::as_i64).collect()))
            .unwrap_or_default();
        let (Some(&d), Some(&a)) = (params.get(1), params.get(2)) else { return Ok(false) };
        // [[[12, [[unit, 1], value]]]]: 1 = length (m), 2 = angle (rad).
        let quantity = |unit: i64, v: f64| {
            param(list(vec![Value::from(12), list(vec![list(vec![Value::from(unit), Value::from(1)]), Value::F64(v)])]))
        };
        self.db.conn.execute(
            "UPDATE HistoryTreeNodes SET Properties = ?1 WHERE HistoryTreeNodeID = ?2",
            (pack(&quantity(1, distance / 1000.0)), d),
        )?;
        self.db.conn.execute(
            "UPDATE HistoryTreeNodes SET Properties = ?1 WHERE HistoryTreeNodeID = ?2",
            (pack(&quantity(2, draft.to_radians())), a),
        )?;
        Ok(true)
    }

    /// Sets the radius of a Shapr3D Fillet step, the distance of a Chamfer, or the distance of an Offset Face
    /// (millimeters; an offset may be negative). False when the node is none of those.
    pub fn set_fillet(&mut self, node: i64, radius: f64) -> Result<bool> {
        if !radius.is_finite() || radius == 0.0 || radius.abs() > 1e9 {
            return Ok(false);
        }
        let Ok(props) = self.db.conn.query_row(
            "SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1 AND HistoryTreeNodeType = 2",
            [node],
            |r| r.get::<_, Vec<u8>>(0),
        ) else {
            return Ok(false);
        };
        let Some(step) = unpack(&props) else { return Ok(false) };
        let (op, params): (&str, Vec<i64>) = match step.as_array() {
            Some(a) => (
                a.get(3).and_then(Value::as_str).unwrap_or(""),
                a.get(4).and_then(Value::as_array).map(|p| p.iter().filter_map(Value::as_i64).collect()).unwrap_or_default(),
            ),
            None => return Ok(false),
        };
        // Fillet and Chamfer keep the size at param 3, Offset Face at param 1.
        let at = match op {
            "Fillet" | "Chamfer" if radius > 0.0 => 3,
            "OffsetFace" => 1,
            _ => return Ok(false),
        };
        let Some(&r) = params.get(at) else { return Ok(false) };
        let quantity =
            param(list(vec![Value::from(12), list(vec![list(vec![Value::from(1), Value::from(1)]), Value::F64(radius / 1000.0)])]));
        self.db.conn.execute("UPDATE HistoryTreeNodes SET Properties = ?1 WHERE HistoryTreeNodeID = ?2", (pack(&quantity), r))?;
        Ok(true)
    }

    /// A sketch as Shapr3D keeps one: a `MaterializeSketchPlane` step, the plane's controller, its curves and the
    /// plane origin as a managed point (as Shapr3D makes for every sketch). Curve kinds not learned yet are left
    /// out. Returns the sketch id.
    pub fn add_sketch(&mut self, sk: &Sketch) -> Result<i64> {
        let id = max_id(&self.db, "SketchControllers", "SketchID")?.max(9) + 1;
        let order: i64 = self.db.conn.query_row("SELECT COALESCE(MAX(OrderInList) + 1, 0) FROM SketchControllers", [], |r| r.get(0))?;
        let m = |v: f64| v / 1000.0;
        self.db.conn.execute(
            "INSERT INTO SketchControllers VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                id,
                sk.name,
                sk.hidden,
                order,
                m(sk.origin[0]),
                m(sk.origin[1]),
                m(sk.origin[2]),
                sk.normal[0],
                sk.normal[1],
                sk.normal[2],
                sk.u[0],
                sk.u[1],
                sk.u[2]
            ],
        )?;
        let mut point = 0i64;
        let mut construction = Vec::new();
        // Curve end points (id, position); ones in the same place are joined into a group, as Shapr3D does.
        let mut ends: Vec<(i64, [f64; 2])> = Vec::new();
        let mut at = |p: [f64; 2], ends_too: bool| {
            point += 1;
            if ends_too {
                ends.push((point, p));
            }
            serde_json::json!({ "id": point, "x": m(p[0]), "y": m(p[1]) })
        };
        for c in &sk.curves {
            let (data, dashed) = match c {
                SketchCurve::Circle { center, radius, construction, .. } => {
                    (serde_json::json!({ "type": 2, "center": at(*center, false), "radius": m(*radius) }), construction)
                }
                SketchCurve::Line { a, b, construction, .. } => (
                    serde_json::json!({ "type": 0, "start": at(*a, true), "end": at(*b, true), "isDirectionFlipped": false }),
                    construction,
                ),
                SketchCurve::Arc { center, start, end, construction, .. } => (
                    serde_json::json!({ "type": 1, "center": at(*center, false), "start": at(*start, true), "end": at(*end, true) }),
                    construction,
                ),
                // A fit spline as Shapr3D fits one (it drops a spline whose poles aren't its own fit): points,
                // poles, knots, and the derivative at each point.
                SketchCurve::Spline { points, construction, .. } => {
                    let flat: Vec<glam::DVec3> = points.iter().map(|p| glam::DVec3::new(p[0], p[1], 0.0)).collect();
                    let Some(f) = shapecraft_kernel::spline::fit(&flat) else { continue };
                    let last = points.len() - 1;
                    let through: Vec<serde_json::Value> = points.iter().enumerate().map(|(k, p)| at(*p, k == 0 || k == last)).collect();
                    let xy = |v: glam::DVec3| serde_json::json!({ "x": m(v.x), "y": m(v.y) });
                    let slopes: Vec<serde_json::Value> = f
                        .params
                        .iter()
                        .map(|&u| {
                            let d = f.derivative(u);
                            serde_json::json!({ "x": m(d.x), "y": m(d.y), "forced": false })
                        })
                        .collect();
                    let (knots, mults) = f.knot_runs();
                    let data = serde_json::json!({
                        "type": 4,
                        "interpolatingPoints": through,
                        "parametrization": 0,
                        "controlPointPositions": f.poles.iter().map(|&p| xy(p)).collect::<Vec<_>>(),
                        "derivativeVectors": slopes,
                        "knots": knots,
                        "multiplicities": mults,
                        "weights": [],
                        "degree": f.degree,
                        "periodic": false,
                    });
                    (data, construction)
                }
                SketchCurve::Other { .. } => continue,
            };
            let curve = max_id(&self.db, "SketchCurves", "CurveID")? + 1;
            self.db.conn.execute(
                "INSERT INTO SketchCurves VALUES (?1, ?2, ?3, 0, NULL)",
                (curve, id, String::from_utf8_lossy(&json(data)).into_owned()),
            )?;
            if *dashed {
                construction.push(curve);
            }
        }
        let mut groups: Vec<Vec<i64>> = Vec::new();
        let mut grouped: Vec<[f64; 2]> = Vec::new();
        for (k, &(p, pos)) in ends.iter().enumerate() {
            let same = |q: [f64; 2]| (q[0] - pos[0]).hypot(q[1] - pos[1]) < 1e-6;
            if grouped.iter().any(|&q| same(q)) {
                continue;
            }
            let members: Vec<i64> = std::iter::once(p).chain(ends[k + 1..].iter().filter(|e| same(e.1)).map(|e| e.0)).collect();
            if members.len() > 1 {
                grouped.push(pos);
                groups.push(members);
            }
        }
        let groups: Vec<serde_json::Value> =
            groups.iter().enumerate().map(|(k, g)| serde_json::json!({ "id": k + 1, "points": g, "isBroken": false })).collect();
        let text = |v: serde_json::Value| String::from_utf8_lossy(&json(v)).into_owned();
        self.db
            .conn
            .execute("INSERT INTO ConstructionSketches VALUES (?1, ?2)", (id, text(serde_json::json!({ "elements": construction }))))?;
        self.db.conn.execute(
            "INSERT INTO ConstraintPointProperties VALUES (?1, ?2)",
            (id, text(serde_json::json!({ "locked": [], "groups": groups }))),
        )?;
        // The plane's origin, a point the sketch can snap to.
        point += 1;
        let managed = max_id(&self.db, "HistoryManagedSketchPoints", "ManagedPointID")? + 1;
        let origin = self.name(24, serde_json::json!({ "sketchPlaneID": id, "standalonePointID": managed }))?;
        self.db.conn.execute("INSERT INTO HistoryManagedSketchPoints VALUES (?1, ?2, 3, ?3, ?4)", (managed, id, point, b"{}".to_vec()))?;
        self.db.conn.execute(
            "INSERT INTO StandaloneConstraintPoints (PointID, SketchID, Data, Type) VALUES (?1, ?2, ?3, 1)",
            (point, id, text(serde_json::json!({ "x": 0.0, "y": 0.0 }))),
        )?;
        self.db
            .conn
            .execute("INSERT INTO MaterializedPointNameToPointID (SketchID, NameID, PointID) VALUES (?1, ?2, ?3)", (id, origin, point))?;
        // Step parameters as Shapr3D writes them for a sketch on a standard plane.
        let p = |v: Value| Value::Array(vec![Value::Array(vec![v])]);
        let a = self.node(3, p(Value::Array(vec![Value::from(10), Value::from(id)])))?;
        let b = self.node(3, p(Value::Array(vec![Value::from(0)])))?;
        let c = self.node(1, Value::Array(vec![Value::Array(Vec::new())]))?;
        let d = self.node(3, p(Value::Array(vec![Value::from(9), Value::Array(Vec::new())])))?;
        let n = self.step_count("MaterializeSketchPlane")? + 1;
        let name = format!("Sketch {n:02}");
        let params = Value::Array([a, b, c, d].into_iter().map(Value::from).collect());
        let step = self.node(
            2,
            Value::Array(vec![
                Value::from(HISTORY_VERSION),
                Value::from(HISTORY_VERSION),
                Value::from(name),
                Value::from("MaterializeSketchPlane"),
                params,
            ]),
        )?;
        self.append_step(step)?;
        self.file_under(&[], &id.to_string(), 2)?;
        Ok(id)
    }

    /// Moves and resizes the circles of a sketch already in the file (in curve order), keeping their radius
    /// dimensions in step; Shapr3D rebuilds everything made from the sketch. False when the sketch isn't there.
    pub fn update_circles(&mut self, sketch: i64, circles: &[([f64; 2], f64)]) -> Result<bool> {
        let rows: Vec<(i64, String)> = {
            let mut q = self.db.conn.prepare("SELECT CurveID, Data FROM SketchCurves WHERE SketchID = ?1 ORDER BY CurveID")?;
            q.query_map([sketch], |r| Ok((r.get::<_, i64>(0)?, crate::text(r.get(1)?))))?.flatten().collect()
        };
        let mut k = 0;
        for (curve, data) in rows {
            let mut j: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            if j["type"].as_i64() != Some(2) {
                continue;
            }
            let Some(&(c, r)) = circles.get(k) else { break };
            k += 1;
            j["center"]["x"] = serde_json::json!(c[0] / 1000.0);
            j["center"]["y"] = serde_json::json!(c[1] / 1000.0);
            j["radius"] = serde_json::json!(r / 1000.0);
            self.db.conn.execute("UPDATE SketchCurves SET Data = ?1 WHERE CurveID = ?2", (j.to_string(), curve))?;
            // Radius dimensions (type 21) on this curve follow.
            let cons: Vec<(i64, String)> = {
                let mut q = self.db.conn.prepare("SELECT RowID, Data FROM Constraints WHERE SketchID = ?1")?;
                q.query_map([sketch], |r| Ok((r.get::<_, i64>(0)?, crate::text(r.get(1)?))))?.flatten().collect()
            };
            for (row, data) in cons {
                let mut cj: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
                if cj["type"].as_i64() == Some(21) && cj["curve"].as_i64() == Some(curve) {
                    cj["radius"] = serde_json::json!(r / 1000.0);
                    self.db.conn.execute("UPDATE Constraints SET Data = ?1 WHERE RowID = ?2", (cj.to_string(), row))?;
                }
            }
        }
        Ok(k > 0)
    }

    /// Moves the body that import step `creator` brings in by `by` (mm), through the import's placement, so
    /// it stays the same item and Shapr3D's later steps on it replay on the moved body. False when the step
    /// isn't an import of exactly one body.
    pub fn translate_import(&mut self, creator: i64, by: [f64; 3]) -> Result<bool> {
        let Ok(props) = self.db.conn.query_row(
            "SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1 AND HistoryTreeNodeType = 2",
            [creator],
            |r| r.get::<_, Vec<u8>>(0),
        ) else {
            return Ok(false);
        };
        let Some(step) = unpack(&props) else { return Ok(false) };
        let is_import = step.as_array().and_then(|a| a.get(3)).and_then(Value::as_str) == Some("MaterializeImportedBodies");
        let param = step.as_array().and_then(|a| a.get(4)).and_then(Value::as_array).and_then(|p| p.first()).and_then(Value::as_i64);
        let (true, Some(param)) = (is_import, param) else { return Ok(false) };
        let arg: Vec<u8> =
            self.db.conn.query_row("SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1", [param], |r| r.get(0))?;
        // [[[9, [[[11, body]], …]]]]: the imported bodies.
        let bodies: Vec<i64> = unpack(&arg)
            .as_ref()
            .and_then(|v| v.as_array()?.first()?.as_array()?.first()?.as_array()?.get(1)?.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|b| {
                let pair = b.as_array()?.first()?.as_array()?.clone();
                (pair.first()?.as_i64()? == 11).then(|| pair.get(1)?.as_i64()).flatten()
            })
            .collect();
        let [body] = bodies.as_slice() else { return Ok(false) };
        if self.used_later(creator)? {
            return Ok(false);
        }
        let Ok(t) = self
            .db
            .conn
            .query_row("SELECT Transform FROM HistoryImportedBodies WHERE ImportedBodyID = ?1", [body], |r| r.get::<_, Vec<u8>>(0))
        else {
            return Ok(false);
        };
        let mut m: Vec<f64> =
            unpack(&t).and_then(|v| v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect())).unwrap_or_default();
        if m.len() != 16 {
            return Ok(false);
        }
        // Column-major 4×4 in meters: the translation is the last column.
        for k in 0..3 {
            m[12 + k] += by[k] / 1000.0;
        }
        let packed = pack(&Value::Array(m.into_iter().map(Value::F64).collect()));
        self.db.conn.execute("UPDATE HistoryImportedBodies SET Transform = ?1 WHERE ImportedBodyID = ?2", (packed, body))?;
        Ok(true)
    }

    /// Whether a step after `creator` refers to anything `creator` made (a Subtract using its body as the
    /// tool, a fillet on its edges): moving the body under such a step would change what that step does.
    fn used_later(&self, creator: i64) -> Result<bool> {
        let names: Vec<(i64, serde_json::Value)> = {
            let mut q = self.db.conn.prepare("SELECT NameID, Data FROM HistoryNames")?;
            q.query_map([], |r| Ok((r.get::<_, i64>(0)?, crate::text(r.get(1)?))))?
                .flatten()
                .map(|(id, t)| (id, serde_json::from_str(&t).unwrap_or_default()))
                .collect()
        };
        fn refs(v: &serde_json::Value, out: &mut Vec<i64>) {
            const KEYS: &[&str] = &["wrappedName", "nameToAnchor", "sourceTopologyNames", "originals", "filling", "originalEdgeOrCurve"];
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
                        refs(x, out);
                    }
                }
                serde_json::Value::Array(a) => a.iter().for_each(|x| refs(x, out)),
                _ => {}
            }
        }
        let made_by = |j: &serde_json::Value| {
            j["callKey"]["nodeID"].as_i64() == Some(creator) || j["callKeyPrefix"]["nodeID"].as_i64() == Some(creator)
        };
        let mut tainted: std::collections::HashSet<i64> = names.iter().filter(|(_, j)| made_by(j)).map(|(id, _)| *id).collect();
        for _ in 0..64 {
            let before = tainted.len();
            for (id, j) in &names {
                let mut r = Vec::new();
                refs(j, &mut r);
                if r.iter().any(|x| tainted.contains(x)) {
                    tainted.insert(*id);
                }
            }
            if tainted.len() == before {
                break;
            }
        }
        // Steps after the creator, and the name ids ([7, id]) their parameters use.
        let (_, root): (i64, Vec<u8>) = self.db.conn.query_row(
            "SELECT HistoryTreeNodeID, Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeType = 0 LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let order: Vec<i64> = unpack(&root)
            .and_then(|v| v.as_array()?.get(1)?.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()))
            .unwrap_or_default();
        let Some(at) = order.iter().position(|&n| n == creator) else { return Ok(true) };
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
        for &step in &order[at + 1..] {
            let props: Vec<u8> = self
                .db
                .conn
                .query_row("SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1", [step], |r| r.get(0))
                .unwrap_or_default();
            let params: Vec<i64> = unpack(&props)
                .and_then(|v| v.as_array()?.get(4)?.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()))
                .unwrap_or_default();
            for p in params {
                let pb: Vec<u8> = self
                    .db
                    .conn
                    .query_row("SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1", [p], |r| r.get(0))
                    .unwrap_or_default();
                let mut used = Vec::new();
                if let Some(v) = unpack(&pb) {
                    name_refs(&v, &mut used);
                }
                if used.iter().any(|n| tainted.contains(n)) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Keeps a copy of the body step `creator` made (a Parasolid part) where only we look ("ShapeCraftBodies"), so the
    /// file read here again before Shapr3D has rebuilt it still has that body. Shapr3D ignores the table (t6b).
    /// Imports are left out: their own placed prototype already gives the body. False when not kept.
    pub fn stash_body(&mut self, creator: i64, xt: &[u8]) -> Result<bool> {
        let op: Option<String> = self
            .db
            .conn
            .query_row("SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1 AND HistoryTreeNodeType = 2", [creator], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .ok()
            .and_then(|b| unpack(&b))
            .and_then(|v| v.as_array()?.get(3)?.as_str().map(str::to_string));
        match op.as_deref() {
            None | Some("MaterializeImportedBodies") => return Ok(false),
            Some(_) => {}
        }
        self.db.conn.execute_batch(crate::STASH_SCHEMA)?;
        self.db.conn.execute("INSERT INTO ShapeCraftBodies (HistoryTreeNodeID, Body) VALUES (?1, ?2)", (creator, xt))?;
        Ok(true)
    }

    /// Keeps our own parametric history (JSON) where only we look ("ShapeCraftHistory"), so a file we wrote opens
    /// here again with its steps. Replaces any kept before.
    pub fn set_own_history(&mut self, json: &str) -> Result<()> {
        self.db.conn.execute_batch(crate::HISTORY_SCHEMA)?;
        self.db.conn.execute("DELETE FROM ShapeCraftHistory", [])?;
        self.db.conn.execute("INSERT INTO ShapeCraftHistory (Version, Data) VALUES (1, ?1)", [json.as_bytes()])?;
        Ok(())
    }

    /// Hides the Items folder at `path` (and so everything in it); false when there's no such folder.
    pub fn hide_folder(&mut self, path: &[String]) -> Result<bool> {
        let mut at: String =
            self.db.conn.query_row("SELECT FolderID FROM HistoryFolders WHERE FolderName = 'root' LIMIT 1", [], |r| r.get(0))?;
        for name in path {
            let next = self.children(&at)?.into_iter().filter(|(_, t)| *t == 0).find(|(id, _)| {
                self.db
                    .conn
                    .query_row("SELECT FolderName FROM HistoryFolders WHERE FolderID = ?1", [id], |r| r.get::<_, String>(0))
                    .ok()
                    .as_deref()
                    == Some(name.as_str())
            });
            match next {
                Some((id, _)) => at = id,
                None => return Ok(false),
            }
        }
        Ok(!path.is_empty() && self.db.conn.execute("UPDATE HistoryFolders SET Hidden = 1 WHERE FolderID = ?1", [at])? > 0)
    }

    /// The finished `.shapr` file (a new project identity each time, so Shapr3D opens it as a new project).
    pub fn finish(mut self) -> Result<Vec<u8>> {
        self.db
            .conn
            .execute("INSERT OR REPLACE INTO Settings (SettingName, SettingValue) VALUES ('Persistence_SaveTimestamp', ?1)", [iso_now()])?;
        let uuid = self.uuid();
        let nonce = self.uuid();
        self.db.conn.execute("INSERT OR REPLACE INTO Settings (SettingName, SettingValue) VALUES ('ProjectStateNonce', ?1)", [nonce])?;
        let workspace = self.db.bytes()?;
        let file = ShaprFile { metadata: serde_json::json!({ "remoteID": uuid, "revisionID": 1, "changeSeqNo": 2 }), workspace };
        Ok(file.to_bytes())
    }
}

/// Files written before the rename (as Shayper3D) keep their body copies in "ShayperBodies". The table takes the new
/// name before anything is added, or the reader would find only the new rows and lose the old ones.
fn adopt_old_stash(conn: &rusqlite::Connection) -> Result<()> {
    let has = |table: &str| {
        conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            > 0
    };
    if has("ShayperBodies") && !has("ShapeCraftBodies") {
        conn.execute_batch("ALTER TABLE ShayperBodies RENAME TO ShapeCraftBodies;")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Writer;

    #[test]
    fn body_copies_from_before_the_rename_join_the_new_table() {
        let w = Writer::fresh().unwrap();
        let conn = &w.db.conn;
        conn.execute_batch(
            "CREATE TABLE ShayperBodies (HistoryTreeNodeID INTEGER NOT NULL, Body BLOB NOT NULL); INSERT INTO ShayperBodies VALUES (7, x'01');",
        )
        .unwrap();
        // Twice: a file patched again has nothing left to adopt.
        super::adopt_old_stash(conn).unwrap();
        super::adopt_old_stash(conn).unwrap();
        conn.execute_batch(crate::STASH_SCHEMA).unwrap();
        conn.execute("INSERT INTO ShapeCraftBodies VALUES (8, x'02')", []).unwrap();
        let nodes: Vec<i64> = {
            let mut q = conn.prepare("SELECT HistoryTreeNodeID FROM ShapeCraftBodies ORDER BY HistoryTreeNodeID").unwrap();
            q.query_map([], |r| r.get(0)).unwrap().flatten().collect()
        };
        assert_eq!(nodes, [7, 8]);
        let old: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'ShayperBodies'", [], |r| r.get(0)).unwrap();
        assert_eq!(old, 0);
    }

    #[test]
    fn an_import_moves_only_when_no_later_step_uses_it() {
        let mut w = Writer::fresh().unwrap();
        let a = w.import_body("A", b"PS\0\0", &[], Default::default()).unwrap();
        let b = w.import_body("B", b"PS\0\0", &[], Default::default()).unwrap();
        // A later step refers to A (here a Delete): moving A's import would move what that step works on.
        w.delete_body(a).unwrap();
        assert!(!w.translate_import(a, [1.0, 0.0, 0.0]).unwrap());
        assert!(w.translate_import(b, [1.0, 2.0, 3.0]).unwrap());
        // Not an import step at all.
        assert!(!w.translate_import(9999, [1.0, 0.0, 0.0]).unwrap());
    }

    #[test]
    fn delete_and_move_steps_nest_their_parameters_as_shapr3d_does() {
        let mut w = Writer::fresh().unwrap();
        let a = w.import_body("A", b"PS\0\0", &[], Default::default()).unwrap();
        assert!(w.move_body(a, [0.0, 15.0, 4.0]).unwrap());
        w.delete_body(a).unwrap();
        let steps: Vec<Vec<u8>> = {
            let mut q =
                w.db.conn
                    .prepare("SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeType = 2 ORDER BY HistoryTreeNodeID")
                    .unwrap();
            q.query_map([], |r| r.get(0)).unwrap().flatten().collect()
        };
        let ops: Vec<(String, Vec<i64>)> = steps
            .iter()
            .filter_map(|b| crate::unpack(b))
            .map(|v| {
                let a = v.as_array().unwrap().clone();
                (a[3].as_str().unwrap().to_string(), a[4].as_array().unwrap().iter().filter_map(rmpv::Value::as_i64).collect())
            })
            .collect();
        assert_eq!(ops.iter().map(|o| o.0.as_str()).collect::<Vec<_>>(), ["MaterializeImportedBodies", "Transform", "Delete"]);
        for (_, params) in &ops[1..] {
            for p in params {
                let b: Vec<u8> =
                    w.db.conn.query_row("SELECT Properties FROM HistoryTreeNodes WHERE HistoryTreeNodeID = ?1", [p], |r| r.get(0)).unwrap();
                // [[[tag, value]]]: the tag is a number three lists down.
                let v = crate::unpack(&b).unwrap();
                let tag = v.as_array().and_then(|a| a.first()?.as_array()?.first()?.as_array()?.first()?.as_i64());
                assert!(tag.is_some(), "{v}");
            }
        }
    }
}
