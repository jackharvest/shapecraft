//! A tutorial running in the workspace (see `crate::tutorials` for the lessons): no sidebars and only the tools the
//! step needs; top left a ring that fills as you go with "Step N out of M" beside it (the ring asks whether to quit); an
//! instruction card at the top that moves on as you go and holds its own buttons (the value to set, Do It for Me,
//! Continue); Undo, Redo and Retry on the left. Each step opens on a large picture of the step being done that shrinks
//! into the corner and keeps playing there, and an animated pointer shows the gesture the instruction asks for, on the
//! very thing it's asked of.
//!
//! [`Workspace::tutorial_gestures`] works out that gesture in screen points. The pointer plays it, and so does the
//! snapshot tool's `play_tutorial`, with real events: what's shown is what works.

use std::collections::HashMap;

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use glam::DVec3;
use serde_json::Value;
use shapecraft_engine::Session;

use super::Workspace;
use super::toolbar::{self, Action, ToolDef, ToolKind, UiAction};
use super::tools::{Handle, Tool};
use crate::icons::Icon;
use crate::theme::{self, Tokens, radius};
use crate::tutorials::{self, Goal, Lesson, Live, Show, Snapshot};

/// What the tutorial asks the app to do (it builds each step's workspace).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Request {
    /// The same step again, from its own scene.
    Retry,
    /// The step done for you.
    Solve,
    Next,
    Quit,
    /// The finished part, as a project of your own.
    Keep,
    /// The lesson that comes after this one.
    NextLesson,
}

/// A pointer gesture in screen points.
#[derive(Clone, Debug, PartialEq)]
pub enum Gesture {
    Click(Pos2),
    DoubleClick(Pos2),
    Drag(Pos2, Pos2),
}

/// The step being done, as pictures from nothing to done, worked out a picture a frame.
pub struct Clip {
    /// The step's scene and what its commands named.
    pub scene: Session,
    pub vars: HashMap<&'static str, Value>,
    pub frames: Vec<egui::TextureHandle>,
    /// How far along each picture is.
    pub wanted: Vec<f64>,
    pub next: usize,
    pub bounds: Option<(DVec3, DVec3)>,
}

pub struct TutorialRun {
    pub lesson: &'static Lesson,
    pub step: usize,
    /// The instruction showing (an index into the step's hints); all of them met means the step is done.
    pub hint: usize,
    pub base: Snapshot,
    now: Snapshot,
    seen_revision: u64,
    base_back: DVec3,
    turned: f64,
    /// How deep the undo history was when the step began: Undo never goes back into the lesson's own setup.
    floor: usize,
    /// When the current instruction came up.
    hint_since: f64,
    /// When what an earlier instruction set up was last seen to be there still.
    held: f64,
    /// When the step was done (the tick and the ring's last bit).
    done_at: Option<f64>,
    /// The step's caption shows large until then (and grows back from the corner when clicked).
    intro_until: Option<f64>,
    started: bool,
    quit_ask: bool,
    pub clip: Option<Clip>,
    /// Where the things an instruction can name were drawn this frame.
    pub spots: HashMap<&'static str, Rect>,
    pub request: Option<Request>,
}

/// How long the caption shows large, and how long it takes to shrink into the corner.
const INTRO_S: f64 = 2.8;
const SHRINK_S: f64 = 0.35;
/// How long a new instruction calls attention to itself.
const CALL_S: f64 = 1.6;
/// The corner's picture: this many steps of the solution, played over this long, then held.
pub const CLIP_FRAMES: usize = 14;
const CLIP_PLAY_S: f64 = 1.5;
const CLIP_HOLD_S: f64 = 1.3;
/// How long an instruction waits before Do It for Me shows.
const HELP_AFTER_S: f64 = 6.0;
/// The pointer's loop, and how long after an instruction it starts.
const POINTER_S: f64 = 2.4;
const POINTER_AFTER_S: f64 = 0.5;
/// How long something an earlier instruction set up can be gone before that instruction comes back.
const LOST_S: f64 = 0.6;

impl TutorialRun {
    pub fn new(lesson: &'static Lesson, step: usize, base: Snapshot) -> Self {
        Self {
            lesson,
            step,
            hint: 0,
            now: base.clone(),
            base,
            seen_revision: u64::MAX,
            base_back: DVec3::Z,
            turned: 0.0,
            floor: 0,
            hint_since: 0.0,
            held: 0.0,
            done_at: None,
            intro_until: None,
            started: false,
            quit_ask: false,
            clip: None,
            spots: HashMap::new(),
            request: None,
        }
    }

    pub fn step_def(&self) -> &'static tutorials::Step {
        &self.lesson.steps[self.step.min(self.lesson.steps.len().saturating_sub(1))]
    }

    pub fn done(&self) -> bool {
        self.hint >= self.step_def().hints.len()
    }

    pub fn last_step(&self) -> bool {
        self.step + 1 >= self.lesson.steps.len()
    }

    /// Whether a click may open a sketch that's on screen: only where the step has tools to draw with.
    pub fn sketches(&self) -> bool {
        !self.step_def().sketch_tools.is_empty()
    }
}

