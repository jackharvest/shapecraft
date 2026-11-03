//! Visualization's custom environments: the user's panorama prefiltered for the material model. The panorama is
//! resampled to a 2:1 power-of-two equirectangular texture whose mip levels are blurred step by step (glossy
//! materials read a sharp level, rough ones a blurry one), plus a small cosine-convolved irradiance map for the
//! diffuse light. Both are Rgba16Float, filterable on every backend wgpu runs on (WebGPU core).
//!
//! Directions are z-up. A direction maps to `u = fract(0.75 - φ/2π)`, `v = θ/π` (φ the azimuth from +X, θ from +Z),
//! so the panorama's middle faces +Y (what the Front view looks at) and turning right moves along the picture to
//! the right, as standing inside it. The shader uses the same mapping (shader.wgsl `env_uv`).

use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use shapecraft_doc::HdrPixels;

/// The widest level kept on the GPU.
const MAX_WIDTH: u32 = 2048;
/// The irradiance map's size: diffuse light changes slowly.
const IRR: (u32, u32) = (32, 16);
/// The level the irradiance is integrated from.
const IRR_SOURCE_WIDTH: u32 = 64;

/// A panorama ready to upload: mip levels (largest first, each 2:1) and the irradiance map, linear RGB times the
/// exposure.
pub(crate) struct Prefiltered {
    pub levels: Vec<(u32, u32, Vec<[f32; 3]>)>,
    pub irradiance: (u32, u32, Vec<[f32; 3]>),
}

/// Direction for a texel centre of a `w`×`h` equirectangular map.
fn dir_of(x: u32, y: u32, w: u32, h: u32) -> [f32; 3] {
    let u = (x as f32 + 0.5) / w as f32;
    let v = (y as f32 + 0.5) / h as f32;
    let phi = TAU * (0.75 - u);
    let theta = PI * v;
    [theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()]
}

/// Bilinear sample of the source picture at (u, v) in 0..1, wrapping around in u.
fn sample(px: &HdrPixels, u: f32, v: f32) -> [f32; 3] {
    let (w, h) = (px.width as usize, px.height as usize);
    let fx = u * w as f32 - 0.5;
    let fy = (v * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let xi = |x: f32| (x as i64).rem_euclid(w as i64) as usize;
    let (xa, xb) = (xi(x0), xi(x0 + 1.0));
    let (ya, yb) = (y0 as usize, (y0 as usize + 1).min(h - 1));
    let at = |x: usize, y: usize| px.rgb.get(y * w + x).copied().unwrap_or([0.0; 3]);
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let top = at(xa, ya)[k] * (1.0 - tx) + at(xb, ya)[k] * tx;
        let bottom = at(xa, yb)[k] * (1.0 - tx) + at(xb, yb)[k] * tx;
        *o = top * (1.0 - ty) + bottom * ty;
    }
    out
}

/// Half the size: each texel the average of four, then a soft [1 2 1] blur (wrapping around, clamped at the poles)
/// so each level is a little rougher than plain averaging would make it.
fn next_level(w: u32, h: u32, src: &[[f32; 3]]) -> (u32, u32, Vec<[f32; 3]>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let at = |x: u32, y: u32| src.get((y.min(h - 1) * w + x.min(w - 1)) as usize).copied().unwrap_or([0.0; 3]);
    let mut half = vec![[0.0f32; 3]; (nw * nh) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let four = [at(2 * x, 2 * y), at(2 * x + 1, 2 * y), at(2 * x, 2 * y + 1), at(2 * x + 1, 2 * y + 1)];
            let o = &mut half[(y * nw + x) as usize];
            for c in four {
                for k in 0..3 {
                    o[k] += c[k] * 0.25;
                }
            }
        }
    }
    let blur = |img: &[[f32; 3]], horizontal: bool| -> Vec<[f32; 3]> {
        let mut out = vec![[0.0f32; 3]; img.len()];
        for y in 0..nh {
            for x in 0..nw {
                let (a, b) = if horizontal {
                    ((y * nw + (x + nw - 1) % nw) as usize, (y * nw + (x + 1) % nw) as usize)
                } else {
                    ((y.saturating_sub(1) * nw + x) as usize, ((y + 1).min(nh - 1) * nw + x) as usize)
                };
                let i = (y * nw + x) as usize;
                for k in 0..3 {
                    out[i][k] = 0.25 * img[a][k] + 0.5 * img[i][k] + 0.25 * img[b][k];
                }
            }
        }
        out
    };
    let blurred = blur(&blur(&half, true), false);
    (nw, nh, blurred)
}

