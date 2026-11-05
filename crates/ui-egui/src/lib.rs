//! ShapeCraft's desktop UI. The look and flow follow the reference app closely (see AGENTS.md, golden rule);
//! everything acts through the engine's commands.

#![forbid(unsafe_code)]

mod app;
pub mod control;
pub mod crash_log;
pub mod folder_access;
pub mod graphics;
pub mod i18n;
pub mod icons;
mod landing;
pub mod menubar;
pub(crate) mod onboarding;
pub mod settings;
pub mod system_theme;
pub mod tablet;
pub mod theme;
pub mod tutorials;
pub mod updates;
pub mod whats_new;
pub mod window_chrome;
pub mod workspace;

pub use app::{Options, ShapeCraftApp, write_color_image, write_rgba};