fn kind(tool: tutorials::Tool) -> Option<ToolKind> {
    Some(match tool {
        tutorials::Tool::Sketch => ToolKind::Sketch,
        tutorials::Tool::Line => ToolKind::Line,
        tutorials::Tool::Rectangle => ToolKind::Rectangle,
        tutorials::Tool::Circle => ToolKind::Circle,
        tutorials::Tool::Arc => ToolKind::Arc,
        tutorials::Tool::Extrude => ToolKind::Extrude,
        tutorials::Tool::Shell => ToolKind::Shell,
        tutorials::Tool::Fillet => ToolKind::Fillet,
        tutorials::Tool::Revolve => ToolKind::Revolve,
        tutorials::Tool::Sweep => ToolKind::Sweep,
        tutorials::Tool::Loft => ToolKind::Loft,
        tutorials::Tool::CircularPattern => ToolKind::CircularPattern,
        tutorials::Tool::Mirror => ToolKind::Mirror,
        tutorials::Tool::Union => ToolKind::Union,
        tutorials::Tool::Subtract => ToolKind::Subtract,
        tutorials::Tool::Thread => ToolKind::Thread,
        tutorials::Tool::Visualization => return None,
    })
}

fn def(tool: tutorials::Tool) -> ToolDef {
    let Some(kind) = kind(tool) else {
        return ToolDef {
            icon: Icon::Visualize,
            label: "Visualize",
            sub: None,
            keys: "",
            action: Action::Ui(UiAction::Visualization),
            flyout: false,
        };
    };
    let d = toolbar::def_for(kind).unwrap_or(ToolDef {
        icon: Icon::Sketch,
        label: "Sketch",
        sub: None,
        keys: "",
        action: Action::Tool(kind),
        flyout: false,
    });
    // The lesson's button runs the tool; its other types (a flyout) aren't part of it.
    ToolDef { flyout: false, ..d }
}

const fn plain(icon: Icon, label: &'static str) -> ToolDef {
    ToolDef { icon, label, sub: None, keys: "", action: Action::Ui(UiAction::Noop), flyout: false }
}

const EXIT: ToolDef =
    ToolDef { icon: Icon::Close, label: "Exit Sketching", sub: None, keys: "", action: Action::Ui(UiAction::ExitSketching), flyout: false };

impl Workspace {
    /// What the workspace is doing, as the lessons' goals ask it.
    fn tutorial_live(&self, t: &TutorialRun) -> Live {
        let sketch = self.session.active_sketch();
        let step = t.step_def();
        Live {
            picking_plane: matches!(self.tool, Tool::PickPlane { .. }),
            sketch_open: sketch.is_some(),
            sketch_on_face: sketch.is_some_and(|s| s.on_body.is_some()),
            selection: self.session.selection_kind(),
            running: step.tools.iter().chain(step.sketch_tools).filter(|k| self.tutorial_tool_on(**k)).fold(0, |bits, k| bits | k.bit()),
            ready: self.confirm_seen == Some(true),
            parts: match &self.tool {
                Tool::Boolean(b) => b.targets.len() + b.tools.len(),
                _ => 0,
            },
            turned: t.turned,
            from_top: self.camera.back().z > 0.999,
            visualizing: self.visualizing,
        }
    }

    /// Moves the tutorial on (instructions met, the step's goal), once a frame before drawing.
    pub(super) fn tutorial_tick(&mut self) {
        let Some(mut t) = self.tutorial.take() else { return };
        let back = self.camera.back();
        let now = self.now;
        if !t.started {
            t.started = true;
            t.base_back = back;
            t.floor = self.session.undo_depth();
            t.hint_since = now;
            t.held = now;
            t.intro_until = Some(now + INTRO_S);
        }
        // The camera still on its way to the step's view isn't the person turning it.
        if self.anim.is_some() && t.hint == 0 && now - t.hint_since < 1.0 {
            t.base_back = back;
        }
        if t.seen_revision != self.session.revision {
            t.now = tutorials::snapshot(&self.session);
            t.seen_revision = self.session.revision;
        }
        t.turned = t.turned.max(back.dot(t.base_back).clamp(-1.0, 1.0).acos().to_degrees());
        let live = self.tutorial_live(&t);
        let hints = t.step_def().hints;
        // A later instruction the document already shows done skips ahead (typing a size before selecting, say);
        // what's on screen (a selection, the camera) only counts in turn.
        while t.hint < hints.len() {
            let ahead = (t.hint..hints.len())
                .rev()
                .find(|&j| (j == t.hint || tutorials::on_document(hints[j].goal)) && tutorials::met(hints[j].goal, &t.base, &t.now, &live));
            match ahead {
                Some(j) => {
                    t.hint = j + 1;
                    t.hint_since = now;
                    t.held = now;
                }
                None => break,
            }
        }
        // What an earlier instruction set up has gone (the sketch was left, the pick let go): that instruction again.
        // A running tool takes the picks over, so they only count while none is.
        if t.hint < hints.len() {
            let busy = !matches!(self.tool, Tool::Idle | Tool::Await { .. }) || self.visualizing || self.working.is_some();
            let lost = hints[..t.hint].iter().position(|h| {
                let goal = h.goal;
                goal.holds() && !(busy && matches!(goal, Goal::Picked(_))) && !tutorials::met(goal, &t.base, &t.now, &live)
            });
            match lost {
                None => t.held = now,
                Some(j) if now - t.held > LOST_S => {
                    t.hint = j;
                    t.hint_since = now;
                    t.held = now;
                }
                Some(_) => {}
            }
        }
        // A new color shows on what isn't picked.
        if t.done() && t.done_at.is_none() && self.visualizing {
            self.session.selection.clear();
        }
        t.done_at = if t.done() { t.done_at.or(Some(now)) } else { None };
        self.tutorial = Some(t);
    }

