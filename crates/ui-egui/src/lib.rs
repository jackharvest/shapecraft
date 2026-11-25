//! ShapeCraft's desktop UI. The look and flow follow the reference app closely (see AGENTS.md, golden rule);
//! everything acts through the engine's commands.

#![forbid(unsafe_code)]

mod app;
pub mod app_log;
pub mod backup;
pub mod cloud;
pub mod control;
pub mod crash_log;
pub mod dialogs;
pub mod folder_access;
mod glide;
#[cfg(target_os = "linux")]
pub mod global_menu;
pub mod graphics;
pub mod i18n;
pub mod icons;
mod landing;
pub mod menubar;
pub mod motion;
pub(crate) mod onboarding;
pub(crate) mod plugins;
pub mod report;
pub mod saving;
pub mod settings;
pub mod setup;
pub mod system_theme;
pub mod tablet;
pub mod theme;
pub mod tutorials;
pub mod updates;
pub mod whats_new;
pub mod window_chrome;
pub mod workspace;

pub use app::{Options, ShapeCraftApp, TabletRequest, set_url_opener, write_color_image, write_rgba};
