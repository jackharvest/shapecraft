//! How much of the bodies to draw while the view moves (#25). The renderer keeps coarser copies of heavy bodies; this
//! decides when they're worth showing: never while the view is still, and while it moves only as coarse as this
//! computer needs to keep up with this model.

use shapecraft_render::{Camera, Detail};

/// Preferences ▸ Coarser While Moving.
pub const CHOICES: [&str; 3] = ["Automatic", "Always", "Never"];

/// The view counts as moving until it has stood still this long, so a slow drag doesn't flicker.
const SETTLE_S: f64 = 0.2;
/// How far a copy may stray from its body at each step, in pixels: the first can't be told from the full mesh.
const STEPS_PX: [Option<f32>; 4] = [None, Some(1.0), Some(2.5), Some(6.0)];
/// From this many triangles on the copies are made ahead, and from this many the first step is always taken.
const READY: usize = 150_000;
const HEAVY: usize = 600_000;
/// A frame slower than this is one to do something about (36 a second).
const SLOW_S: f32 = 0.028;
/// Frames looked at before judging.
const LOOK: u32 = 10;

#[derive(Default)]
pub struct Moving {
    camera: Option<Camera>,
    moved_at: f64,
    was_moving: bool,
    /// The last [`LOOK`] frames' average, in the making.
    frames: u32,
    total: f32,
    step: usize,
    /// How long frames took before the step was taken, while it's on trial.
    trial: Option<f32>,
    /// Coarser didn't help: this is as far as it goes for this model.
    top: Option<usize>,
    triangles: usize,
}

impl Moving {
    /// A frame: where the camera is, what there is to draw, and how long the last frame took. Says how much to draw, and
    /// whether another frame is needed to go back to the full meshes.
    pub fn frame(&mut self, choice: &str, camera: &Camera, triangles: usize, now: f64, took: f32) -> (Detail, bool) {
        if self.camera.as_ref().is_some_and(|c| c != camera) {
            self.moved_at = now;
        }
        let first = self.camera.replace(*camera).is_none();
        let moving = !first && self.moved_at > 0.0 && now - self.moved_at < SETTLE_S;
        // Another model: what was learned about the last one says nothing.
        if triangles > self.triangles * 2 || triangles * 2 < self.triangles {
            *self = Self { camera: self.camera, moved_at: self.moved_at, was_moving: self.was_moving, triangles, ..Self::default() };
        }
        let automatic = choice == CHOICES[0];
        // Only between two frames drawn one right after the other does the time say anything.
        if automatic && moving && self.was_moving && took > 0.0 && took < 3.0 {
            self.pace(took);
        }
        self.was_moving = moving;
        let step = match choice {
            "Never" => 0,
            "Always" => 2,
            _ => self.step.max(usize::from(triangles >= HEAVY)),
        };
        let prepare = step > 0 || (automatic && triangles >= READY);
        (Detail { prepare, within_px: STEPS_PX.get(step).copied().flatten().filter(|_| moving) }, moving && prepare)
    }

    fn pace(&mut self, took: f32) {
        self.frames += 1;
        self.total += took;
        if self.frames < LOOK {
            return;
        }
        let pace = self.total / self.frames as f32;
        (self.frames, self.total) = (0, 0.0);
        if let Some(before) = self.trial.take()
            && pace > before * 0.9
        {
            // Not what holds it up (a slow screen, a slow window system): coarser would only look worse.
            self.step = self.step.saturating_sub(1);
            self.top = Some(self.step);
            return;
        }
        if pace > SLOW_S && self.step + 1 < STEPS_PX.len() && self.top.is_none_or(|t| self.step < t) {
            self.trial = Some(pace);
            self.step += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` frames of a turning view, each taking what `took` says for the pixels allowed. The last frame's answer.
    fn turn(m: &mut Moving, choice: &str, triangles: usize, n: usize, took: impl Fn(Option<f32>) -> f32) -> Detail {
        let mut d = Detail::default();
        let mut camera = Camera::default();
        let mut now = 1.0;
        for _ in 0..n {
            camera.yaw += 0.01;
            let t = took(d.within_px);
            now += f64::from(t);
            d = m.frame(choice, &camera, triangles, now, t).0;
        }
        d
    }

    #[test]
    fn a_fast_computer_always_draws_everything() {
        let d = turn(&mut Moving::default(), "Automatic", 100_000, 200, |_| 0.008);
        assert_eq!(d, Detail::default());
    }

    #[test]
    fn a_slow_one_goes_coarser_until_it_keeps_up() {
        // Twice as fast with every step: 80 ms, 40, 20.
        let took = |px: Option<f32>| match px {
            None => 0.080,
            Some(p) if p < 2.0 => 0.040,
            _ => 0.020,
        };
        let mut m = Moving::default();
        let d = turn(&mut m, "Automatic", 100_000, 200, took);
        assert_eq!(d.within_px, Some(2.5));
        assert!(d.prepare);
        assert_eq!(turn(&mut m, "Never", 100_000, 5, took), Detail::default());
    }

    #[test]
    fn coarser_that_doesnt_help_is_dropped() {
        let d = turn(&mut Moving::default(), "Automatic", 100_000, 400, |_| 0.060);
        assert_eq!(d.within_px, None);
    }

    #[test]
    fn the_full_meshes_come_back_when_the_view_stops() {
        let mut m = Moving::default();
        let d = turn(&mut m, "Always", 100_000, 20, |_| 0.010);
        assert_eq!(d.within_px, Some(2.5));
        let still = m.camera.unwrap();
        // Held a moment, then back; and it asks for that frame itself.
        let (held, again) = m.frame("Always", &still, 100_000, m.moved_at + 0.1, 0.1);
        assert_eq!((held.within_px, again), (Some(2.5), true));
        let (back, again) = m.frame("Always", &still, 100_000, m.moved_at + 0.25, 0.1);
        assert_eq!((back.within_px, again), (None, false));
    }

    #[test]
    fn a_huge_model_takes_the_first_step_anyway() {
        let d = turn(&mut Moving::default(), "Automatic", 2_000_000, 20, |_| 0.008);
        assert_eq!(d.within_px, Some(1.0));
    }
}
