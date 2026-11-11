//! Committing tools: applying a tool's result to the document, and adding the sketch entities the drawing tools
//! make.

use glam::DVec2;
use serde_json::json;
use shapecraft_doc::{ItemId, Pick, SketchEntity};

use super::*;
use crate::workspace::Workspace;

impl Workspace {
    /// Replace Face / Project as Edge with new picks or options: tries it live (from the model as it was), keeping
    /// a refusal for the card.
    pub(in crate::workspace) fn apply_onto(&mut self, mut t: OntoTool) {
        if self.is_live() {
            self.session.drop_last_step();
            self.live_step = None;
        }
        if let Some(params) = t.params() {
            match self.session.execute(t.command(), &params) {
                Ok(_) => {
                    self.live_step = Some(self.session.revision);
                    self.session.selection.clear();
                }
                Err(e) => {
                    t.error = Some(match e {
                        shapecraft_engine::EngineError::NotAvailable(m) | shapecraft_engine::EngineError::BadParams(m) => m,
                        other => other.to_string(),
                    });
                    // Replace Face: a refused face of the same body joins the faces to replace (s11-126/127).
                    if !t.project
                        && let Some((to, at)) = t.to.take()
                        && let (Some(b), Some(f)) = (to["body"].as_u64(), to["face"].as_u64())
                        && t.body().is_some_and(|x| x.0 == b)
                    {
                        t.items.push((Pick::Face { body: ItemId(b), face: f as u32 }, at));
                    }
                }
            }
        }
        if !self.is_live() {
            self.session.selection = t.items.iter().map(|(p, _)| *p).collect();
        }
        self.tool = Tool::Onto(t);
    }

    /// Split Body with new splitting tools: tried live (on the model as it was); a refusal takes the banner's place
    /// and the tools stay picked (s12-004).
    pub(in crate::workspace) fn apply_split(&mut self, mut t: MirrorTool) {
        if self.is_live() {
            self.session.drop_last_step();
            self.live_step = None;
        }
        t.pieces.clear();
        if t.with.is_empty() {
            self.session.selection = t.items.iter().map(|&body| Pick::Body { body }).collect();
        } else {
            let with: Vec<serde_json::Value> = t.with.iter().map(|(w, _)| w.clone()).collect();
            let params = json!({ "items": t.items.iter().map(|i| i.0).collect::<Vec<_>>(), "with": with, "keep": t.keep });
            match self.session.execute("split_body", &params) {
                Ok(r) => {
                    self.live_step = Some(self.session.revision);
                    t.pieces = r.value["bodies"].as_array().into_iter().flatten().filter_map(|v| v.as_u64()).map(ItemId).collect();
                }
                Err(e) => {
                    let text = match e {
                        shapecraft_engine::EngineError::NotAvailable(m) | shapecraft_engine::EngineError::BadParams(m) => m,
                        other => other.to_string(),
                    };
                    self.notice = Some((text, self.now + 3.0));
                }
            }
            // Once a splitting tool is in, the idle set shows with Split Body below it (s12-004).
            self.session.selection.clear();
        }
        self.tool = Tool::Mirror(t);
    }