    /// The instruction's gesture, in screen points, as things stand now (the tool's arrow where it is, the button where
    /// it was drawn). Empty when there's nothing to point at yet.
    pub fn tutorial_gestures(&self) -> Vec<Gesture> {
        let Some(t) = self.tutorial.as_deref() else { return Vec::new() };
        let Some(h) = t.step_def().hints.get(t.hint) else {
            return t.spots.get("continue").map(|r| Gesture::Click(r.center())).into_iter().collect();
        };
        // A drawing started on another plane than the lesson's goes on there.
        let drawn_on = self.session.active_sketch().map(|s| s.plane).filter(|_| t.step_def().setup.is_empty());
        let at = |p: tutorials::Spot| {
            let q = DVec3::from(p);
            self.to_screen(drawn_on.map_or(q, |plane| plane.to_world(q.truncate())))
        };
        let spot = |name: &str| t.spots.get(name).map(|r| r.center());
        let one = |p: Option<Pos2>| p.map(Gesture::Click).into_iter().collect();
        match h.show {
            Show::Tool(k) => one(spot(def(k).label)),
            Show::At(p) => one(at(p)),
            Show::Twice(p) => at(p).map(Gesture::DoubleClick).into_iter().collect(),
            Show::Two(a, b) => [at(a), at(b)].into_iter().flatten().map(Gesture::Click).collect(),
            Show::Pull(mm) => self.tutorial_pull(mm).into_iter().collect(),
            Show::Move(axis, mm) => self.tutorial_move(axis, mm).into_iter().collect(),
            Show::Value(_) => one(spot("value")),
            Show::Cube => spot("cube").map(|c| Gesture::Drag(c, c + vec2(-70.0, 8.0))).into_iter().collect(),
            Show::CubeTop => one(self.tutorial_cube_top()),
            Show::Plane(i) => one(self.mirror_tile_spot(i)),
            Show::Color => one(spot("color")),
            Show::Done => one(spot("Done")),
        }
    }

    /// The running tool's arrow, from where it can be taken hold of to where it has been pulled by `mm`: at least far
    /// enough to see.
    fn tutorial_pull(&self, mm: f64) -> Option<Gesture> {
        let (base, dir, _, gain) = self.arrow_handle()?;
        let (s0, sdir, per_mm) = self.handle_frame(base, dir)?;
        // (The fillet's ↕ sits just outside its point; the others' on it.)
        let from = if matches!(self.tool, Tool::Fillet(_)) { s0 - sdir * 20.0 } else { s0 };
        let by = (mm * per_mm / gain.max(1e-9)) as f32;
        let by = if by.abs() < 18.0 { 18.0 * by.signum() } else { by };
        Some(Gesture::Drag(from, from + sdir * by))
    }

    fn tutorial_move(&self, axis: usize, mm: f64) -> Option<Gesture> {
        let Tool::Move(m) = &self.tool else { return None };
        let pivot = m.shown_pivot();
        let shapes = self.gizmo_shapes(pivot, m.frame);
        let (_, pts) = shapes.into_iter().find(|(h, _)| *h == Handle::Arrow(axis))?;
        let (a, b) = (*pts.first()?, *pts.last()?);
        let (o, e) = (self.to_screen(pivot)?, self.to_screen(pivot + [DVec3::X, DVec3::Y, DVec3::Z][axis.min(2)] * mm)?);
        let from = a + (b - a) * 0.5;
        Some(Gesture::Drag(from, from + (e - o)))
    }

    /// The middle of the view cube's top face.
    fn tutorial_cube_top(&self) -> Option<Pos2> {
        let r = self.tutorial.as_deref()?.spots.get("cube")?;
        let (right, up) = self.camera.basis();
        let s = r.width() as f64 * 0.25;
        let p = DVec3::Z * 0.96;
        Some(r.center() + vec2((p.dot(right) * s) as f32, (-p.dot(up) * s) as f32))
    }

    /// Where something an instruction can name was drawn this frame.
    pub(super) fn tutorial_spot(&mut self, name: &'static str, r: Rect) {
        if let Some(t) = self.tutorial.as_deref_mut() {
            t.spots.insert(name, r);
        }
    }

    /// The instruction's own button: sets its value in the running tool, as typing it and Return would.
    fn tutorial_value(&mut self, value: &str) {
        self.numeric = Some(value.to_string());
        self.numeric_fresh = false;
        self.key(egui::Key::Enter, egui::Modifiers::NONE);
    }

