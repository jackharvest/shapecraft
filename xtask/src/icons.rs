//! The ShapeCraft app icon (K4, "step"): one flat net on a blue tile, written as SVG variants tuned per size and
//! rendered from those same SVGs to PNG (resvg), with the Windows .ico and, on macOS, the .icns.
//!
//!   cargo xtask icons OUT_DIR    → OUT_DIR/svg/*.svg, OUT_DIR/png/shapecraft-N.png + mac-N.png, OUT_DIR/shapecraft.ico
//!                                  (+ OUT_DIR/shapecraft.icns on macOS)
//!
//! The committed icons live in assets/icon; copy over what changed after checking it.

use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// A vertical three-stop tile, an inner rim glow, and glints at top-left and bottom-right.
const TILE_STOPS: [(f64, &str); 3] = [(0.0, "#3f7bff"), (0.5, "#0d5cff"), (1.0, "#004fff")];
const RIM_WIDTH: f64 = 2.6;
const RIM_BLUR: f64 = 0.9;
const RIM_OPACITY: (f64, f64) = (0.6, 0.18);
const RIM_CORNERS: f64 = 0.15;
/// (corner 0 = top-left, 1 = bottom-right; radius / tile; opacity)
const GLINTS: [(f64, f64, f64); 2] = [(0.0, 0.16, 0.8), (1.0, 0.14, 0.6)];
/// K4 faces: (kind, solid?, points). The kind picks the panel and stroke-tint gradients.
const FACES: [(&str, bool, &str); 5] = [
    ("t", false, "50.00,67.76 34.62,58.88 65.38,41.12 80.76,50.00"),
    ("r", true, "80.76,50.00 80.76,67.76 50.00,85.52 50.00,67.76"),
    ("l", false, "50.00,85.52 50.00,67.76 34.62,58.88 34.62,41.12 19.24,32.24 19.24,67.76"),
    ("t", false, "34.62,41.12 19.24,32.24 50.00,14.48 65.38,23.36"),
    ("r", true, "65.38,23.36 65.38,41.12 34.62,58.88 34.62,41.12"),
];
const PANEL: [(&str, &str, &str); 3] = [("t", "#ffffff", "#eef2fa"), ("l", "#f7f9fd", "#e2e8f5"), ("r", "#eef2fa", "#d3dcef")];
const TINT: [(&str, &str, &str); 3] = [("t", "#ffffff", "#f1f5fc"), ("l", "#eef3fc", "#dde6f7"), ("r", "#e2e9f8", "#cfdbf3")];

/// Per-size tuning: thin strokes and blur vanish at small sizes, so small variants go heavier and crisper.
struct Variant {
    name: &'static str,
    stroke: f64,
    soft: f64,
    hi: f64,
    tint: bool,
}

const VARIANTS: [Variant; 4] = [
    Variant { name: "large", stroke: 3.85, soft: 0.15, hi: 0.55, tint: true }, // 128 px and up (the master)
    Variant { name: "medium", stroke: 4.4, soft: 0.08, hi: 0.5, tint: true },  // 48–64 px
    Variant { name: "small", stroke: 5.4, soft: 0.0, hi: 0.0, tint: true },    // 32 px
    Variant { name: "tiny", stroke: 6.6, soft: 0.0, hi: 0.0, tint: false },    // 16 px
];

fn variant_for(size: u32) -> &'static Variant {
    let i = if size <= 16 {
        3
    } else if size <= 32 {
        2
    } else if size <= 64 {
        1
    } else {
        0
    };
    &VARIANTS[i]
}

/// A number written as Python writes a float (2.0, not 2), so the SVG text stays what it always was.
fn py(v: f64) -> String {
    let s = format!("{v}");
    if v.is_finite() && !s.contains('.') && !s.contains('e') { format!("{s}.0") } else { s }
}

/// Apple's continuous-corner tile: the superellipse |x|^n + |y|^n = 1 over the tile square (n = 5).
fn squircle(t: f64, w: f64) -> Vec<(f64, f64)> {
    let (n, steps) = (5.0f64, 192);
    let (c, h) = (t + w / 2.0, w / 2.0);
    (0..steps)
        .map(|i| {
            let a = 2.0 * PI * f64::from(i) / f64::from(steps);
            let (ca, sa) = (a.cos(), a.sin());
            (c + h * ca.abs().powf(2.0 / n).copysign(ca), c + h * sa.abs().powf(2.0 / n).copysign(sa))
        })
        .collect()
}