    /// Return: commits whatever the tool has.
    pub(in crate::workspace) fn commit_tool(&mut self) {
        let kind = self.typed_kind();
        let typed = self.numeric.take().and_then(|t| self.eval_field(&t, kind).ok());
        match self.tool.clone() {
            Tool::Extrude(mut t) => {
                if let Some(v) = typed {
                    t.distance = v;
                }
                self.apply_extrude(&t);
            }
            Tool::Fillet(mut t) => {
                if let Some(v) = typed {
                    t.radius = v;
                }
                self.apply_fillet(&t);
            }
            Tool::Shell(mut t) => {
                if let Some(v) = typed {
                    t.thickness = v;
                }
                if t.thickness > 1e-9
                    && self.run_live("shell", json!({ "body": t.body.0, "faces": t.faces, "thickness": t.thickness }), false)
                {
                    self.tool = Tool::Shell(ShellTool { dragging: None, ..t });
                }
            }
            Tool::OffsetFace(mut t) if t.fillet.is_some() => {
                // A fillet's face edits the fillet's own step (history replays); the tool stays on the new radius.
                if let Some(v) = typed {
                    t.set_shown(self.offset_kind(&t), v);
                }
                if t.distance.abs() > 1e-9 {
                    let (id, params) = t.request(t.distance);
                    let r = t.shown(self.offset_kind(&t));
                    if self.run(id, params).is_some() {
                        self.live_step = None;
                        if let (Some(f), true) = (t.fillet.as_mut(), r > 1e-6) {
                            f.radius = r;
                            t.distance = 0.0;
                            t.dragging = None;
                            self.session.selection = t.faces.iter().map(|&face| Pick::Face { body: t.body, face }).collect();
                            self.tool = Tool::OffsetFace(t);
                        } else {
                            self.tool = Tool::OffsetFace(OffsetFaceTool { fillet: None, dragging: None, distance: 0.0, ..t });
                            self.session.selection.clear();
                        }
                    }
                }
            }
            Tool::OffsetFace(mut t) => {
                if let Some(v) = typed {
                    t.set_shown(self.offset_kind(&t), v);
                }
                if t.distance.abs() > 1e-9 || self.is_live() {
                    let (id, params) = t.request(t.distance);
                    if self.run_live(id, params, t.distance.abs() < 1e-9) {
                        self.tool = Tool::OffsetFace(OffsetFaceTool { dragging: None, ..t });
                    }
                }
            }
            Tool::Scale(mut t) => {
                // A plain number is a factor here, not a length. Like Move, the step stays live (as observed): the
                // handle and its value stay, and pulling it again changes the same step.
                let f = typed.unwrap_or(t.factor);
                let (id, params) = t.request(f);
                if self.run_live(id, params, !t.changed(f)) {
                    t.factor = f;
                    self.tool = Tool::Scale(t);
                }
            }
            Tool::Move(mut t) => {
                let v = match t.handle {
                    Handle::Arrow(_) | Handle::Plane(_) => typed.unwrap_or(t.value),
                    Handle::Ring(_) => self.numeric_angle.take().unwrap_or(t.value),
                };
                // As observed, the move stays live: the gizmo and its value stay where the drag left them, and
                // another drag of the same handle (or a typed value) changes the same step.
                let (id, params) = t.request(v);
                if self.run_live(id, params, !t.moves(v)) {
                    t.value = v;
                    self.tool = Tool::Move(t);
                }
            }
            Tool::Pattern(mut t) => {
                if let Some(v) = typed {
                    t.spacing = v;
                }
                if t.spacing.abs() > 1e-9 {
                    let (id, params) = t.request(t.spacing);
                    if self.run(id, params).is_some() {
                        self.tool = Tool::Idle;
                    }
                }
            }
            Tool::OffsetPlane(t) => {
                let (id, params) = t.request(typed.unwrap_or(t.distance));
                if self.run(id, params).is_some() {
                    self.tool = Tool::Idle;
                }
            }
            Tool::OffsetEdge(t) => {
                let d = typed.unwrap_or(t.distance);
                if d.abs() > 1e-9 {
                    let (id, params) = t.request(d);
                    if self.run(id, params).is_some() {
                        self.tool = Tool::Idle;
                    }
                }
            }
            Tool::Loft(t) => {
                if let Some((id, params)) = t.request()
                    && self.run(id, params).is_some()
                {
                    self.tool = Tool::Idle;
                }
            }
            Tool::Boolean(t) => {
                if t.ready() {
                    let mut ok = true;
                    for (id, params) in t.requests() {
                        ok &= self.run(id, params).is_some();
                    }
                    if ok {
                        self.tool = Tool::Idle;
                        self.session.selection.clear();
                    }
                }
            }
            Tool::BodyEdgeOffset(t) => {
                let d = typed.unwrap_or(t.distance);
                if let Some((id, params)) = t.request(d)
                    && self.run(id, params).is_some()
                {
                    self.tool = Tool::Idle;
                }
            }
            Tool::EdgePlane(mut t) => {
                let v = if t.through { self.numeric_angle.take().unwrap_or(t.value) } else { t.value };
                let (id, params) = t.request(v);
                if self.run_live(id, params, false) {
                    t.value = v;
                    t.dragging = None;
                    // The edge stays selected, as observed (345), not the plane.
                    self.session.selection = vec![Pick::Edge { body: t.body, edge: t.edge }];
                    self.tool = Tool::EdgePlane(t);
                }
            }
            Tool::Translate { items, from: Some(f), to: Some(t), copy } => {
                let by = t - f;
                let ids: Vec<u64> = items.iter().map(|i| i.0).collect();
                if self.run("translate", json!({ "by": [by.x, by.y, by.z], "items": ids, "copy": copy })).is_some() {
                    // As observed, the moved items stay selected and Move/Rotate takes over.
                    self.tool = Tool::Idle;
                    self.predict_tool();
                }
            }
            Tool::Sweep(t) => {
                if let Some((id, params)) = t.request()
                    && self.run(id, params).is_some()
                {
                    self.tool = Tool::Idle;
                }
            }
            Tool::Revolve(mut t) => {
                // Typed values are angles here, not lengths, unless Tab moved on to the height.
                let typed_angle = self.numeric_angle.take();
                let angle = if t.height_typed {
                    if let Some(h) = typed {
                        t.height = h;
                    }
                    t.angle.clamp(-t.max_angle(), t.max_angle())
                } else {
                    typed_angle.unwrap_or(t.angle)
                };
                if let Some(params) = t.params(angle)
                    && self.run("revolve", params).is_some()
                {
                    self.tool = Tool::Idle;
                } else {
                    // Refused (turns that overlap, say): the tool stays with what was typed, to change it.
                    t.angle = angle;
                    self.numeric = None;
                    self.tool = Tool::Revolve(t);
                }
            }
            Tool::CircularPattern(t) => {
                let angle = self.numeric_angle.take().unwrap_or(t.angle);
                if let Some((id, params)) = t.request(angle)
                    && self.run(id, params).is_some()
                {
                    self.tool = Tool::Idle;
                }
            }
            Tool::RotateAxis(mut t) => {
                // Live like Move: the turn applies at once and the handle stays to turn it further; Done ends it.
                let angle = self.numeric_angle.take().unwrap_or(t.angle);
                if let Some((id, params)) = t.request(angle)
                    && self.run_live(id, params, angle.abs() < 1e-9)
                {
                    t.angle = angle;
                    t.dragging = None;
                    self.tool = Tool::RotateAxis(t);
                }
            }
            // Return: a typed (or Tab-fixed) length places the segment toward the pointer, then the chain ends
            // (s11-037); without one it just ends.
            Tool::Line { last, .. } => {
                if let (Some(l), Some(cursor)) = (last, self.sketch_cursor())
                    && let Some(len) = typed.map(f64::abs).or(self.line_len)
                    && len > 1e-9
                    && self.add_line(l, l + (cursor - l).normalize_or(DVec2::X) * len)
                {
                    self.drive("distance", len, |_| 0.0);
                }
                self.line_len = None;
                self.tool = Tool::Line { first: None, last: None };
            }
            Tool::Text { at: Some(at), text, size, align, angle } => self.place_text(at, &text, size, align, angle),
            Tool::SketchFillet { corners, radius, .. } => {
                let r = typed.map(f64::abs).unwrap_or(radius);
                let pts: Vec<[f64; 2]> = corners.iter().map(|c| [c.x, c.y]).collect();
                if !pts.is_empty() && self.run("sketch_fillet", json!({ "corners": pts, "radius": r })).is_some() {
                    self.tool = Tool::Idle;
                }
            }
            // Return takes the point under the pointer as the last one (s11-069).
            Tool::Spline { mut points, control } => {
                if let Some(c) = self.sketch_cursor()
                    && points.last().is_some_and(|l| l.distance(c) > 1e-6)
                {
                    points.push(c);
                }
                self.finish_spline(points, control)
            }
            // Return with a typed sweep makes the arc as previewed (to the pointer, or between the placed ends).
            Tool::Arc { a: Some(a), b } => {
                if typed.is_some()
                    && let Some(cursor) = self.sketch_cursor()
                {
                    // The preview reads the typed text, so put it back for the call.
                    self.numeric = typed.map(|v| v.to_string());
                    let arc = self.arc_preview(a, b, cursor);
                    self.numeric = None;
                    if let Some((p, mid, q)) = arc {
                        self.add_arc(p, mid, q);
                    }
                }
            }
            Tool::Rect3 { a: Some(a), b: Some(b) } => {
                if let (Some(v), Some(cursor)) = (typed, self.sketch_cursor()) {
                    let n = (b - a).perp().normalize_or_zero();
                    let w = if (cursor - b).dot(n) < 0.0 { -v } else { v };
                    self.add_rect3(a, b, b + n * w);
                }
            }
            // Width fixed with Tab: the typed value (or the cursor) gives the height, on the cursor's side.
            Tool::Rectangle { first: Some(a) } if self.rect_width.is_some() => {
                let fixed_w = self.rect_width;
                if let Some(cursor) = self.sketch_cursor() {
                    let c = self.rect_cursor(a, cursor);
                    let c = match typed {
                        Some(h) => {
                            let sy = if cursor.y < a.y { -1.0 } else { 1.0 };
                            let half = if self.variant("Rectangle") == 0 { 0.5 } else { 1.0 };
                            DVec2::new(c.x, a.y + sy * h.abs().max(1e-6) * half)
                        }
                        None => c,
                    };
                    self.add_rectangle(self.rect_corner(a, c), c);
                    self.drive_rect(fixed_w, typed);
                }
            }
            Tool::Rectangle { first: Some(a) } if self.variant("Rectangle") == 0 => {
                if let (Some(v), Some(cursor)) = (typed, self.sketch_cursor()) {
                    // Center: the typed width is the whole width, keeping the cursor's aspect.
                    let d = (cursor - a).abs();
                    let h = if d.x > 1e-9 { (d.y / d.x * v).max(1e-6) } else { v };
                    self.add_rectangle(a - DVec2::new(v, h) / 2.0, a + DVec2::new(v, h) / 2.0);
                    self.drive_rect(Some(v.abs()), None);
                }
            }
            Tool::Rectangle { first: Some(a) } => {
                if let (Some(v), Some(cursor)) = (typed, self.sketch_cursor()) {
                    // A typed width keeps the direction the cursor is pulling in, with a square aspect.
                    let d = cursor - a;
                    let sx = if d.x < 0.0 { -1.0 } else { 1.0 };
                    let sy = if d.y < 0.0 { -1.0 } else { 1.0 };
                    let h = if d.x.abs() > 1e-9 { (d.y.abs() / d.x.abs() * v).max(1e-6) } else { v };
                    self.add_rectangle(a, a + DVec2::new(sx * v, sy * h));
                    self.drive_rect(Some(v.abs()), None);
                }
            }
            Tool::Circle { center: Some(c) } => {
                // The typed value is the diameter, as observed ("Ø 20 mm"), unless Preferences ask for radius.
                if let Some(v) = typed {
                    let r = if self.settings.circular_annotations == "Always Radius" { v } else { v / 2.0 };
                    self.add_circle(c, r.abs());
                    self.drive("radius", r.abs(), |_| 0.0);
                }
            }
            Tool::Ellipse { center: Some(c), major } => {
                if let (Some(v), Some(cursor)) = (typed, self.sketch_cursor()) {
                    match major {
                        // The first typed value is the half-axis toward the cursor.
                        None => {
                            let m = c + (cursor - c).normalize_or(DVec2::X) * v.abs();
                            self.tool = Tool::Ellipse { center: Some(c), major: Some(m) };
                        }
                        Some(m) => self.add_ellipse(c, m, v.abs()),
                    }
                }
            }
            Tool::Polygon { center: Some(c) } => {
                if let Some(r) = typed {
                    let angle = self.sketch_cursor().map(|p| (p - c).to_angle()).unwrap_or(std::f64::consts::FRAC_PI_2);
                    self.add_polygon(c, r, angle);
                }
            }
            // Applied live already: Return is Done (Replace Face, Project as Edge, Split Body).
            Tool::Onto(_) | Tool::Mirror(MirrorTool { split: true, .. }) if self.is_live() => {
                self.live_step = None;
                self.tool = Tool::Idle;
            }
            _ => {}
        }
    }