    /// The tutorial's controls, in place of the toolbar, the right column and the hint banner.
    pub(super) fn tutorial_controls(&mut self, ui: &mut Ui) {
        let Some(t) = self.tutorial.as_deref() else { return };
        let vp = self.viewport_rect;
        let now = self.now;
        let step = t.step_def();
        let steps = t.lesson.steps.len();
        let (done, done_at) = (t.done(), t.done_at);
        let hint = step.hints.get(t.hint);
        let (hint_since, floor, last) = (t.hint_since, t.floor, t.last_step());
        let touch = crate::tablet::touch_ui() && !crate::tablet::shows_shortcuts();
        let moving = crate::motion::enabled();
        let p = ui.painter().clone();
        let x = vp.left() + 5.0;
        let top = if crate::tablet::touch_ui() { self.left_top } else { vp.top() + 9.0 };
        let mut request = None;
        let mut value = None;
        let mut spots: Vec<(&'static str, Rect)> = Vec::new();

        // Top left: the ring (how far through the lesson) and the step.
        let ring = Rect::from_min_size(pos2(x, top), vec2(Tokens::tool_button(), Tokens::tool_button()));
        let ring_resp = ui.interact(ring, ui.scope_id().with("tut-ring"), Sense::click());
        p.rect_filled(ring, radius(10), if ring_resp.hovered() { Tokens::button_hover() } else { Tokens::button() });
        p.rect_stroke(ring, radius(10), Stroke::new(1.0, Tokens::card_border()), StrokeKind::Inside);
        let part = if done { 1.0 } else { t.hint as f32 / step.hints.len().max(1) as f32 };
        let progress = crate::motion::tween(ui.ctx(), "tut-ring", (t.step as f32 + part) / steps.max(1) as f32, 0.35);
        let c = ring.center();
        p.circle_stroke(c, 9.0, Stroke::new(2.6, theme::c(0x4a4a4c)));
        let arc: Vec<Pos2> = (0..=40)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * progress.clamp(0.0, 1.0) * i as f32 / 40.0;
                c + vec2(a.cos(), a.sin()) * 9.0
            })
            .collect();
        p.add(egui::Shape::line(arc, Stroke::new(3.0, theme::hex(0x2e68ff))));
        let label = crate::i18n::tr("Step {n} out of {m}").replace("{n}", &(t.step + 1).to_string()).replace("{m}", &steps.to_string());
        let galley = p.layout_no_wrap(label, theme::strong(12.0), Tokens::text_strong());
        let pill = Rect::from_min_size(pos2(ring.right() + 6.0, c.y - 15.0), vec2(galley.size().x + 20.0, 30.0));
        p.rect_filled(pill, radius(8), Tokens::button());
        p.rect_stroke(pill, radius(8), Stroke::new(1.0, Tokens::card_border()), StrokeKind::Inside);
        p.galley(pos2(pill.left() + 10.0, c.y - galley.size().y / 2.0), galley, Tokens::text_strong());