fn grad(id: &str, a: &str, b: &str) -> String {
    format!(
        r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="0" y1="12" x2="0" y2="88"><stop offset="0" stop-color="{a}"/><stop offset="1" stop-color="{b}"/></linearGradient>"#
    )
}

/// One SVG variant; `mac` puts it on Apple's icon grid (tile ~80% of the canvas, soft drop shadow).
fn svg(v: &Variant, mac: bool) -> String {
    let (t, k) = if mac { (9.8, 80.4 / 96.0) } else { (2.0, 1.0) };
    let w = 100.0 - 2.0 * t;
    let q = w / 96.0;
    let tile_d = format!("M{} Z", squircle(t, w).iter().map(|(x, y)| format!("{x:.3},{y:.3}")).collect::<Vec<_>>().join(" L"));
    let (tp, wp) = (py(t), py(w));
    let mut defs = vec![
        format!(
            r#"<linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">{}</linearGradient>"#,
            TILE_STOPS.iter().map(|(o, c)| format!(r#"<stop offset="{}" stop-color="{c}"/>"#, py(*o))).collect::<String>()
        ),
        format!(r#"<clipPath id="tclip"><path d="{tile_d}"/></clipPath>"#),
        format!(
            r#"<filter id="rimblur" x="-10%" y="-10%" width="120%" height="120%"><feGaussianBlur stdDeviation="{}"/></filter>"#,
            py(RIM_BLUR * q)
        ),
        format!(
            r##"<linearGradient id="rim" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="{}"/><stop offset="1" stop-color="#ffffff" stop-opacity="{}"/></linearGradient>"##,
            py(RIM_OPACITY.0),
            py(RIM_OPACITY.1)
        ),
        format!(
            r##"<linearGradient id="across" x1="1" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="{c}"/><stop offset="0.5" stop-color="#ffffff"/><stop offset="1" stop-color="#ffffff" stop-opacity="{c}"/></linearGradient>"##,
            c = py(RIM_CORNERS)
        ),
        format!(
            r#"<mask id="rimfade" maskContentUnits="userSpaceOnUse"><rect x="{tp}" y="{tp}" width="{wp}" height="{wp}" fill="url(#across)"/></mask>"#
        ),
    ];
    for (i, (corner, rad, op)) in GLINTS.iter().enumerate() {
        let c = py(t + corner * w);
        defs.push(format!(
            r##"<radialGradient id="glint{i}" gradientUnits="userSpaceOnUse" cx="{c}" cy="{c}" r="{}"><stop offset="0" stop-color="#ffffff" stop-opacity="{}"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/></radialGradient>"##,
            py(rad * w),
            py(*op)
        ));
    }
    defs.extend(PANEL.iter().map(|(n, a, b)| grad(&format!("p{n}"), a, b)));
    defs.extend(TINT.iter().map(|(n, a, b)| grad(&format!("i{n}"), a, b)));
    defs.extend(FACES.iter().enumerate().map(|(i, (_, _, p))| format!(r#"<clipPath id="c{i}"><polygon points="{p}"/></clipPath>"#)));
    if v.soft != 0.0 {
        defs.push(format!(
            r#"<filter id="soft" x="-10%" y="-10%" width="120%" height="120%"><feGaussianBlur stdDeviation="{}"/></filter>"#,
            py(v.soft)
        ));
    }
    if mac {
        defs.push(
            r##"<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="0.9" stdDeviation="1.1" flood-color="#001a66" flood-opacity="0.32"/></filter>"##
                .into(),
        );
    }
    let mut body = vec![
        format!(r#"<path d="{tile_d}" fill="url(#tile)"{}/>"#, if mac { r#" filter="url(#shadow)""# } else { "" }),
        format!(
            r#"<g clip-path="url(#tclip)"><path d="{tile_d}" fill="none" stroke="url(#rim)" mask="url(#rimfade)" stroke-width="{}" filter="url(#rimblur)"/>{}</g>"#,
            py(2.0 * RIM_WIDTH * q),
            (0..GLINTS.len())
                .map(|i| format!(r#"<rect x="{tp}" y="{tp}" width="{wp}" height="{wp}" fill="url(#glint{i})"/>"#))
                .collect::<String>()
        ),
    ];
    let stroke = py(v.stroke);
    let mut g: Vec<String> =
        FACES.iter().filter(|f| f.1).map(|(kind, _, p)| format!(r#"<polygon points="{p}" fill="url(#p{kind})"/>"#)).collect();
    g.extend(FACES.iter().map(|(_, _, p)| {
        format!(r##"<polygon points="{p}" fill="none" stroke="#ffffff" stroke-width="{stroke}" stroke-linejoin="round"/>"##)
    }));
    if v.tint {
        g.extend(FACES.iter().enumerate().map(|(i, (kind, _, p))| {
            format!(r#"<polygon points="{p}" fill="none" stroke="url(#i{kind})" stroke-width="{stroke}" stroke-linejoin="round" clip-path="url(#c{i})"/>"#)
        }));
    }
    if v.hi != 0.0 {
        let hi = py(v.hi);
        g.extend(FACES.iter().map(|(_, _, p)| {
            format!(r##"<polygon points="{p}" fill="none" stroke="#ffffff" stroke-width="{hi}" stroke-linejoin="round"/>"##)
        }));
    }
    let tf = if k != 1.0 { format!(r#" transform="translate(50 50) scale({k:.4}) translate(-50 -50)""#) } else { String::new() };
    let flt = if v.soft != 0.0 { r#" filter="url(#soft)""# } else { "" };
    body.push(format!("<g{tf}><g{flt}>{}</g></g>", g.concat()));
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="1024" height="1024"><title>ShapeCraft</title><defs>{}</defs>{}</svg>
"#,
        defs.concat(),
        body.concat()
    )
}

/// Renders an SVG to an RGBA PNG of `size` × `size`.
fn png(svg_text: &str, size: u32) -> Result<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_str(svg_text, &resvg::usvg::Options::default()).context("reading the SVG")?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).context("an empty image")?;
    let scale = size as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied colour; PNG wants it straight.
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&rgba)?;
    }
    Ok(out)
}

/// An .ico holding PNG images (Windows reads PNG entries from Vista on).
fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(0u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend((images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in images {
        let b = if *size >= 256 { 0 } else { *size as u8 };
        out.extend([b, b, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in images {
        out.extend(data);
    }
    out
}

pub fn write(args: &[String]) -> Result<()> {
    let Some(out) = args.first() else { bail!("usage: cargo xtask icons OUT_DIR") };
    let out = Path::new(out);
    std::fs::create_dir_all(out.join("svg"))?;
    std::fs::create_dir_all(out.join("png"))?;
    for v in &VARIANTS {
        std::fs::write(out.join(format!("svg/shapecraft-{}.svg", v.name)), svg(v, false))?;
        std::fs::write(out.join(format!("svg/shapecraft-mac-{}.svg", v.name)), svg(v, true))?;
    }
    let mut for_ico = Vec::new();
    let mut log = String::new();
    for size in [16, 32, 48, 64, 128, 256, 512, 1024] {
        let v = variant_for(size);
        let plain = png(&svg(v, false), size)?;
        std::fs::write(out.join(format!("png/shapecraft-{size}.png")), &plain)?;
        std::fs::write(out.join(format!("png/mac-{size}.png")), png(&svg(v, true), size)?)?;
        if size <= 256 {
            for_ico.push((size, plain));
        }
        let _ = write!(log, "{size} ");
    }
    std::fs::write(out.join("shapecraft.ico"), ico(&for_ico))?;
    if cfg!(target_os = "macos") {
        // iconutil wants icon_NxN[@2x].png in a .iconset folder.
        let set = out.join("ShapeCraft.iconset");
        std::fs::create_dir_all(&set)?;
        for (name, size) in [
            ("16x16", 16),
            ("16x16@2x", 32),
            ("32x32", 32),
            ("32x32@2x", 64),
            ("128x128", 128),
            ("128x128@2x", 256),
            ("256x256", 256),
            ("256x256@2x", 512),
            ("512x512", 512),
            ("512x512@2x", 1024),
        ] {
            std::fs::copy(out.join(format!("png/mac-{size}.png")), set.join(format!("icon_{name}.png")))?;
        }
        crate::util::run("iconutil", &["-c", "icns", crate::util::s(&set), "-o", crate::util::s(&out.join("shapecraft.icns"))])?;
    }
    println!("{}: svg/, png/ ({log}px), shapecraft.ico{}", out.display(), if cfg!(target_os = "macos") { ", shapecraft.icns" } else { "" });
    Ok(())
}