    pub(in crate::workspace) fn next_numeric_field(&mut self) {
        // A rectangle has two fields, as observed: Tab fixes the typed width and opens the height.
        if matches!(self.tool, Tool::Rectangle { first: Some(_) })
            && self.rect_width.is_none()
            && let Some(w) = self.numeric.as_deref().and_then(|t| self.eval_typed(t))
            && w.abs() > 1e-9
        {
            self.rect_width = Some(w.abs());
            self.numeric = Some(String::new());
            return;
        }
        // A line: Tab fixes the typed length and the pointer only aims it (s11-035).
        if matches!(self.tool, Tool::Line { last: Some(_), .. })
            && let Some(v) = self.numeric.as_deref().and_then(|t| self.eval_typed(t))
            && v.abs() > 1e-9
        {
            self.line_len = Some(v.abs());
            self.numeric = None;
            return;
        }
        // Revolve: Tab fixes the typed angle and opens the height (a helix rises as it turns).
        let typed_angle = self.numeric.as_deref().and_then(|n| self.eval_angle(n)).filter(|a| a.abs() > 1e-9);
        if let Tool::Revolve(t) = &mut self.tool
            && !t.height_typed
        {
            if let Some(a) = typed_angle {
                t.angle = a.clamp(-360.0 * 200.0, 360.0 * 200.0);
            }
            t.height_typed = true;
            self.numeric = Some(String::new());
            return;
        }
        // Other tools have one field: Tab commits it like Return would.
        self.commit_tool();
    }

