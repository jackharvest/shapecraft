//! The 3D viewport. Nothing here depends on the UI toolkit: the app hands us a wgpu device and shows the
//! texture we draw into, and the CLI/MCP use the same renderer headless for PNGs and thumbnails.

#![forbid(unsafe_code)]

mod blur;
mod camera;
mod colors;
mod decal;
mod display_copy;
mod dof;
mod envmap;
mod gpu;
mod pick;
mod scene;

pub use camera::{Camera, StandardView};
pub use colors::{Rgba, SceneColors};
pub use gpu::{Frame, RenderError, Renderer};
pub use pick::{PickContext, pick, pick_through};
pub use scene::{DisplayMode, Overlay, PlaneTile, SceneInput, Segment, grid_step};
