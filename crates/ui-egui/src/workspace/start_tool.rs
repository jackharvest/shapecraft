//! Starting a tool: what each tool needs selected, waiting for it, and the tool a fresh selection predicts.

use glam::DVec3;
use serde_json::json;
use shapecraft_doc::{ItemId, Pick, SketchEntity};

use super::Workspace;
use super::toolbar::ToolKind;
use super::tools::{self, Tool};

impl Workspace {
    /// A tool started with nothing (or the wrong thing) selected waits for a selection, as observed (its banner
    /// says what to pick); it starts as soon as what's picked fits.
    fn await_selection(&mut self, kind: ToolKind, hint: &'static str) {
        // What's picked that the tool can't take lets go (an edge for Extrude, a region for Fillet: s24 matrix); Sweep
        // keeps picked curves or edges for its path.
        let bodies = self.session.selection_kind() == shapecraft_engine::SelectionKind::Bodies;
        // Move keeps them (Isolate still offered) and waits with the idle column.
        if kind != ToolKind::Move && (kind != ToolKind::Sweep || bodies) {
            self.session.selection.clear();
        }
        let title = match kind {
            ToolKind::Move => "Move/Rotate (M)",
            ToolKind::Mirror => "Mirror",
            ToolKind::Pattern => "Linear Pattern",
            ToolKind::CircularPattern => "Circular Pattern",
            ToolKind::Extrude => "Extrude - One-Sided (E)",
            ToolKind::Fillet => "Chamfer/Fillet (F)",
            ToolKind::Shell => "Shell (H)",
            ToolKind::OffsetFace => "Offset Face",
            ToolKind::OffsetEdge => "Offset Edge (O)",
            ToolKind::Trim => "Trim (T)",
            ToolKind::Project => "Project as Sketch (P)",
            ToolKind::Scale | ToolKind::ScaleNonUniform => "Scale (S)",
            ToolKind::SplitBody => "Split Body",
            ToolKind::RotateAxis => "Rotate Around Axis",
            ToolKind::OffsetPlane => "Plane - Offset",
            ToolKind::Sweep => "Sweep (W)",
            ToolKind::Revolve => "Revolve (V)",
            ToolKind::WrapEmboss => "Wrap & Emboss",
            _ => "Select",
        };
        self.tool = Tool::Await { kind, title, hint };
    }

    /// A tool type picked from the menu bar (Sketch ▸ Rectangle ▸ 3 Points): remembered as the toolbar's
    /// pick, then the tool starts that way. A running Offset Edge switches its Type in place.
    pub fn start_variant(&mut self, kind: ToolKind, key: &str, i: usize) {
        if kind == ToolKind::OffsetEdge && matches!(self.tool, Tool::OffsetEdge(_)) {
            self.badge_pick(super::badges::Badge::OffsetType, i);
            return;
        }
        self.variants.insert(key.to_string(), i);
        // An armed drawing tool of the same kind restarts as the new type (its key would cycle it instead).
        if matches!(self.tool, Tool::Rectangle { .. } | Tool::Rect3 { .. } | Tool::Polygon { .. } | Tool::Spline { .. }) {
            self.tool = Tool::Idle;
        }
        self.start_tool(kind);
    }