    pub(in crate::workspace) fn apply_extrude(&mut self, t: &ExtrudeTool) {
        if t.distance.abs() < 1e-9 && !self.is_live() {
            return;
        }
        let (id, params) = t.request(t.distance);
        // The picked region (or face) stays picked while you keep pulling (26.200, s11-101).
        let keep = self.session.selection.clone();
        if self.run_live(id, params, t.distance.abs() < 1e-9) {
            let shapes_left = keep.iter().all(|p| match *p {
                Pick::Region { sketch, region } => self.session.doc.sketch(sketch).is_some_and(|s| s.region(region).is_some()),
                Pick::Face { body, .. } => self.session.doc.body(body).is_some(),
                _ => true,
            });
            if shapes_left {
                self.session.selection = keep;
            }
            // Stay in the tool, still measured from where it started, like the reference: pulling again changes the
            // same extrusion.
            self.tool = Tool::Extrude(ExtrudeTool { dragging: None, drafting: None, ..t.clone() });
        }
    }

    pub(in crate::workspace) fn apply_fillet(&mut self, t: &FilletTool) {
        if t.radius <= 1e-9 {
            return;
        }
        let (id, params) = fillet_request(t, t.radius);
        if self.run_live(id, params, false) {
            // The rounded body isn't shown selected afterwards, as observed (307).
            self.session.selection.clear();
            self.tool = Tool::Fillet(FilletTool { dragging: None, ..t.clone() });
        }
    }

