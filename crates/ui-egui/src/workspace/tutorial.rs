//! A tutorial running in the workspace (see `crate::tutorials` for the lessons): as observed in the reference
//! (s30-020..095), no sidebars and only the tools the step needs; top left a ring that fills as you go with
//! "Step N out of M" beside it (the ring asks whether to quit); an instruction pill at the top that moves on as you
//! go; Retry on the left until the step's goal is met, then Continue and Redo. Each step opens on a large caption
//! (the reference plays a captioned video) that shrinks into a picture of where the step is headed, bottom left.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::Workspace;
use super::toolbar::{self, Action, ToolDef, ToolKind, UiAction};
use super::tools::Tool;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};
use crate::tutorials::{self, Lesson, Live, Snapshot};

/// What the tutorial asks the app to do (it builds each step's workspace).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Request {
    /// The same step again, from its own scene.
    Retry,
    Next,
    Quit,
}

pub struct TutorialRun {
    pub lesson: &'static Lesson,
    pub step: usize,
    /// The instruction showing (an index into the step's hints); all of them met means the step is done.
    pub hint: usize,
    pub base: Snapshot,
    now: Snapshot,
    seen_revision: u64,
    base_back: glam::DVec3,
    turned: f64,
    /// When the current instruction came up (a tip shows by the pointer after a while).
    hint_since: f64,
    /// The step's caption shows large until then (and grows back from the corner when clicked).
    intro_until: Option<f64>,
    started: bool,
    quit_ask: bool,
    pub picture: Option<egui::TextureHandle>,
    pub request: Option<Request>,
}

/// How long the caption shows large, and how long it takes to shrink into the corner.
const INTRO_S: f64 = 2.8;
const SHRINK_S: f64 = 0.35;

impl TutorialRun {
    pub fn new(lesson: &'static Lesson, step: usize, base: Snapshot) -> Self {
        Self {
            lesson,
            step,
            hint: 0,
            now: base.clone(),
            base,
            seen_revision: u64::MAX,
            base_back: glam::DVec3::Z,
            turned: 0.0,
            hint_since: 0.0,
            intro_until: None,
            started: false,
            quit_ask: false,
            picture: None,
            request: None,
        }
    }

    fn step_def(&self) -> &'static tutorials::Step {
        &self.lesson.steps[self.step.min(self.lesson.steps.len().saturating_sub(1))]
    }

    pub fn done(&self) -> bool {
        self.hint >= self.step_def().hints.len()
    }
}

fn def(tool: tutorials::Tool) -> ToolDef {
    let kind = match tool {
        tutorials::Tool::Sketch => ToolKind::Sketch,
        tutorials::Tool::Line => ToolKind::Line,
        tutorials::Tool::Rectangle => ToolKind::Rectangle,
        tutorials::Tool::Circle => ToolKind::Circle,
        tutorials::Tool::Arc => ToolKind::Arc,
        tutorials::Tool::Extrude => ToolKind::Extrude,
        tutorials::Tool::Shell => ToolKind::Shell,
        tutorials::Tool::Fillet => ToolKind::Fillet,
        tutorials::Tool::Visualization => {
            return ToolDef {
                icon: Icon::Visualize,
                label: "Visualization",
                sub: None,
                keys: "",
                action: Action::Ui(UiAction::Visualization),
                flyout: false,
            };
        }
    };
    toolbar::def_for(kind).unwrap_or(ToolDef {
        icon: Icon::Sketch,
        label: "Sketch",
        sub: None,
        keys: "",
        action: Action::Tool(kind),
        flyout: false,
    })
}

const fn plain(icon: Icon, label: &'static str) -> ToolDef {
    ToolDef { icon, label, sub: None, keys: "", action: Action::Ui(UiAction::Noop), flyout: false }
}