/// Diffuse light arriving at each direction: ∫ L(ω) max(0, n·ω) dω / π over the sphere (a white panorama gives 1).
fn irradiance(w: u32, h: u32, src: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let dirs: Vec<([f32; 3], f32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let theta = PI * (y as f32 + 0.5) / h as f32;
            (dir_of(x, y, w, h), theta.sin() * (TAU / w as f32) * (PI / h as f32))
        })
        .collect();
    let mut out = Vec::with_capacity((IRR.0 * IRR.1) as usize);
    for y in 0..IRR.1 {
        for x in 0..IRR.0 {
            let n = dir_of(x, y, IRR.0, IRR.1);
            let mut e = [0.0f32; 3];
            for ((d, dw), c) in dirs.iter().zip(src) {
                let cos = n[0] * d[0] + n[1] * d[1] + n[2] * d[2];
                if cos > 0.0 {
                    for k in 0..3 {
                        e[k] += c[k] * cos * dw;
                    }
                }
            }
            out.push(e.map(|v| v / PI));
        }
    }
    out
}

/// Prefilters a panorama; `None` for an empty one.
pub(crate) fn prefilter(px: &HdrPixels) -> Option<Prefiltered> {
    if px.width == 0 || px.height == 0 || px.rgb.len() != (px.width as usize) * (px.height as usize) {
        return None;
    }
    // The power of two that holds all the picture's detail, 2:1, at most MAX_WIDTH wide.
    let w = px.width.max(px.height.saturating_mul(2)).clamp(8, MAX_WIDTH).next_power_of_two();
    let h = w / 2;
    let exposure = px.exposure();
    let mut base = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = sample(px, (x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            base.push(c.map(|v| (v * exposure).clamp(0.0, HdrPixels::MAX)));
        }
    }
    let mut levels = vec![(w, h, base)];
    while let Some((lw, lh, last)) = levels.last()
        && *lw > 4
    {
        let next = next_level(*lw, *lh, last);
        levels.push(next);
    }
    let source = levels.iter().find(|l| l.0 <= IRR_SOURCE_WIDTH).or(levels.last())?;
    let irr = irradiance(source.0, source.1, &source.2);
    Some(Prefiltered { levels, irradiance: (IRR.0, IRR.1, irr) })
}

/// Rounds to the nearest half float (clamped to the largest finite one).
fn f16_bits(v: f32) -> u16 {
    let sign = if v.is_sign_negative() { 0x8000u16 } else { 0 };
    let a = v.abs();
    if a.is_nan() {
        return 0;
    }
    if a >= 65504.0 {
        return sign | 0x7bff;
    }
    if a < 6.103_515_6e-5 {
        // Subnormal: multiples of 2^-24.
        return sign | (a / 5.960_464_5e-8).round() as u16;
    }
    let bits = a.to_bits();
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    let mut h = ((exp as u32) << 10) | (mant >> 13);
    if mant & 0x1000 != 0 {
        h += 1;
    }
    sign | (h.min(0x7bff) as u16)
}

