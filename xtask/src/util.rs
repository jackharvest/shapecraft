//! What every job shares: where the repository and the build are, the version, and running tools.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The repository (this crate sits in `xtask/` at its root).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Cargo's target folder (CARGO_TARGET_DIR, else `target/` in the repository).
pub fn target() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root().join("target"), PathBuf::from)
}

/// The workspace version from the root Cargo.toml.
pub fn version() -> Result<String> {
    let toml = std::fs::read_to_string(root().join("Cargo.toml")).context("reading Cargo.toml")?;
    toml.lines()
        .find_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("version")?.trim_start().strip_prefix('=')?.trim();
            Some(rest.trim_matches('"').to_string())
        })
        .context("no version in Cargo.toml")
}

/// Runs a program, failing with its name when it fails or can't start.
pub fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status().with_context(|| format!("couldn't start {program}"))?;
    if !status.success() {
        bail!("{program} {} failed ({status})", args.join(" "));
    }
    Ok(())
}

/// Runs a program and returns what it printed.
pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program).args(args).output().with_context(|| format!("couldn't start {program}"))?;
    if !out.status.success() {
        bail!("{program} {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether a program can be started (found on PATH).
pub fn has(program: &str) -> bool {
    Command::new(program).arg("--version").output().is_ok()
}

/// A path as text for a tool's arguments.
pub fn s(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

/// Copies a file, making the folder it goes into.
pub fn copy(from: &Path, to: &Path) -> Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(from, to).with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
    Ok(())
}

/// Removes a folder if it's there.
pub fn clear(dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).with_context(|| format!("removing {}", dir.display())),
        _ => Ok(()),
    }
}

/// A size in megabytes for messages.
pub fn mb(p: &Path) -> String {
    std::fs::metadata(p).map_or_else(|_| "?".into(), |m| format!("{:.0} MB", m.len() as f64 / 1e6))
}