impl Workspace {
    /// Moves the tutorial on (instructions met, the step's goal), once a frame before drawing.
    pub(super) fn tutorial_tick(&mut self) {
        let live = {
            let Some(t) = self.tutorial.as_deref() else { return };
            let _ = t;
            let sketch = self.session.active_sketch();
            Live {
                picking_plane: matches!(self.tool, Tool::PickPlane { .. }),
                sketch_open: sketch.is_some(),
                sketch_on_face: sketch.is_some_and(|s| s.on_body.is_some()),
                selection: self.session.selection_kind(),
                turned: 0.0,
                visualizing: self.visualizing,
            }
        };
        let revision = self.session.revision;
        let back = self.camera.back();
        let now = self.now;
        let fresh = self.tutorial.as_deref().is_some_and(|t| t.seen_revision != revision);
        let snap = fresh.then(|| tutorials::snapshot(&self.session));
        let Some(t) = self.tutorial.as_deref_mut() else { return };
        if !t.started {
            t.started = true;
            t.base_back = back;
            t.hint_since = now;
            t.intro_until = Some(now + INTRO_S);
        }
        if let Some(s) = snap {
            t.now = s;
            t.seen_revision = revision;
        }
        t.turned = t.turned.max(back.dot(t.base_back).clamp(-1.0, 1.0).acos().to_degrees());
        let live = Live { turned: t.turned, ..live };
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
                }
                None => break,
            }
        }
    }

    /// The tutorial's controls, in place of the toolbar, the right column and the hint banner.
    pub(super) fn tutorial_controls(&mut self, ui: &mut Ui) {
        let Some(t) = self.tutorial.as_deref() else { return };
        let vp = self.viewport_rect;
        let now = self.now;
        let step = t.step_def();
        let steps = t.lesson.steps.len();
        let done = t.done();
        let hint = step.hints.get(t.hint);
        let p = ui.painter().clone();
        let x = vp.left() + 5.0;
        let top = vp.top() + 9.0;

        // Top left: the ring (how far through the lesson) and the step.
        let ring = Rect::from_min_size(pos2(x, top), vec2(Tokens::TOOL_BUTTON, Tokens::TOOL_BUTTON));
        let ring_resp = ui.interact(ring, ui.scope_id().with("tut-ring"), Sense::click());
        p.rect_filled(ring, radius(10), if ring_resp.hovered() { Tokens::button_hover() } else { Tokens::button() });
        p.rect_stroke(ring, radius(10), Stroke::new(1.0, Tokens::card_border()), StrokeKind::Inside);
        let progress = (t.step as f32 + if done { 1.0 } else { 0.4 }) / steps.max(1) as f32;
        let c = ring.center();
        p.circle_stroke(c, 9.0, Stroke::new(2.6, theme::c(0x4a4a4c)));
        let arc: Vec<Pos2> = (0..=40)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * progress.clamp(0.0, 1.0) * i as f32 / 40.0;
                c + vec2(a.cos(), a.sin()) * 9.0
            })
            .collect();
        p.add(egui::Shape::line(arc, Stroke::new(3.0, theme::hex(0x2e68ff))));
        let label = format!("Step {} out of {}", t.step + 1, steps);
        let galley = p.layout_no_wrap(label, theme::strong(12.0), Tokens::text_strong());
        let pill = Rect::from_min_size(pos2(ring.right() + 6.0, c.y - 15.0), vec2(galley.size().x + 20.0, 30.0));
        p.rect_filled(pill, radius(8), Tokens::button());
        p.rect_stroke(pill, radius(8), Stroke::new(1.0, Tokens::card_border()), StrokeKind::Inside);
        p.galley(pos2(pill.left() + 10.0, c.y - galley.size().y / 2.0), galley, Tokens::text_strong());

        // Top centre: what to do now.
        let text = match hint {
            Some(h) => h.text,
            None if t.step + 1 == steps => "All done. Select Continue to finish",
            None => "Nicely done. Select Continue",
        };
        let galley = p.layout_no_wrap(text.to_string(), theme::body(12.0), Tokens::text_strong());
        let pill = Rect::from_center_size(pos2(vp.center().x, vp.top() + 16.0), vec2(galley.size().x + 22.0, 22.0));
        // White in Light, as observed; a dark pill in Dark.
        p.rect_filled(pill.translate(vec2(0.0, 1.0)), radius(11), Color32::from_black_alpha(22));
        p.rect_filled(pill, radius(11), if theme::is_light() { Color32::WHITE } else { theme::hex(0x2b2a2a) });
        p.galley(pill.center() - galley.size() / 2.0, galley, Tokens::text_strong());

        // Left: the step's tools and Retry, or Continue and Redo once it's done; centred like the reference's.
        let sketching = self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. });
        let tools: Vec<ToolDef> =
            if done { Vec::new() } else { (if sketching { step.sketch_tools } else { step.tools }).iter().map(|k| def(*k)).collect() };
        let gap = toolbar::GROUP_GAP;
        let h_tools = if tools.is_empty() { 0.0 } else { toolbar::group_height(tools.len()) + gap };
        let h_end = if done { 2.0 * toolbar::group_height(1) + gap } else { toolbar::group_height(1) };
        let mut y = (vp.center().y - (h_tools + h_end) / 2.0).max(ring.bottom() + 20.0);
        let mut clicked: Option<ToolDef> = None;
        let mut request = None;
        if !tools.is_empty() {
            let active: Vec<bool> = tools.iter().map(|d| self.tutorial_tool_on(d)).collect();
            if let Some(i) = toolbar::group(ui, pos2(x, y), &tools, &active, true, "tut-tools") {
                clicked = tools.get(i).copied();
            }
            y += h_tools;
        }
        if done {
            let cont = plain(Icon::ChevronRight, "Continue");
            if toolbar::group(ui, pos2(x, y), &[cont], &[false], true, "tut-continue").is_some() {
                request = Some(Request::Next);
            }
            // The reference tints Continue green.
            let cell = Rect::from_min_size(pos2(x + 1.0, y + 1.0), vec2(Tokens::TOOL_BUTTON - 2.0, Tokens::TOOL_PITCH));
            p.rect_filled(cell.shrink(4.0), radius(8), theme::hex(0x34c759).gamma_multiply(0.22));
            icons::paint(&p, Rect::from_center_size(cell.center(), vec2(18.0, 18.0)), Icon::ChevronRight, theme::hex(0x2a9d48));
            y += toolbar::group_height(1) + gap;
            if toolbar::group(ui, pos2(x, y), &[plain(Icon::Retry, "Redo")], &[false], true, "tut-redo").is_some() {
                request = Some(Request::Retry);
            }
        } else if toolbar::group(ui, pos2(x, y), &[plain(Icon::Retry, "Retry")], &[false], true, "tut-retry").is_some() {
            request = Some(Request::Retry);
        }

        // A tip by the pointer when an instruction has waited a while.
        if let (Some(h), Some(cursor)) = (hint, self.cursor)
            && let Some(tip) = h.tip
            && now - t.hint_since > 6.0
            && vp.contains(cursor)
        {
            let galley = p.layout(tip.to_string(), theme::strong(11.5), Color32::WHITE, 220.0);
            let r = Rect::from_min_size(cursor + vec2(18.0, 18.0), galley.size() + vec2(20.0, 14.0));
            p.rect_filled(r, radius(8), theme::hex(0x34a853));
            p.galley(r.min + vec2(10.0, 7.0), galley, Color32::WHITE);
        }

        // The caption: large while the step starts, then shrunk into the corner with its picture.
        let corner = Rect::from_min_size(pos2(vp.left() + 10.0, vp.bottom() - 190.0), vec2(244.0, 180.0));
        let big = Rect::from_center_size(vp.center(), vec2(560.0, 400.0));
        let f = match t.intro_until {
            Some(until) if now < until => 1.0,
            Some(until) if now < until + SHRINK_S => {
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
        let card_resp = ui.interact(card, ui.scope_id().with("tut-card"), Sense::click());
        p.rect_filled(card.translate(vec2(0.0, 4.0)), radius(12), Color32::from_black_alpha(60));
        p.rect_filled(card, radius(12), theme::hex(0x111111));
        let pic = card.shrink2(vec2(8.0, 8.0));
        if let Some(tex) = &t.picture {
            let size = tex.size_vec2();
            let scale = (pic.width() / size.x).min((pic.height() - 26.0 * (0.6 + f)) / size.y).max(0.01);
            let shown = Rect::from_center_size(pos2(pic.center().x, pic.bottom() - size.y * scale / 2.0), size * scale);
            p.rect_filled(pic, radius(8), theme::hex(0xe9e8e8));
            p.image(tex.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        let caption = p.layout(step.intro.to_string(), theme::strong(11.0 + 9.0 * f), Color32::WHITE, card.width() - 40.0);
        let cap = Rect::from_center_size(
            pos2(card.center().x, card.top() + 14.0 + 12.0 * f + caption.size().y / 2.0),
            caption.size() + vec2(18.0, 8.0),
        );
        p.rect_filled(cap, radius(7), Color32::from_black_alpha(170));
        p.galley(cap.min + vec2(9.0, 4.0), caption, Color32::WHITE);
        let skip_intro = f >= 1.0 && (card_resp.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.any_click()));
        let replay = f == 0.0 && card_resp.clicked();

        // The ring asks before leaving (as observed: "Quit Tasks?" Continue / Quit).
        let mut quit_ask = t.quit_ask;
        if ring_resp.clicked() {
            quit_ask = true;
        }
        if quit_ask {
            let full = ui.max_rect();
            egui::Area::new(ui.scope_id().with("tut-quit")).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
                let _ = ui.allocate_rect(full, Sense::click());
                let p = ui.painter();
                p.rect_filled(full, 0.0, Color32::from_black_alpha(90));
                let card = Rect::from_center_size(full.center(), vec2(226.0, 84.0));
                p.rect_filled(card.translate(vec2(0.0, 4.0)), radius(14), Color32::from_black_alpha(50));
                p.rect_filled(card, radius(14), Tokens::panel());
                p.text(
                    pos2(card.center().x, card.top() + 24.0),
                    Align2::CENTER_CENTER,
                    "Quit the tutorial?",
                    theme::strong(12.5),
                    Tokens::text_strong(),
                );
                let b1 = Rect::from_min_size(pos2(card.left() + 14.0, card.bottom() - 38.0), vec2(95.0, 24.0));
                let b2 = Rect::from_min_size(pos2(card.right() - 109.0, card.bottom() - 38.0), vec2(95.0, 24.0));
                let r1 = ui.interact(b1, ui.scope_id().with("tut-quit-continue"), Sense::click());
                let r2 = ui.interact(b2, ui.scope_id().with("tut-quit-quit"), Sense::click());
                p.rect_filled(b1, radius(12), if r1.hovered() { Tokens::button_hover() } else { Tokens::button() });
                p.text(b1.center(), Align2::CENTER_CENTER, "Continue", theme::body(12.0), Tokens::text_strong());
                p.rect_filled(b2, radius(12), theme::hex(0xff3b30).gamma_multiply(if r2.hovered() { 0.45 } else { 0.3 }));
                p.text(b2.center(), Align2::CENTER_CENTER, "Quit", theme::body(12.0), theme::hex(0xff3b30));
                if r1.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    quit_ask = false;
                }
                if r2.clicked() {
                    quit_ask = false;
                    request = Some(Request::Quit);
                }
            });
        }

        if let Some(def) = clicked {
            self.act(def.action, def.label);
        }
        let Some(t) = self.tutorial.as_deref_mut() else { return };
        t.quit_ask = quit_ask;
        if skip_intro {
            t.intro_until = Some(now - 0.0001);
        } else if replay {
            t.intro_until = Some(now + INTRO_S);
        }
        if request.is_some() {
            t.request = request;
        }
    }

    /// A tutorial button shows lit while its tool runs.
    fn tutorial_tool_on(&self, d: &ToolDef) -> bool {
        match d.action {
            Action::Tool(ToolKind::Sketch) => self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. }),
            Action::Ui(UiAction::Visualization) => self.visualizing,
            _ => false,
        }
    }

    /// The step's goal, for the app (and the control channel).
    pub fn tutorial_state(&self) -> Option<serde_json::Value> {
        let t = self.tutorial.as_deref()?;
        let step = t.step_def();
        Some(serde_json::json!({
            "lesson": t.lesson.id,
            "step": t.step,
            "steps": t.lesson.steps.len(),
            "hint": step.hints.get(t.hint).map(|h| h.text),
            "done": t.done(),
            "goal": step.hints.last().map(|h| format!("{:?}", h.goal)),
        }))
    }
}