        // Top centre: what to do now, with its own buttons.
        let shown_at = t.intro_until.map_or(hint_since, |u| hint_since.max(u + SHRINK_S));
        let (said, more) = match hint {
            Some(h) => {
                let words = |w: &'static str| crate::i18n::tr_s(if touch { tutorials::for_touch(w) } else { w });
                (words(h.text), h.tip.map(words))
            }
            None if last => (
                crate::i18n::tr("All done. Nicely done!").to_string(),
                Some(crate::i18n::tr_s("Keep it to go on working on it, or go on to the next.")),
            ),
            None => (crate::i18n::tr("Nicely done!").to_string(), None),
        };
        let mut buttons: Vec<(&'static str, String, bool)> = Vec::new();
        match hint {
            Some(h) => {
                if let Show::Value(v) = h.show {
                    let unit = match &self.tool {
                        Tool::Revolve(_) => "°",
                        _ => " mm",
                    };
                    buttons.push(("value", format!("{v}{unit}"), true));
                }
                if now - shown_at > HELP_AFTER_S && !step.solution.is_empty() {
                    buttons.push(("solve", crate::i18n::tr_s("Do It for Me"), false));
                }
            }
            None if last => {
                buttons.push(("keep", crate::i18n::tr_s("Keep as a Project"), false));
                if tutorials::next_lesson(t.lesson.id).is_some() {
                    buttons.push(("quit", crate::i18n::tr_s("All Tutorials"), false));
                    buttons.push(("continue", crate::i18n::tr_s("Next Tutorial"), true));
                } else {
                    buttons.push(("continue", crate::i18n::tr_s("Finish"), true));
                }
            }
            None => buttons.push(("continue", crate::i18n::tr_s("Continue"), true)),
        }
        let room = (vp.width() - 2.0 * (pill.right() - vp.left()) - 24.0).clamp(220.0, 520.0);
        let text = p.layout(said, theme::strong(14.0), Tokens::text_strong(), room - 60.0);
        let under = more.map(|m| p.layout(m, theme::body(11.5), Tokens::text_dim(), room - 60.0));
        let bh = if touch { 34.0 } else { 28.0 };
        let widths: Vec<f32> =
            buttons.iter().map(|(_, l, _)| p.layout_no_wrap(l.clone(), theme::strong(12.0), Color32::WHITE).size().x + 26.0).collect();
        let row_w: f32 = widths.iter().sum::<f32>() + 8.0 * buttons.len().saturating_sub(1) as f32;
        let text_w = text.size().x.max(under.as_ref().map_or(0.0, |u| u.size().x));
        // Short enough, the buttons sit beside the words; else in a row under them.
        let beside = !buttons.is_empty() && 34.0 + text_w + 16.0 + row_w + 14.0 <= room;
        let text_h = text.size().y + under.as_ref().map_or(0.0, |u| u.size().y + 3.0);
        let w = if beside { 34.0 + text_w + 16.0 + row_w + 12.0 } else { (34.0 + text_w).max(row_w + 12.0) + 16.0 };
        let h = if beside { text_h.max(bh) + 18.0 } else { text_h + 20.0 + if buttons.is_empty() { 0.0 } else { bh + 8.0 } };
        let center_x = vp.center().x.max(pill.right() + 12.0 + w / 2.0);
        let card = Rect::from_min_size(pos2(center_x - w / 2.0, self.banner_y() - 3.0), vec2(w, h));
        // A new instruction, or the step's picture just shrunk away: the card glows for a moment, so the eye finds it.
        let age = now - shown_at;
        let call = if (0.0..CALL_S).contains(&age) { (1.0 - age / CALL_S) as f32 } else { 0.0 };
        if call > 0.0 {
            ui.ctx().request_repaint();
            p.rect_filled(card.expand(3.0 + 5.0 * call), radius(18), Tokens::accent().gamma_multiply(0.4 * call));
        } else if hint.is_some() && age < HELP_AFTER_S {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((HELP_AFTER_S - age).clamp(0.05, 10.0)));
        }
        // (Taking clicks, so one on the words never picks what's behind.)
        let _ = ui.interact(card, ui.scope_id().with("tut-card-words"), Sense::click());
        let begin = crate::motion::pop_begin(&p);
        p.rect_filled(card.translate(vec2(0.0, 2.0)), radius(14), Color32::from_black_alpha(40));
        p.rect_filled(card, radius(14), if theme::is_light() { Color32::WHITE } else { theme::hex(0x2b2a2a) });
        let edge = if done { theme::hex(0x34c759) } else { Tokens::accent() };
        p.rect_stroke(card, radius(14), Stroke::new(1.0 + 0.8 * call, edge.gamma_multiply(0.45 + 0.55 * call)), StrokeKind::Inside);
        // Its number in a disc, a tick once the step's done.
        let disc = pos2(card.left() + 19.0, card.top() + 9.0 + text.size().y.max(if beside { text_h.max(bh) } else { 0.0 }) / 2.0);
        if done {
            let s = if moving { crate::motion::settle(((now - done_at.unwrap_or(now)) / 0.4).clamp(0.0, 1.0) as f32) } else { 1.0 };
            if s < 1.0 || now - done_at.unwrap_or(now) < 0.5 {
                ui.ctx().request_repaint();
            }
            p.circle_filled(disc, 9.0 * s.max(0.0), theme::hex(0x34c759));
            let pen = Stroke::new(1.8, Color32::WHITE);
            p.line_segment([disc + vec2(-4.0, 0.3) * s, disc + vec2(-1.2, 3.2) * s], pen);
            p.line_segment([disc + vec2(-1.2, 3.2) * s, disc + vec2(4.2, -3.2) * s], pen);
        } else {
            p.circle_filled(disc, 9.0, Tokens::accent());
            p.text(disc, Align2::CENTER_CENTER, (t.hint + 1).to_string(), theme::strong(11.0), Color32::WHITE);
        }
        let ty = if beside { card.center().y - text_h / 2.0 } else { card.top() + 10.0 };
        let text_y = text.size().y;
        p.galley(pos2(card.left() + 34.0, ty), text, Tokens::text_strong());
        if let Some(u) = under {
            p.galley(pos2(card.left() + 34.0, ty + text_y + 3.0), u, Tokens::text_dim());
        }
        let mut bx = card.right() - 12.0 - row_w;
        let by = if beside { card.center().y - bh / 2.0 } else { card.bottom() - 10.0 - bh };
        for ((id, label, primary), bw) in buttons.iter().zip(&widths) {
            let r = Rect::from_min_size(pos2(bx, by), vec2(*bw, bh));
            bx += bw + 8.0;
            let resp = ui.interact(r, ui.scope_id().with(("tut-btn", *id)), Sense::click());
            let fill = match (primary, done, resp.hovered()) {
                (true, true, false) => theme::hex(0x2a9d48),
                (true, true, true) => theme::hex(0x34c759),
                (true, false, false) => Tokens::accent(),
                (true, false, true) => Tokens::accent().gamma_multiply(0.85),
                (false, _, false) => Tokens::button_active(),
                (false, _, true) => Tokens::button_hover(),
            };
            p.rect_filled(r, radius(8), fill);
            let ink = if *primary { Color32::WHITE } else { Tokens::text_strong() };
            p.text(r.center(), Align2::CENTER_CENTER, label, theme::strong(12.0), ink);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            spots.push((id, r));
            if resp.clicked() {
                match *id {
                    "value" => {
                        if let Some(Show::Value(v)) = hint.map(|h| h.show) {
                            value = Some(v);
                        }
                    }
                    "solve" => request = Some(Request::Solve),
                    "keep" => request = Some(Request::Keep),
                    "quit" => request = Some(Request::Next),
                    _ if last && tutorials::next_lesson(t.lesson.id).is_some() => request = Some(Request::NextLesson),
                    _ => request = Some(Request::Next),
                }
            }
        }
        crate::motion::pop_end(&p, "tut-words", begin, card.center());
        spots.push(("card", card));

