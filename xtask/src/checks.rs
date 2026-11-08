//! Repository rules checked by code (run in CI):
//!
//!   cargo xtask assets   every image, font, picture or sound in git has its row in ATTRIBUTION.md
//!   cargo xtask layers   crates depend only on crates below them (kernel → doc → engine → UI / MCP → apps), the
//!                        UI toolkit stays in the UI and apps, the GPU in render and above, C++ only in kernel-occt

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::util::{output, root};

const ASSET_EXTENSIONS: [&str; 15] =
    ["png", "jpg", "jpeg", "gif", "mp4", "webm", "svg", "icns", "ico", "ttf", "otf", "wav", "hdr", "exr", "ogg"];

pub fn assets() -> Result<()> {
    let attribution = std::fs::read_to_string(root().join("ATTRIBUTION.md"))?;
    // The first column of each table row: `path` (a file, or a folder ending in /).
    let covered: Vec<String> =
        attribution.lines().filter(|l| l.starts_with("| `")).filter_map(|l| l.split('`').nth(1).map(str::to_string)).collect();
    let files = output("git", &["-C", crate::util::s(&root()), "ls-files"])?;
    let missing: Vec<&str> = files
        .lines()
        .filter(|f| !f.starts_with("plan/") && !f.starts_with("bench/"))
        .filter(|f| f.rsplit_once('.').is_some_and(|(_, ext)| ASSET_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())))
        .filter(|f| !covered.iter().any(|c| if c.ends_with('/') { f.starts_with(c.as_str()) } else { f == c }))
        .collect();
    if !missing.is_empty() {
        bail!("assets without an ATTRIBUTION.md row:\n  {}", missing.join("\n  "));
    }
    println!("assets: every asset has its ATTRIBUTION.md row");
    Ok(())
}

/// Each crate's layer: a crate may depend only on crates in lower layers.
const LAYERS: [(&str, u8); 15] = [
    ("shapecraft-kernel", 0),
    ("shapecraft-dwg", 0),
    ("shapecraft-update", 0),
    ("shapecraft-kernel-occt", 1),
    ("shapecraft-doc", 1),
    ("shapecraft-xt", 2),
    ("shapecraft-render", 2),
    ("shapecraft-shapr", 3),
    ("shapecraft-engine", 4),
    ("shapecraft-ui", 5),
    ("shapecraft-mcp", 5),
    ("shapecraft", 6),
    ("shapecraft-cli", 6),
    ("shapecraft-snap", 6),
    ("xtask", 7),
];

/// Outside dependencies only some layers may use: (crate, the lowest layer allowed).
const RESTRICTED: [(&str, u8); 6] = [("egui", 5), ("eframe", 5), ("egui-wgpu", 5), ("wgpu", 2), ("cc", 1), ("cxx", 1)];

/// A Cargo.toml's package name and the names in its [dependencies] (not dev- or build-only ones).
fn manifest(text: &str) -> (String, Vec<String>) {
    let mut name = String::new();
    let mut deps = Vec::new();
    let mut section = String::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_string();
            continue;
        }
        let Some((key, _)) = line.split_once('=') else { continue };
        let key = key.trim().trim_matches('"').to_string();
        if section == "package" && key == "name" {
            name = line.split('"').nth(1).unwrap_or_default().to_string();
        } else if section == "dependencies" && !key.is_empty() && !key.starts_with('#') {
            deps.push(key.split('.').next().unwrap_or_default().to_string());
        }
    }
    (name, deps)
}

pub fn layers() -> Result<()> {
    let layer: BTreeMap<&str, u8> = LAYERS.into_iter().collect();
    let mut problems = Vec::new();
    let mut manifests = Vec::new();
    for dir in ["crates", "apps"] {
        for entry in std::fs::read_dir(root().join(dir))?.flatten() {
            let p = entry.path().join("Cargo.toml");
            if p.exists() {
                manifests.push(p);
            }
        }
    }
    manifests.push(root().join("xtask/Cargo.toml"));
    for p in &manifests {
        let (name, deps) = manifest(&std::fs::read_to_string(p)?);
        let Some(&own) = layer.get(name.as_str()) else {
            problems.push(format!("{name}: not in xtask's layer list (xtask/src/checks.rs)"));
            continue;
        };
        for d in &deps {
            if let Some(&l) = layer.get(d.as_str())
                && l >= own
            {
                problems.push(format!("{name} (layer {own}) depends on {d} (layer {l})"));
            }
            if let Some((_, lowest)) = RESTRICTED.iter().find(|(r, _)| r == d)
                && own < *lowest
            {
                problems.push(format!("{name} (layer {own}) uses {d}, which belongs at layer {lowest} or above"));
            }
            if matches!(d.as_str(), "cc" | "cxx") && name != "shapecraft-kernel-occt" {
                problems.push(format!("{name} builds C++: only shapecraft-kernel-occt may"));
            }
        }
        let build_rs = p.with_file_name("build.rs");
        if build_rs.exists() && name != "shapecraft-kernel-occt" {
            problems.push(format!("{name} has a build script: only shapecraft-kernel-occt builds native code"));
        }
    }
    if !problems.is_empty() {
        bail!("layering:\n  {}", problems.join("\n  "));
    }
    println!("layers: {} crates, every dependency points down", manifests.len());
    Ok(())
}