    pub fn start_tool(&mut self, kind: ToolKind) {
        // The key of the tool a pick already started leaves it as it is (s24 matrix).
        let same = match &self.tool {
            Tool::OffsetFace(_) => kind == ToolKind::OffsetFace,
            Tool::Extrude(_) => kind == ToolKind::Extrude,
            Tool::Fillet(t) => kind == if t.chamfer { ToolKind::Chamfer } else { ToolKind::Fillet },
            Tool::Move(_) => kind == ToolKind::Move,
            _ => false,
        };
        // 26.200: Extrude's key while it runs flips One-Sided / Symmetric, and a bubble says so (s25 e2).
        if !self.predicted
            && same
            && let Tool::Extrude(t) = &mut self.tool
        {
            t.symmetric = !t.symmetric;
            self.preview = None;
            self.cycled = Some((super::badges::Badge::Sides, self.now));
            return;
        }
        // O on a running Offset Edge flips its Type, Single / Chain (the same article: cycling works in Offset Edge).
        if !self.predicted && kind == ToolKind::OffsetEdge && matches!(&self.tool, Tool::OffsetEdge(t) if t.face.is_none()) {
            let next = 1 - self.variant("Offset Edge").min(1);
            self.badge_pick(super::badges::Badge::OffsetType, next);
            self.cycled = Some((super::badges::Badge::OffsetType, self.now));
            return;
        }
        // F on a running Fillet flips its Type, Radius / Chordal (26.200 article "Meet Parameter Cards").
        if !self.predicted
            && same
            && let Tool::Fillet(t) = &self.tool
            && !t.chamfer
        {
            let mut t = t.clone();
            t.chordal = !t.chordal;
            self.variants.insert("Fillet Size Type".into(), usize::from(t.chordal));
            self.preview = None;
            self.cycled = Some((super::badges::Badge::SizeType, self.now));
            self.tool = Tool::Fillet(t.clone());
            if t.radius > 1e-9 {
                self.apply_fillet(&t);
            }
            return;
        }
        if self.predicted && self.live_step.is_none() && same {
            // Extrude's key on the Extrude a region started: it becomes a picked tool with its card open (s24 matrix).
            if matches!(self.tool, Tool::Extrude(_)) {
                self.predicted = false;
                self.options_menu = Some(self.viewport_rect.left_top());
            }
            return;
        }
        let only_faces = !self.session.selection.is_empty() && self.session.selection.iter().all(|p| matches!(p, Pick::Face { .. }));
        match kind {
            // M over faces or edges waits for items to move, keeping the pick (s24 matrix). Faces of one body, or one
            // straight edge or rim, move on their own (the gizmo comes up at them); so do sketch regions.
            ToolKind::Move
                if self.session.active_sketch.is_none()
                    && tools::picked_faces(&self.session).is_none()
                    && tools::MoveTool::from_selection(&self.session).is_none_or(|t| t.edge.is_none())
                    && !self.session.selection.is_empty()
                    && self.session.selection.iter().all(|p| matches!(p, Pick::Face { .. } | Pick::Edge { .. })) =>
            {
                self.predicted = false;
                self.live_step = None;
                self.await_selection(ToolKind::Move, "Pick what to move or turn, then use the gizmo.");
                return;
            }
            // As observed (s24 matrix): Fillet on picked faces takes their edges; Move/Rotate drops picked faces and
            // waits for items; Trim does nothing over a pick that isn't sketch curves.
            ToolKind::Fillet | ToolKind::Chamfer if only_faces => {
                let mut edges = Vec::new();
                for p in &self.session.selection {
                    if let Pick::Face { body, face } = *p
                        && let Some(b) = self.session.doc.body(body)
                        && let Ok(list) = self.session.kernel.face_edges(&b.shape, face)
                    {
                        // Seams aren't drawn, so they aren't picked either.
                        edges.extend(list.into_iter().filter(|e| b.mesh.edge_ids.contains(e)).map(|edge| Pick::Edge { body, edge }));
                    }
                }
                self.session.selection = edges;
            }
            ToolKind::Trim
                if self.session.active_sketch.is_none()
                    && !self.session.selection.is_empty()
                    && !self.session.selection.iter().any(|p| matches!(p, Pick::SketchEntity { .. })) =>
            {
                return;
            }
            _ => {}
        }
        self.items_focus = false;
        self.predicted = false;
        self.numeric = None;
        self.line_len = None;
        self.live_step = None;
        match kind {
            ToolKind::Sketch => {
                // A selected face goes straight to sketching on it (a curved one: on the origin plane facing the view).
                let face = self.session.selection.iter().find_map(|p| match *p {
                    Pick::Face { body, face } => Some((body, face)),
                    _ => None,
                });
                let plane = self.session.selection.iter().find_map(|p| match *p {
                    Pick::Plane { plane } => Some(plane),
                    _ => None,
                });
                if let Some((body, face)) = face {
                    self.sketch_on_face(body, face);
                } else if let Some(plane) = plane {
                    self.start_sketch_on(json!({ "construction": plane.0 }));
                } else {
                    self.session.selection.clear();
                    self.pending_tool = None;
                    self.enter_plane_pick();
                }
            }
            ToolKind::Line
            | ToolKind::Rectangle
            | ToolKind::Circle
            | ToolKind::Polygon
            | ToolKind::Arc
            | ToolKind::Ellipse
            | ToolKind::Spline => {
                if self.session.active_sketch.is_some() {
                    self.tool = self.sketch_tool(kind);
                } else {
                    // The click picks the plane and the first point; what was picked lets go (s24 matrix).
                    self.session.selection.clear();
                    self.pending_tool = Some(kind);
                    self.enter_plane_pick();
                }
            }
            ToolKind::Extrude => match tools::ExtrudeTool::from_selection(&self.session, self.last_hit) {
                Some(t) => {
                    self.remember("extrude");
                    // 26.200 leaves the sketch as Extrude starts: no grid or constraints, the idle column plus
                    // Extrude, Cancel at the bottom (side by side, 2026-11-02). The picked regions stay picked.
                    if self.session.active_sketch.is_some() {
                        let _ = self.run("exit_sketching", json!({}));
                    }
                    self.tool = Tool::Extrude(t);
                }
                None => self.await_selection(ToolKind::Extrude, "Select a sketch region or a flat face to extrude"),
            },
            ToolKind::Chamfer => {
                self.start_tool(ToolKind::Fillet);
                if let Tool::Fillet(t) = &mut self.tool {
                    t.chamfer = true;
                }
            }
            ToolKind::Fillet => {
                let picked = {
                    // A body becomes all of its edges, as observed (311).
                    if let [Pick::Body { body }] = self.session.selection.as_slice()
                        && let Some(b) = self.session.doc.body(*body)
                    {
                        let body = *body;
                        let mut ids = b.mesh.edge_ids.clone();
                        ids.sort_unstable();
                        ids.dedup();
                        self.session.selection = ids.into_iter().map(|edge| Pick::Edge { body, edge }).collect();
                    }
                    tools::FilletTool::from_selection(&self.session, self.last_hit)
                };
                match picked {
                    Some(mut t) => {
                        self.remember("fillet");
                        // Tools remember their last choices (26.200's Parameter Cards).
                        t.g2 = self.variants.get("Fillet Continuity") == Some(&1);
                        t.chordal = self.variants.get("Fillet Size Type") == Some(&1);
                        t.setback = self.variants.get("Fillet Corner") == Some(&1);
                        self.tool = Tool::Fillet(t);
                    }
                    None => self.await_selection(ToolKind::Fillet, "Select edges or a body to round"),
                }
            }
            ToolKind::Shell => {
                let picked = {
                    // A body becomes all of its faces, as observed (329: "6 faces", the handle on one of them).
                    if let [Pick::Body { body }] = self.session.selection.as_slice()
                        && let Some(b) = self.session.doc.body(*body)
                    {
                        let body = *body;
                        let mut ids = b.mesh.triangle_faces.clone();
                        ids.sort_unstable();
                        ids.dedup();
                        self.session.selection = ids.into_iter().map(|face| Pick::Face { body, face }).collect();
                    }
                    tools::ShellTool::from_selection(&self.session)
                };
                match picked {
                    Some(t) => {
                        self.remember("shell");
                        self.tool = Tool::Shell(t);
                    }
                    None => self.await_selection(ToolKind::Shell, "Select the faces to open, then Shell"),
                }
            }
            ToolKind::Move => match tools::MoveTool::from_selection(&self.session) {
                Some(t) => {
                    self.remember("translate");
                    self.tool = Tool::Move(t);
                }
                None => self.await_selection(ToolKind::Move, "Select bodies or sketch curves to move or rotate"),
            },
            ToolKind::PlaneEdge | ToolKind::PlaneCurve => {
                let edge = self.session.selection.iter().find_map(|p| match *p {
                    Pick::Edge { body, edge } => Some((body, edge)),
                    _ => None,
                });
                match edge {
                    Some((body, edge)) => {
                        let through = kind == ToolKind::PlaneEdge;
                        let mut t = tools::EdgePlaneTool { body, edge, through, value: 0.0, dragging: None };
                        // On a curve it starts at the end nearer where the edge was clicked (346).
                        if !through
                            && let (Some(hit), Some(a), Some(b)) =
                                (self.last_hit, t.point_at(&self.session, 0.0), t.point_at(&self.session, 1.0))
                            && hit.distance(b) < hit.distance(a)
                        {
                            t.value = 1.0;
                        }
                        self.remember(if through { "plane_through_edge" } else { "plane_on_curve" });
                        // Made at once, as observed; the handle then changes the same step.
                        let (id, params) = t.request(t.value);
                        if self.run_live(id, params, false) {
                            self.session.selection = vec![Pick::Edge { body, edge }];
                            self.tool = Tool::EdgePlane(t);
                        }
                    }
                    None => self.await_selection(kind, "Select an edge"),
                }
            }
            ToolKind::Union | ToolKind::Subtract | ToolKind::Intersect => {
                let op = match kind {
                    ToolKind::Subtract => "subtract",
                    ToolKind::Intersect => "intersect",
                    _ => "union",
                };
                let mut bodies: Vec<ItemId> = Vec::new();
                for b in self.session.selection.iter().filter_map(Pick::body) {
                    if !bodies.contains(&b) {
                        bodies.push(b);
                    }
                }
                // Subtract: the first selected body is cut from; any others selected with it are what's cut away.
                let (targets, tools) =
                    if op == "subtract" && bodies.len() > 1 { (bodies[..1].to_vec(), bodies[1..].to_vec()) } else { (bodies, Vec::new()) };
                if targets.is_empty() {
                    self.toast("Select a body first");
                } else {
                    self.remember(op);
                    self.session.selection = targets.iter().chain(&tools).map(|&body| Pick::Body { body }).collect();
                    self.tool = Tool::Boolean(tools::BooleanTool { op, targets, tools, keep: 0 });
                }
            }
            ToolKind::Translate => {
                let mut items: Vec<ItemId> = Vec::new();
                for p in &self.session.selection {
                    let id = match *p {
                        Pick::Plane { plane } => Some(plane),
                        _ => p.body(),
                    };
                    if let Some(id) = id
                        && !items.contains(&id)
                    {
                        items.push(id);
                    }
                }
                if items.is_empty() {
                    self.await_selection(ToolKind::Translate, "Select items to translate");
                } else {
                    self.remember("translate");
                    self.tool = Tool::Translate { items, from: None, to: None, copy: false };
                }
            }
            ToolKind::Mirror => match tools::MirrorTool::from_selection(&self.session) {
                Some(t) => self.tool = Tool::Mirror(t),
                None => self.await_selection(ToolKind::Mirror, "Select bodies or sketch curves to mirror"),
            },
            ToolKind::SplitBody => match tools::MirrorTool::from_selection(&self.session) {
                // Keep Originals is remembered (26.200 Parameter Cards).
                Some(t) => {
                    let keep = self.variants.get("Split Keep") == Some(&1);
                    self.tool = Tool::Mirror(tools::MirrorTool { split: true, keep, ..t })
                }
                None => self.await_selection(ToolKind::SplitBody, "Select bodies to split"),
            },
            ToolKind::OffsetFace => {
                // A whole body picked: Offset Face takes all its faces ("4 faces"), as the reference does (s52r-019).
                let whole = if let [Pick::Body { body }] = self.session.selection[..]
                    && let Some(b) = self.session.doc.body(body)
                    && let Ok(n) = self.session.kernel.face_count(&b.shape)
                    && n > 0
                {
                    self.session.selection = (0..n).map(|face| Pick::Face { body, face }).collect();
                    true
                } else {
                    false
                };
                match tools::OffsetFaceTool::from_selection(&self.session, self.last_hit.filter(|_| !whole)) {
                    Some(t) => {
                        self.remember("offset_face");
                        self.tool = Tool::OffsetFace(tools::OffsetFaceTool { whole, ..t });
                    }
                    None => self.await_selection(ToolKind::OffsetFace, "Select faces to offset"),
                }
            }
            // A sketch region picked outside its sketch scales right there, the region staying picked (26.202,
            // s52r-034; 26.200 opened the sketch and waited, s24 matrix).
            ToolKind::ScaleNonUniform => match tools::ScaleTool::from_selection(&self.session, false) {
                Some(t) => {
                    self.remember("scale");
                    self.tool = Tool::Scale(t);
                }
                None => self.await_selection(ToolKind::ScaleNonUniform, self.scale_hint()),
            },
            ToolKind::Scale => match tools::ScaleTool::from_selection(&self.session, true) {
                Some(t) => {
                    self.remember("scale");
                    self.tool = Tool::Scale(t);
                }
                None => self.await_selection(ToolKind::Scale, self.scale_hint()),
            },
            ToolKind::Pattern => {
                match tools::PatternTool::from_selection(&self.session).map(|t| tools::PatternTool { total: self.pattern_total, ..t }) {
                    Some(t) => {
                        self.remember("pattern");
                        self.tool = Tool::Pattern(t);
                    }
                    None => self.await_selection(ToolKind::Pattern, "Select bodies or sketch curves to pattern"),
                }
            }
            ToolKind::CircularPattern => match tools::CircularPatternTool::from_selection(&self.session) {
                Some(t) => {
                    self.remember("pattern_circular");
                    self.tool = Tool::CircularPattern(t);
                }
                None => self.await_selection(ToolKind::CircularPattern, "Select bodies or sketch curves to pattern"),
            },
            ToolKind::RotateAxis => match tools::RotateAxisTool::from_selection(&self.session) {
                Some(t) => {
                    self.remember("rotate");
                    self.tool = Tool::RotateAxis(t);
                }
                None => self.await_selection(
                    ToolKind::RotateAxis,
                    if self.session.active_sketch.is_some() { "Select sketch curves to turn" } else { "Select bodies to rotate" },
                ),
            },
            ToolKind::OffsetPlane => match tools::OffsetPlaneTool::from_selection(&self.session) {
                Some(t) => {
                    self.remember("offset_plane");
                    self.tool = Tool::OffsetPlane(t);
                }
                None => self.await_selection(ToolKind::OffsetPlane, "Select a flat face, plane or sketch region to offset from"),
            },
            ToolKind::Align => {
                let mut items: Vec<ItemId> = Vec::new();
                for b in self.session.selection.iter().filter_map(Pick::body) {
                    if !items.contains(&b) {
                        items.push(b);
                    }
                }
                let from = self.session.selection.iter().find_map(|p| match *p {
                    Pick::Face { body, face } => Some((body, face)),
                    _ => None,
                });
                if items.is_empty() {
                    self.toast("Select a body or face to align");
                } else {
                    self.remember("align");
                    self.tool = Tool::Align { items, from, to: None, flip: false, edge: false };
                }
            }
            // Project from outside a sketch is Project as Sketch: items, then the face or plane to project onto (s24 matrix).
            ToolKind::Project if self.session.active_sketch.is_some() => {
                self.remember("project");
                if self.session.selection.iter().any(|p| matches!(p, Pick::Edge { .. } | Pick::Face { .. })) {
                    let _ = self.run("project", json!({}));
                }
                self.tool = Tool::Project;
            }
            ToolKind::ReplaceFace | ToolKind::ProjectEdge | ToolKind::Project => {
                // What's picked that the tool takes, badges at their middles (the last one where it was clicked);
                // or nothing yet, picked in the tool.
                let sketch = kind == ToolKind::Project;
                let mut t = tools::OntoTool { project: kind != ToolKind::ReplaceFace, sketch, ..Default::default() };
                let k = &self.session.kernel;
                let middle = |p: &Pick| -> Option<DVec3> {
                    match *p {
                        Pick::Face { body, face } => {
                            Some(k.measure(&self.session.doc.body(body)?.shape, shapecraft_kernel::SubShape::Face(face)).ok()?.1)
                        }
                        Pick::Edge { body, edge } => {
                            Some(k.measure(&self.session.doc.body(body)?.shape, shapecraft_kernel::SubShape::Edge(edge)).ok()?.1)
                        }
                        Pick::Region { sketch, region } => {
                            let bb = k.bounds(&self.session.doc.sketch(sketch)?.region(region)?.face).ok()?;
                            Some((bb.min + bb.max) / 2.0)
                        }
                        _ => None,
                    }
                };
                let picks: Vec<Pick> = self
                    .session
                    .selection
                    .iter()
                    .copied()
                    .filter(|p| t.takes_item(p) || t.project && matches!(p, Pick::Face { .. }))
                    .collect();
                for (i, p) in picks.iter().enumerate() {
                    if !t.project && t.body().is_some_and(|b| Some(b) != p.body()) {
                        continue;
                    }
                    let at = if i + 1 == picks.len() { self.last_hit.or_else(|| middle(p)) } else { middle(p) };
                    if let Some(at) = at {
                        t.items.push((*p, at));
                    }
                }
                self.remember(t.command());
                self.tool = Tool::Onto(t);
            }
            ToolKind::PlanePerpEdge | ToolKind::PlaneParFace => {
                let perp = kind == ToolKind::PlanePerpEdge;
                // What's picked already serves as the edge or face.
                let base = self
                    .session
                    .selection
                    .iter()
                    .copied()
                    .find(|p| if perp { matches!(p, Pick::Edge { .. }) } else { matches!(p, Pick::Face { .. } | Pick::Plane { .. }) });
                self.tool = Tool::PointPlane { perp, base };
            }
            ToolKind::Axis2 => {
                self.remember("axis_2points");
                self.session.selection.clear();
                self.tool = Tool::Axis2 { points: Vec::new() };
            }
            ToolKind::SketchFillet => {
                let Some(sk) = self.session.active_sketch() else {
                    self.toast("Fillet rounds corners inside a sketch: start one first");
                    return;
                };
                let picked: Vec<usize> = self
                    .session
                    .selection
                    .iter()
                    .filter_map(|p| match *p {
                        Pick::SketchEntity { sketch, entity } if sketch == sk.id => Some(entity as usize),
                        _ => None,
                    })
                    .collect();
                let all = shapecraft_doc::sketch_ops::line_corners(&sk.entities, None);
                let corners =
                    if picked.is_empty() { Vec::new() } else { shapecraft_doc::sketch_ops::line_corners(&sk.entities, Some(&picked)) };
                // About a tenth of the shortest side meeting at a picked corner (0.7 mm on ~7 mm sides, s11-095).
                let shortest = sk
                    .entities
                    .iter()
                    .filter_map(|e| match *e {
                        SketchEntity::Line { a, b } if corners.iter().any(|c| c.distance(a) < 1e-7 || c.distance(b) < 1e-7) => {
                            Some(a.distance(b))
                        }
                        _ => None,
                    })
                    .fold(f64::INFINITY, f64::min);
                let radius = if shortest.is_finite() { (shortest * 0.1 * 10.0).round().max(1.0) / 10.0 } else { 1.0 };
                self.remember("sketch_fillet");
                self.tool = Tool::SketchFillet { all, corners, radius };
            }
            ToolKind::WrapEmboss => {
                // The manual's order: the sketch profiles, then one cylindrical or conical face; the step's card then
                // takes the Emboss depth (negative engraves) and the Rotation.
                let regions = self.session.selection.iter().any(|p| matches!(p, Pick::Region { .. }));
                let face = self.session.selection.iter().any(|p| match *p {
                    Pick::Face { body, face } => {
                        self.session.doc.body(body).is_some_and(|b| self.session.kernel.face_plane(&b.shape, face).ok().flatten().is_none())
                    }
                    _ => false,
                });
                if regions && face {
                    self.tool = Tool::Idle;
                    if self.run("wrap_emboss", json!({ "depth": 1.0 })).is_some() {
                        self.remember("wrap_emboss");
                        self.show_history = true;
                        self.history.open = self.session.doc.history.len().checked_sub(1);
                    }
                } else if !matches!(self.tool, Tool::Await { kind: ToolKind::WrapEmboss, .. }) {
                    let keep = self.session.selection.clone();
                    self.await_selection(ToolKind::WrapEmboss, "Select the sketch profiles to wrap, then a cylindrical or conical face");
                    self.session.selection = keep;
                }
            }
            ToolKind::Plane3 => {
                self.remember("plane_3points");
                self.session.selection.clear();
                self.tool = Tool::Plane3 { points: Vec::new() };
            }
            ToolKind::Text => {
                if self.session.active_sketch.is_none() {
                    self.toast("Text goes into a sketch: start one first");
                } else {
                    self.remember("text");
                    self.tool = Tool::Text { at: None, text: String::new(), size: 10.0, align: 0, angle: 0.0 };
                }
            }
            ToolKind::Trim => {
                // Outside a sketch: a picked sketch curve opens its sketch; nothing picked, it waits for one.
                let curve = self.session.selection.iter().find_map(|p| match *p {
                    Pick::SketchEntity { sketch, .. } => Some(sketch),
                    _ => None,
                });
                if self.session.active_sketch.is_none()
                    && let Some(sk) = curve
                {
                    self.open_sketch_in_place(sk);
                }
                if self.session.active_sketch.is_some() {
                    self.remember("trim");
                    self.tool = Tool::Trim { target: None };
                } else {
                    self.await_selection(ToolKind::Trim, "Select the parts of sketch curves between crossings to trim them away");
                }
            }
            ToolKind::OffsetEdge if matches!(self.session.selection.as_slice(), [Pick::Edge { .. }]) => {
                match tools::BodyEdgeOffsetTool::from_selection(&self.session, self.last_hit) {
                    Some(mut t) => {
                        t.face_viewer(self.camera.back());
                        self.tool = Tool::BodyEdgeOffset(t);
                    }
                    None => self.toast("Offset Edge needs a flat face next to the edge"),
                }
            }
            ToolKind::OffsetEdge => {
                let picked: Vec<u32> = self
                    .session
                    .selection
                    .iter()
                    .filter_map(|p| match *p {
                        Pick::SketchEntity { entity, .. } => Some(entity),
                        _ => None,
                    })
                    .collect();
                if self.variant("Offset Edge") == 1 {
                    self.select_chains();
                }
                match tools::OffsetEdgeTool::from_selection(&self.session).or_else(|| tools::OffsetEdgeTool::from_face(&self.session)) {
                    Some(mut t) => {
                        if !picked.is_empty() {
                            t.picked = picked;
                        }
                        self.remember("offset_edge");
                        self.tool = Tool::OffsetEdge(t);
                    }
                    None => self.await_selection(ToolKind::OffsetEdge, "Select edges or faces to create an offset sketch"),
                }
            }
            ToolKind::Loft => {
                self.remember("loft");
                let t = tools::LoftTool::from_selection(&self.session);
                self.session.selection = t.profiles.clone();
                self.tool = Tool::Loft(t);
            }
            ToolKind::Sweep => match tools::SweepTool::from_selection(&self.session) {
                Some(t) => {
                    self.remember("sweep");
                    self.session.selection = t.picks();
                    self.tool = Tool::Sweep(t);
                }
                None => self.await_selection(ToolKind::Sweep, "Select a region or flat face to sweep"),
            },
            ToolKind::Revolve => match tools::RevolveTool::from_selection(&self.session) {
                Some(t) => {
                    self.remember("revolve");
                    self.tool = Tool::Revolve(t);
                }
                None => self.await_selection(ToolKind::Revolve, "Select a sketch region or flat face to revolve"),
            },
        }
    }