        // Left: the step's tools, then Undo, Redo and Retry; all of them there the whole step, so there is always a way
        // on and a way back.
        let sketching = self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. });
        let kinds = if sketching { step.sketch_tools } else { step.tools };
        let mut tools: Vec<ToolDef> = if done { Vec::new() } else { kinds.iter().map(|k| def(*k)).collect() };
        let mut active: Vec<bool> = if done { Vec::new() } else { kinds.iter().map(|k| self.tutorial_tool_on(*k)).collect() };
        if sketching && !done {
            tools.insert(0, EXIT);
            active.insert(0, false);
        }
        let back = [toolbar::UNDO, toolbar::REDO, plain(Icon::Retry, "Retry")];
        let gap = toolbar::GROUP_GAP;
        let h_tools = if tools.is_empty() { 0.0 } else { toolbar::group_height(tools.len()) + gap };
        let height = h_tools + toolbar::group_height(back.len());
        let mut y = (vp.center().y - height / 2.0).max(ring.bottom() + 20.0);
        let mut clicked: Option<ToolDef> = None;
        let mut undone = false;
        if !tools.is_empty() {
            if let Some(i) = toolbar::group(ui, pos2(x, y), &tools, &active, true, "tut-tools") {
                clicked = tools.get(i).copied();
            }
            for (i, d) in tools.iter().enumerate() {
                let cell = vec2(Tokens::tool_button() - 2.0, Tokens::tool_pitch());
                spots.push((d.label, Rect::from_min_size(pos2(x + 1.0, y + 1.0 + i as f32 * Tokens::tool_pitch()), cell)));
            }
            y += h_tools;
        }
        match toolbar::group(ui, pos2(x, y), &back, &[false; 3], true, "tut-back") {
            Some(2) => request = Some(Request::Retry),
            // Undo stops where the step began (Retry starts the step over).
            Some(i) if i == 1 || self.session.undo_depth() > floor => {
                clicked = back.get(i).copied();
                undone = true;
            }
            _ => {}
        }

        let skip_intro;
        let replay;
        {
            let Some(t) = self.tutorial.as_deref() else { return };
            (skip_intro, replay) = corner(ui, t, vp, now, moving);
        }

        // The ring asks before leaving.
        let mut quit_ask = self.tutorial.as_deref().is_some_and(|t| t.quit_ask);
        if ring_resp.clicked() {
            quit_ask = true;
        }
        if quit_ask {
            match quit_card(ui) {
                Some(true) => {
                    quit_ask = false;
                    request = Some(Request::Quit);
                }
                Some(false) => quit_ask = false,
                None => {}
            }
        }

        if let Some(v) = value {
            self.tutorial_value(v);
        }
        if let Some(def) = clicked {
            self.act(def.action, def.label);
        }
        let Some(t) = self.tutorial.as_deref_mut() else { return };
        t.spots.retain(|k, _| matches!(*k, "cube" | "Done" | "color"));
        t.spots.extend(spots);
        t.quit_ask = quit_ask;
        // After an undo or redo the step's instructions are checked again from the first (#16): an undone result
        // takes Continue away, and what still stands moves straight back to where it was.
        if undone {
            t.hint = 0;
            t.seen_revision = u64::MAX;
        }
        if skip_intro {
            t.intro_until = Some(now - 0.0001);
        } else if replay {
            t.intro_until = Some(now + INTRO_S);
        }
        if request.is_some() {
            t.request = request;
        }
        let pointer = !t.done() && !t.quit_ask && t.intro_until.is_none_or(|u| now > u + SHRINK_S);
        if pointer {
            self.tutorial_pointer(ui);
        }
    }

    /// The animated pointer: the instruction's gesture, played on the thing itself, over and over until it's done.
    fn tutorial_pointer(&self, ui: &Ui) {
        let Some(t) = self.tutorial.as_deref() else { return };
        let gestures = self.tutorial_gestures();
        let Some(first) = gestures.first() else { return };
        let since = self.now - t.intro_until.map_or(t.hint_since, |u| t.hint_since.max(u + SHRINK_S)) - POINTER_AFTER_S;
        if since < 0.0 {
            ui.ctx().request_repaint();
            return;
        }
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("tut-pointer")));
        // Out of the way while the person's own pointer is down.
        if ui.input(|i| i.pointer.any_down()) {
            return;
        }
        let place = |g: &Gesture| match g {
            Gesture::Click(a) | Gesture::DoubleClick(a) | Gesture::Drag(a, _) => *a,
        };
        let accent = Tokens::accent();
        // A button glows around its edge, so its words stay readable.
        let button = t.step_def().hints.get(t.hint).and_then(|h| match h.show {
            Show::Tool(k) => t.spots.get(def(k).label),
            Show::Value(_) => t.spots.get("value"),
            Show::Done => t.spots.get("Done"),
            _ => None,
        });
        if !crate::motion::enabled() {
            // Still: rings on what to press, an arrow for a drag.
            for g in &gestures {
                let a = place(g);
                match (button, g) {
                    (Some(r), _) => {
                        p.rect_stroke(r.expand(3.0), radius(11), Stroke::new(2.5, accent), StrokeKind::Outside);
                    }
                    (None, Gesture::Drag(a, b)) => p.arrow(*a, *b - *a, Stroke::new(2.5, accent)),
                    _ => {
                        p.circle_stroke(a, 13.0, Stroke::new(2.5, accent));
                    }
                }
            }
            return;
        }
        ui.ctx().request_repaint();
        let loop_s = POINTER_S + 0.7 * (gestures.len() - 1) as f64;
        let u = (since % loop_s) as f32;
        let smooth = |x: f32| {
            let x = x.clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let fade = smooth(u / 0.2) * (1.0 - smooth((u - (loop_s as f32 - 0.25)) / 0.25));
        // A ring that breathes on what's to be pressed, whatever the pointer is doing.
        for g in &gestures {
            let wave = (since as f32 * 1.1).fract();
            let ring = Stroke::new(2.0, accent.gamma_multiply(0.8 * (1.0 - wave)));
            match button {
                Some(r) => {
                    p.rect_stroke(r.expand(1.0 + 9.0 * wave), radius((10.0 + 8.0 * wave) as u8), ring, StrokeKind::Outside);
                }
                None => {
                    p.circle_stroke(place(g), 8.0 + 14.0 * wave, ring);
                }
            }
        }
        let start = place(first) + vec2(46.0, 52.0);
        // Where the pointer is, and whether it's pressing.
        let (at, press, trail) = match first {
            Gesture::Drag(a, b) if gestures.len() == 1 => {
                let s = smooth((u - 0.75) / 0.95);
                let at = if u < 0.6 { start + (*a - start) * smooth(u / 0.6) } else { *a + (*b - *a) * s };
                (at, u > 0.6 && u < 1.8, (u > 0.6).then_some(*a))
            }
            Gesture::DoubleClick(a) => {
                let at = start + (*a - start) * smooth(u / 0.7);
                (at, (0.75..0.87).contains(&u) || (1.0..1.12).contains(&u), None)
            }
            _ => {
                // One click after another.
                let k = (u / 0.9).floor().min((gestures.len() - 1) as f32) as usize;
                let from = if k == 0 { start } else { gestures.get(k - 1).map_or(start, place) };
                let to = gestures.get(k).map_or(start, place);
                let v = u - 0.9 * k as f32;
                (from + (to - from) * smooth(v / 0.6), (0.65..0.8).contains(&v), None)
            }
        };
        if let Some(a) = trail {
            p.line_segment([a, at], Stroke::new(2.5, accent.gamma_multiply(0.7 * fade)));
        }
        // The press's ripple, and the pointer itself: a hand-drawn arrow, smaller while pressing.
        if press {
            p.circle_filled(at, 11.0, accent.gamma_multiply(0.35 * fade));
            p.circle_stroke(at, 11.0, Stroke::new(1.5, accent.gamma_multiply(fade)));
        }
        let k = if press { 0.86 } else { 1.0 } * 1.25;
        let shape = [(0.0, 0.0), (0.0, 16.0), (3.9, 12.4), (6.6, 18.2), (9.2, 17.0), (6.6, 11.4), (11.8, 11.2)];
        let pts: Vec<Pos2> = shape.iter().map(|(x, y)| at + vec2(*x, *y) * k).collect();
        let shade: Vec<Pos2> = pts.iter().map(|q| *q + vec2(1.0, 2.0)).collect();
        let (black, white) = (Color32::from_black_alpha((90.0 * fade) as u8), Color32::WHITE.gamma_multiply(fade));
        arrow(&p, &shade, black, Stroke::NONE);
        arrow(&p, &pts, white, Stroke::new(1.4, Color32::from_black_alpha((230.0 * fade) as u8)));
    }

    /// Whether a lesson's tool is running.
    fn tutorial_tool_on(&self, k: tutorials::Tool) -> bool {
        match k {
            tutorials::Tool::Sketch => self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. }),
            tutorials::Tool::Visualization => self.visualizing,
            k => super::panels::tool_is_active(self, &def(k)),
        }
    }

    /// The step's goal and gesture, for the app (and the control channel).
    pub fn tutorial_state(&self) -> Option<Value> {
        let t = self.tutorial.as_deref()?;
        let step = t.step_def();
        let gestures: Vec<Value> = self
            .tutorial_gestures()
            .iter()
            .map(|g| match g {
                Gesture::Click(a) => serde_json::json!({ "click": [a.x, a.y] }),
                Gesture::DoubleClick(a) => serde_json::json!({ "double_click": [a.x, a.y] }),
                Gesture::Drag(a, b) => serde_json::json!({ "drag": [[a.x, a.y], [b.x, b.y]] }),
            })
            .collect();
        Some(serde_json::json!({
            "lesson": t.lesson.id,
            "step": t.step,
            "steps": t.lesson.steps.len(),
            "number": t.hint,
            "hint": step.hints.get(t.hint).map(|h| h.text),
            "done": t.done(),
            "gestures": gestures,
            "goal": step.hints.last().map(|h| format!("{:?}", h.goal)),
        }))
    }
}