    /// The reference selects what you just drew (its dimensions stay up, Delete lights up).
    fn select_new_entity(&mut self, v: &serde_json::Value) {
        let Some(sketch) = v["sketch"].as_u64() else { return };
        // A shape made of several elements (a rectangle's four sides) is selected whole.
        let entities: Vec<u64> = match v["entities"].as_array() {
            Some(list) => list.iter().filter_map(serde_json::Value::as_u64).collect(),
            None => v["entity"].as_u64().into_iter().collect(),
        };
        self.session.selection =
            entities.into_iter().map(|entity| Pick::SketchEntity { sketch: ItemId(sketch), entity: entity as u32 }).collect();
        self.just_drawn = self.session.selection.clone();
    }

    pub(in crate::workspace) fn add_rectangle(&mut self, a: DVec2, b: DVec2) {
        self.rect_width = None;
        if let Some(v) = self.run("rectangle", json!({ "corner": [a.x, a.y], "opposite": [b.x, b.y] })) {
            self.select_new_entity(&v);
            self.tool = Tool::Rectangle { first: None };
        }
    }

    /// Rectangle, 3 Points: side a→b, width out to c.
    pub(in crate::workspace) fn add_rect3(&mut self, a: DVec2, b: DVec2, c: DVec2) {
        if let Some(v) = self.run("rectangle", json!({ "points": [[a.x, a.y], [b.x, b.y], [c.x, c.y]] })) {
            self.select_new_entity(&v);
            self.tool = Tool::Rect3 { a: None, b: None };
        }
    }

    pub(in crate::workspace) fn add_circle(&mut self, c: DVec2, r: f64) {
        if let Some(v) = self.run("circle", json!({ "center": [c.x, c.y], "radius": r })) {
            self.select_new_entity(&v);
            self.tool = Tool::Circle { center: None };
        }
    }

    pub(in crate::workspace) fn add_polygon(&mut self, c: DVec2, r: f64, angle: f64) {
        if let Some(v) =
            self.run("polygon", json!({ "center": [c.x, c.y], "radius": r, "sides": self.polygon_sides(), "angle": angle.to_degrees() }))
        {
            self.select_new_entity(&v);
            self.tool = Tool::Polygon { center: None };
        }
    }