fn rgba16(px: &[[f32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 8);
    for c in px {
        for v in [c[0], c[1], c[2], 1.0] {
            out.extend_from_slice(&f16_bits(v).to_le_bytes());
        }
    }
    out
}

/// The textures of the selected custom environment (or 1×1 stand-ins when there is none).
pub(crate) struct EnvGpu {
    /// The panorama's id and pixels they were made from.
    pub key: Option<(u64, usize)>,
    pub radiance: wgpu::TextureView,
    pub irradiance: wgpu::TextureView,
    /// log2 of the radiance map's width, and its highest mip level.
    pub log2_width: f32,
    pub max_lod: f32,
}

/// A texture with these mip levels (largest first); a 1×1 black one when there are none.
fn texture(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, levels: &[(u32, u32, &[[f32; 3]])]) -> wgpu::TextureView {
    let black: &[[f32; 3]] = &[[0.0; 3]];
    let levels = if levels.is_empty() { &[(1, 1, black)][..] } else { levels };
    let (w, h) = (levels[0].0, levels[0].1);
    let data: Vec<u8> = levels.iter().flat_map(|(_, _, px)| rgba16(px)).collect();
    let tex = wgpu::util::DeviceExt::create_texture_with_data(
        device,
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::MipMajor,
        &data,
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

impl EnvGpu {
    /// Black 1×1 maps, bound while no custom environment is selected.
    pub fn empty(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            key: None,
            radiance: texture(device, queue, "environment (none)", &[]),
            irradiance: texture(device, queue, "irradiance (none)", &[]),
            log2_width: 0.0,
            max_lod: 0.0,
        }
    }

    /// The prefiltered maps of a panorama (black ones when it's empty).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, id: u64, px: &Arc<HdrPixels>) -> Self {
        let key = Some((id, Arc::as_ptr(px) as usize));
        let Some(p) = prefilter(px) else { return Self { key, ..Self::empty(device, queue) } };
        let levels: Vec<(u32, u32, &[[f32; 3]])> = p.levels.iter().map(|(w, h, d)| (*w, *h, d.as_slice())).collect();
        let (iw, ih, irr) = &p.irradiance;
        let w = levels.first().map_or(1, |l| l.0);
        Self {
            key,
            radiance: texture(device, queue, "environment", &levels),
            irradiance: texture(device, queue, "irradiance", &[(*iw, *ih, irr.as_slice())]),
            log2_width: (w as f32).log2(),
            max_lod: levels.len().saturating_sub(1) as f32,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn pano(w: u32, h: u32, f: impl Fn(u32, u32) -> [f32; 3]) -> HdrPixels {
        let rgb = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| f(x, y)).collect();
        HdrPixels { width: w, height: h, rgb, hdr: false }
    }

    #[test]
    fn a_white_sky_gives_white_light_everywhere() {
        let p = prefilter(&pano(100, 50, |_, _| [1.0, 1.0, 1.0])).unwrap();
        assert_eq!((p.levels[0].0, p.levels[0].1), (128, 64), "a power of two holding all its detail, 2:1");
        assert_eq!(p.levels.last().unwrap().0, 4);
        for (w, h, px) in &p.levels {
            assert_eq!(px.len(), (w * h) as usize);
            assert!(px.iter().flatten().all(|v| (v - 1.0).abs() < 1e-4));
        }
        let (_, _, irr) = &p.irradiance;
        assert!(irr.iter().flatten().all(|v| (v - 1.0).abs() < 0.03), "a uniform sky lights every way alike");
    }

    #[test]
    fn light_from_above_lights_what_faces_up() {
        // Bright top half, black bottom.
        let p = prefilter(&pano(64, 32, |_, y| if y < 16 { [2.0, 2.0, 2.0] } else { [0.0; 3] })).unwrap();
        let (w, h, irr) = &p.irradiance;
        let up = irr[(w / 2) as usize][0];
        let down = irr[((h - 1) * w + w / 2) as usize][0];
        assert!(up > 1.8 && down < 0.2, "up {up}, down {down}");
        // The panorama's middle column faces +Y.
        let d = dir_of(w / 2, h / 2, *w, *h);
        assert!(d[1] > 0.95, "{d:?}");
    }

    #[test]
    fn hostile_pixels_stay_finite() {
        assert!(prefilter(&HdrPixels { width: 4, height: 2, rgb: vec![[1.0; 3]; 3], hdr: true }).is_none());
        assert!(prefilter(&HdrPixels::default()).is_none());
        let p = prefilter(&pano(3, 1, |x, _| [x as f32 * 1e4, 0.0, 1.0])).unwrap();
        assert!(p.levels.iter().all(|l| l.2.iter().flatten().all(|v| v.is_finite())));
    }

    /// Draws an empty Visualization with a sky-over-floor panorama and checks the backdrop shows it (skipped where
    /// there's no graphics adapter). This also compiles the shaders.
    #[test]
    fn the_backdrop_shows_the_panorama() {
        let Ok(mut r) = crate::Renderer::headless() else { return };
        let mut doc = shapecraft_doc::Document::new("t");
        let px = pano(64, 32, |_, y| if y < 16 { [0.1, 0.3, 0.9] } else { [0.6, 0.15, 0.05] });
        doc.visual.panoramas.push(shapecraft_doc::Panorama {
            id: shapecraft_doc::ItemId(7),
            name: "sky".into(),
            ext: "hdr".into(),
            file: Arc::new(Vec::new()),
            pixels: Arc::new(px),
        });
        doc.visual.environment = shapecraft_doc::BUILTIN_ENVIRONMENTS;
        let scene = crate::SceneInput {
            doc: &doc,
            selection: &[],
            hover: None,
            active_sketch: None,
            plane_tiles: &[],
            overlays: &[],
            colors: crate::SceneColors::default(),
            show_axes: false,
            show_edges: false,
            show_hidden_edges: false,
            show_decals: true,
            grid: None,
            display: Default::default(),
            section: None,
            presentation: true,
            backdrop: crate::Rgba::hex(0x101010),
            shadow: 0.0,
            ghosts: &[],
            tints: &[],
            drawn: &[],
            defined: &[],
        };
        let mut camera = crate::Camera::standard(crate::StandardView::Front, glam::DVec3::ZERO, 50.0);
        camera.ortho = 0.0;
        r.render(&scene, &camera, (64, 48)).unwrap();
        let (w, h, px) = r.read_pixels().unwrap();
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2]]
        };
        let (top, bottom) = (at(w / 2, 2), at(w / 2, h - 3));
        assert!(top[2] > 200 && top[0] < 120, "sky above: {top:?}");
        assert!(bottom[0] > 150 && bottom[2] < 90, "floor below: {bottom:?}");
    }

    #[test]
    fn half_floats_round_trip() {
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(f16_bits(1e9), 0x7bff);
        assert_eq!(f16_bits(f32::NAN), 0);
        assert_eq!(f16_bits(f32::INFINITY), 0x7bff);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(-2.0), 0xc000);
    }
}
