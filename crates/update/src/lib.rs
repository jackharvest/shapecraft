//! ShapeCraft keeps itself up to date: it checks this project's GitHub releases, downloads the new version in the
//! background, checks it against the release's `SHA256SUMS.txt`, and installs it when ShapeCraft closes. Nobody has
//! to visit a download page. See docs/updates.md.
//!
//! Release files follow the Craft App Suite's naming (`shapecraft-<version>-<os>-<arch>[-portable].<ext>`, with
//! `SHA256SUMS.txt`), and a copy that a launcher manages (`SHAPECRAFT_MANAGED_BY`, or the ArtCraft Launcher's
//! `installed.json`) leaves installing to it, so ShapeCraft can join that launcher without changing how it updates.

#![forbid(unsafe_code)]

mod apply;
pub mod install;
pub mod net;
pub mod release;
mod stage;
pub mod updater;

pub use install::{Install, MANAGED_ENV};
pub use release::{Release, Version, parse_version, version_text};
pub use updater::{Phase, Prefs, Snapshot, Updater};

/// Releases only ever come from this repository.
pub const REPO: &str = "jackharvest/shapecraft";
/// The start of every release file's name.
pub const STEM: &str = "shapecraft";
pub const RELEASES_PAGE: &str = "https://github.com/jackharvest/shapecraft/releases";
const RELEASES_API: &str = "https://api.github.com/repos/jackharvest/shapecraft/releases?per_page=20";
const RELEASE_PAGE_PREFIX: &str = "https://github.com/jackharvest/shapecraft/releases/";
const DOWNLOAD_PREFIX: &str = "https://github.com/jackharvest/shapecraft/releases/download/";
/// The checksum list every release carries.
pub const SUMS: &str = "SHA256SUMS.txt";
/// Makes this copy report another version for one run, to try an update against a published release.
pub const PRETEND_ENV: &str = "SHAPECRAFT_PRETEND_VERSION";

/// The release workflow built this binary (it sets `SHAPECRAFT_RELEASE=1`). Anything else is a development build.
pub fn release_build() -> bool {
    option_env!("SHAPECRAFT_RELEASE") == Some("1") && !cfg!(debug_assertions)
}

/// The version this copy reports.
pub fn current() -> Version {
    std::env::var(PRETEND_ENV)
        .ok()
        .and_then(|v| parse_version(&v))
        .or_else(|| parse_version(env!("CARGO_PKG_VERSION")))
        .unwrap_or((0, 0, 0))
}

pub fn current_text() -> String {
    version_text(current())
}