/// A filled arrow pointer (its outline isn't convex, so it's filled as the three pieces that are).
fn arrow(p: &egui::Painter, pts: &[Pos2], fill: Color32, edge: Stroke) {
    if let [a, b, c, d, e, f, g] = *pts {
        for piece in [vec![a, b, c], vec![a, c, f, g], vec![c, d, e, f]] {
            p.add(egui::Shape::convex_polygon(piece, fill, Stroke::NONE));
        }
        if edge.width > 0.0 {
            p.add(egui::Shape::closed_line(pts.to_vec(), edge));
        }
    }
}

/// The step's caption with its picture playing: large while the step starts, then shrunk into the corner, still
/// playing. Returns whether the large one was clicked away, and whether the small one asked to be large again.
fn corner(ui: &mut Ui, t: &TutorialRun, vp: Rect, now: f64, moving: bool) -> (bool, bool) {
    let p = ui.painter().clone();
    let small = vp.height() < 560.0 || vp.width() < 720.0;
    let size = if small { vec2(168.0, 124.0) } else { vec2(244.0, 180.0) };
    let floor = crate::tablet::above_home_strip(ui.ctx(), vp.bottom());
    let corner = Rect::from_min_size(pos2(vp.left() + 10.0, floor - 10.0 - size.y), size);
    let big = Rect::from_center_size(vp.center(), vec2(560.0, 400.0).min(vp.size() - vec2(40.0, 60.0)));
    let f = match t.intro_until {
        Some(until) if now < until => 1.0,
        Some(until) if moving && now < until + SHRINK_S => {
            let s = ((now - until) / SHRINK_S) as f32;
            1.0 - s * s * (3.0 - 2.0 * s)
        }
        _ => 0.0,
    };
    if f > 0.0 {
        ui.ctx().request_repaint();
    }
    let card = Rect::from_min_max(corner.min.lerp(big.min, f), corner.max.lerp(big.max, f));
    if f > 0.0 {
        p.rect_filled(vp, 0.0, Color32::from_black_alpha((70.0 * f) as u8));
    }
    let resp = ui.interact(card, ui.scope_id().with("tut-card"), Sense::click());
    p.rect_filled(card.translate(vec2(0.0, 4.0)), radius(12), Color32::from_black_alpha(60));
    p.rect_filled(card, radius(12), theme::hex(0x111111));
    let pic = card.shrink2(vec2(8.0, 8.0));
    p.rect_filled(pic, radius(8), theme::hex(0xe9e8e8));
    if let Some(clip) = t.clip.as_ref()
        && let Some(last) = clip.frames.last()
    {
        // From nothing to done, a while on the result, over again; still on the result with motion off.
        let n = clip.frames.len();
        let tex = if !moving || n < 2 {
            last
        } else {
            let u = (now % (CLIP_PLAY_S + CLIP_HOLD_S)) / CLIP_PLAY_S;
            let i = if u >= 1.0 { n - 1 } else { ((u * n as f64) as usize).min(n - 1) };
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
            clip.frames.get(i).unwrap_or(last)
        };
        let size = tex.size_vec2();
        let scale = (pic.width() / size.x).min((pic.height() - 26.0 * (0.6 + f)) / size.y).max(0.01);
        let shown = Rect::from_center_size(pos2(pic.center().x, pic.bottom() - size.y * scale / 2.0), size * scale);
        p.image(tex.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    let caption = p.layout(
        crate::i18n::tr_s(t.step_def().intro),
        theme::strong(if small { 9.5 } else { 11.0 } + 9.0 * f),
        Color32::WHITE,
        card.width() - 40.0,
    );
    let cap = Rect::from_center_size(
        pos2(card.center().x, card.top() + 14.0 + 12.0 * f + caption.size().y / 2.0),
        caption.size() + vec2(18.0, 8.0),
    );
    p.rect_filled(cap, radius(7), Color32::from_black_alpha(170));
    p.galley(cap.min + vec2(9.0, 4.0), caption, Color32::WHITE);
    let skip = f >= 1.0 && (resp.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.any_click()));
    (skip, f == 0.0 && resp.clicked())
}

/// "Quit the tutorial?": Some(true) to quit, Some(false) to go on.
fn quit_card(ui: &mut Ui) -> Option<bool> {
    let full = ui.max_rect();
    let mut answer = None;
    egui::Area::new(ui.scope_id().with("tut-quit")).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
        let _ = ui.allocate_rect(full, Sense::click());
        let p = ui.painter();
        p.rect_filled(full, 0.0, Color32::from_black_alpha(90));
        let title = p.layout_no_wrap(crate::i18n::tr_s("Quit the tutorial?"), theme::strong(12.5), Tokens::text_strong());
        let card = Rect::from_center_size(full.center(), vec2((title.size().x + 40.0).max(240.0), 92.0));
        p.rect_filled(card.translate(vec2(0.0, 4.0)), radius(14), Color32::from_black_alpha(50));
        p.rect_filled(card, radius(14), Tokens::panel());
        p.galley(pos2(card.center().x - title.size().x / 2.0, card.top() + 16.0), title, Tokens::text_strong());
        let bw = (card.width() - 36.0) / 2.0;
        let b1 = Rect::from_min_size(pos2(card.left() + 14.0, card.bottom() - 42.0), vec2(bw, 28.0));
        let b2 = Rect::from_min_size(pos2(card.right() - 14.0 - bw, card.bottom() - 42.0), vec2(bw, 28.0));
        let r1 = ui.interact(b1, ui.scope_id().with("tut-quit-continue"), Sense::click());
        let r2 = ui.interact(b2, ui.scope_id().with("tut-quit-quit"), Sense::click());
        p.rect_filled(b1, radius(14), if r1.hovered() { Tokens::button_hover() } else { Tokens::button() });
        p.text(b1.center(), Align2::CENTER_CENTER, crate::i18n::tr("Continue"), theme::body(12.0), Tokens::text_strong());
        p.rect_filled(b2, radius(14), theme::hex(0xff3b30).gamma_multiply(if r2.hovered() { 0.45 } else { 0.3 }));
        p.text(b2.center(), Align2::CENTER_CENTER, crate::i18n::tr("Quit"), theme::body(12.0), theme::hex(0xff3b30));
        if r1.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            answer = Some(false);
        }
        if r2.clicked() {
            answer = Some(true);
        }
    });
    answer
}