    /// As in the reference, a fresh selection starts the tool it predicts: a face → Offset Face, an edge →
    /// Chamfer/Fillet, a whole body → Move/Rotate (handles show at once). Regions only get the hint.
    pub(super) fn predict_tool(&mut self) {
        if !matches!(self.tool, Tool::Idle) || self.session.active_sketch.is_some() || self.visualizing || self.drawings.is_some() {
            return;
        }
        // Faces or bodies picked with one edge: they turn around it (Rotate Around Axis, the edge as the axis).
        if let Some(t) = tools::RotateAxisTool::items_and_axis(&self.session) {
            self.tool = Tool::RotateAxis(t);
            self.predicted = true;
            return;
        }
        let kind = match self.session.selection_kind() {
            shapecraft_engine::SelectionKind::Faces => ToolKind::OffsetFace,
            // A sketch region starts Extrude right away, arrow and all, as observed.
            shapecraft_engine::SelectionKind::Regions => ToolKind::Extrude,
            shapecraft_engine::SelectionKind::Edges => ToolKind::Fillet,
            // Bodies and construction planes bring up the Move/Rotate gizmo, as observed.
            shapecraft_engine::SelectionKind::Bodies | shapecraft_engine::SelectionKind::Planes => ToolKind::Move,
            _ => return,
        };
        self.start_tool(kind);
        self.predicted = true;
    }

    /// What Scale waits for: bodies, or in a sketch its curves.
    fn scale_hint(&self) -> &'static str {
        if self.session.active_sketch.is_some() { "Select sketch curves to scale up or down" } else { "Select bodies to scale" }
    }

    /// The selection changed (the Items list, Select All, Select Through, a click): a tool it only predicted gives
    /// way to what the new selection predicts, so a body picked in Items brings up the Move gizmo instead of
    /// leaving the old face's Offset Face arrow up. A tool the user started keeps going.
    pub(crate) fn repredict(&mut self) {
        if self.predicted {
            self.end_model_tool();
        }
        if matches!(self.tool, Tool::Idle) {
            self.predict_tool();
        }
    }

    /// 26.200 keeps the History sidebar open while sketching (26.142 closed it).
    fn enter_plane_pick(&mut self) {
        self.tool = Tool::PickPlane { hovered: None };
    }
}