    /// Writes the placed text into the sketch, one text element per line, and leaves the tool.
    pub(in crate::workspace) fn place_text(&mut self, at: DVec2, text: &str, size: f64, align: u8, angle: f64) {
        let font = self.text_font.clone();
        for (origin, line) in text_lines(text, size, align, at, angle, font.as_deref()) {
            let _ = self
                .run("text", json!({ "at": [origin.x, origin.y], "text": line, "size": size, "angle": angle.to_degrees(), "font": font }));
        }
        self.tool = Tool::Idle;
    }

    pub(in crate::workspace) fn add_arc(&mut self, a: DVec2, mid: DVec2, b: DVec2) {
        if let Some(v) = self.run("arc", json!({ "a": [a.x, a.y], "mid": [mid.x, mid.y], "b": [b.x, b.y] })) {
            self.select_new_entity(&v);
            self.tool = Tool::Arc { a: None, b: None };
        }
    }

    /// An ellipse from its center, the end of its first axis and the other half-axis.
    pub(in crate::workspace) fn add_ellipse(&mut self, c: DVec2, major: DVec2, ry: f64) {
        let rx = c.distance(major);
        if rx < 1e-6 || ry < 1e-6 {
            return;
        }
        let angle = (major - c).to_angle().to_degrees();
        if let Some(v) = self.run("ellipse", json!({ "center": [c.x, c.y], "rx": rx, "ry": ry, "angle": angle })) {
            self.select_new_entity(&v);
            self.tool = Tool::Ellipse { center: None, major: None };
        }
    }

    pub(in crate::workspace) fn finish_spline(&mut self, points: Vec<DVec2>, control: bool) {
        if points.len() >= 2 {
            let pts: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
            let params = if control { json!({ "points": pts, "control": true }) } else { json!({ "points": pts }) };
            if let Some(v) = self.run("spline", params) {
                self.select_new_entity(&v);
            }
        }
        self.tool = Tool::Spline { points: Vec::new(), control };
    }

    /// A typed value stays as a driving dimension on what was just drawn (26.200, s11-037/056): `kind` is
    /// "distance" (a line's length) or "radius"; `pick` chooses among the new elements.
    pub(in crate::workspace) fn drive(&mut self, kind: &str, value: f64, pick: impl Fn(&SketchEntity) -> f64) {
        let Some(sk) = self.session.active_sketch() else { return };
        let best = self
            .session
            .selection
            .iter()
            .filter_map(|p| match *p {
                Pick::SketchEntity { sketch, entity } if sketch == sk.id => sk.entities.get(entity as usize).map(|e| (entity, pick(e))),
                _ => None,
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((entity, score)) = best
            && score > f64::NEG_INFINITY
        {
            let keep = self.session.selection.clone();
            let _ = self.run("constrain", json!({ "kind": kind, "value": value, "entities": [entity] }));
            self.session.selection = keep;
        }
    }

    /// Typed rectangle sizes stay as dimensions: the width on its most sideways side, the height on its most
    /// upright one.
    fn drive_rect(&mut self, width: Option<f64>, height: Option<f64>) {
        let along = |e: &SketchEntity, x: bool| match *e {
            // Ties go to the top side for the width and the right side for the height, as shown (s11-056).
            SketchEntity::Line { a, b } => {
                let d = (b - a).normalize_or_zero();
                let m = (a + b) / 2.0;
                if x { d.x.abs() + m.y * 1e-6 } else { d.y.abs() + m.x * 1e-6 }
            }
            _ => f64::NEG_INFINITY,
        };
        if let Some(w) = width.filter(|w| *w > 1e-9) {
            self.drive("distance", w, |e| along(e, true));
        }
        if let Some(h) = height.map(f64::abs).filter(|h| *h > 1e-9) {
            self.drive("distance", h, |e| along(e, false));
        }
    }

    pub(in crate::workspace) fn add_line(&mut self, a: DVec2, b: DVec2) -> bool {
        match self.run("line", json!({ "a": [a.x, a.y], "b": [b.x, b.y] })) {
            Some(v) => {
                self.select_new_entity(&v);
                true
            }
            None => false,
        }
    }
}
