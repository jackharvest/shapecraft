//! Visualization state saved with the project: decals on bodies and the environment, light and camera settings.

use std::sync::Arc;

use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::{ItemId, Pixels};

/// A picture in the project's decal library (Visualization ▸ Material ▸ Decal), saved as images/<id>.<ext>.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecalImage {
    pub id: ItemId,
    pub name: String,
    pub ext: String,
    /// A custom material's picture (Material ▸ Custom) rather than a decal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub material: bool,
    /// A picture placed on a drawing sheet rather than a decal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sheet: bool,
    #[serde(skip)]
    pub file: Arc<Vec<u8>>,
    #[serde(skip)]
    pub pixels: Arc<Pixels>,
}

impl PartialEq for DecalImage {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.name == o.name && self.material == o.material && self.sheet == o.sheet && Arc::ptr_eq(&self.file, &o.file)
    }
}

impl DecalImage {
    /// One of the decal library's pictures (not a custom material's or a drawing's).
    pub fn is_decal(&self) -> bool {
        !self.material && !self.sheet
    }

    /// Width over height (1 for an empty picture).
    pub fn aspect(&self) -> f64 {
        let (w, h, _) = *self.pixels;
        if w == 0 || h == 0 { 1.0 } else { w as f64 / h as f64 }
    }
}

/// How a decal reaches the surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    /// Straight along the normal of the face it was dropped on.
    #[default]
    Planar,
    /// Wrapped around an axis (a cylinder's, or the decal's up direction).
    Cylindrical,
}

/// A picture projected onto a body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decal {
    pub id: ItemId,
    /// The library picture.
    pub image: ItemId,
    pub body: ItemId,
    /// The face it was dropped on; only that face shows it unless `extend` is set.
    pub face: u32,
    pub center: DVec3,
    /// Out of the surface at the centre.
    pub normal: DVec3,
    /// The picture's up direction, perpendicular to `normal`.
    pub up: DVec3,
    /// Width at 1.0x, in mm (the body's size when it was placed).
    pub size: f64,
    pub scale: f64,
    /// Degrees, counter-clockwise looking at the face.
    pub rotate: f64,
    pub opacity: f32,
    #[serde(default)]
    pub projection: Projection,
    /// Cylindrical: a point on the axis (the axis runs along `up`) and the radius.
    #[serde(default)]
    pub axis: Option<(DVec3, f64)>,
    #[serde(default)]
    pub extend: bool,
}

pub const DECAL_SCALE: (f64, f64) = (0.3, 10.0);

impl Decal {
    /// Finite values and a proper frame (hostile project files).
    pub fn valid(&self) -> bool {
        let finite = self.center.is_finite()
            && self.normal.is_finite()
            && self.up.is_finite()
            && self.size.is_finite()
            && self.scale.is_finite()
            && self.rotate.is_finite()
            && self.opacity.is_finite();
        finite
            && self.normal.length() > 1e-9
            && self.up.cross(self.normal).length() > 1e-9
            && self.size > 0.0
            && self.size < 1e7
            && self.axis.is_none_or(|(o, r)| o.is_finite() && r.is_finite() && r > 0.0)
    }

    /// The frame: right, up, out (unit, right-handed), before the rotation.
    pub fn frame(&self) -> (DVec3, DVec3, DVec3) {
        let n = self.normal.normalize_or(DVec3::Z);
        let up = (self.up - n * self.up.dot(n)).normalize_or(DVec3::Y);
        (up.cross(n), up, n)
    }

    /// Width and height in mm for a picture of this aspect.
    pub fn extent(&self, aspect: f64) -> (f64, f64) {
        let w = self.size * self.scale;
        (w, w / aspect.max(1e-6))
    }

    /// The axis a cylindrical projection wraps around: the round face's, else one behind the centre as far as half
    /// the picture's width.
    pub fn wrap_axis(&self, aspect: f64) -> (DVec3, f64) {
        let (_, _, out) = self.frame();
        let w = self.extent(aspect).0;
        self.axis.unwrap_or((self.center - out * (w / 2.0), w / 2.0))
    }

    /// Where a point of the body falls in the picture (0..1 from the top left), as the shader works it out; `None`
    /// off the picture or on a face it doesn't show on.
    pub fn uv_at(&self, p: DVec3, face: u32, aspect: f64) -> Option<(f64, f64)> {
        if !self.extend && face != self.face {
            return None;
        }
        let (right, up, out) = self.frame();
        let (w, h) = self.extent(aspect);
        let rel = p - self.center;
        let (mut s, t) = (rel.dot(right), rel.dot(up));
        let reach;
        if self.projection == Projection::Cylindrical {
            let (o, r) = self.wrap_axis(aspect);
            let q = p - o;
            let radial = q - up * q.dot(up);
            s = radial.dot(right).atan2(radial.dot(out)) * r;
            reach = (radial.length() - r).abs();
        } else {
            reach = rel.dot(out).abs();
        }
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let (rs, rt) = (cos * s + sin * t, -sin * s + cos * t);
        let uv = (rs / w + 0.5, 0.5 - rt / h);
        ((0.0..=1.0).contains(&uv.0) && (0.0..=1.0).contains(&uv.1) && reach <= w.max(h) * 0.5).then_some(uv)
    }
}

/// Where the floor that takes the shadow lies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ground {
    /// Under the lowest point of the visible bodies.
    #[default]
    Model,
    /// The XY plane.
    Xy,
    Off,
}

/// The Environment and Camera tabs of Visualization.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualSettings {
    /// Index into the environment list.
    pub environment: usize,
    /// Degrees around the vertical, −180..180.
    pub light_rotation: f64,
    /// Degrees above the floor, 5..90.
    pub sun_elevation: f64,
    /// 0.5..2.
    pub light_intensity: f64,
    pub ground: Ground,
    pub object_shadow: bool,
    /// Backdrop colours of the environments that let you pick one (Colored and Gradient Mood), sRGB 0..1.
    pub backgrounds: Vec<(usize, [f32; 3])>,
    /// ƒ-number, 1.4..32.
    pub aperture: f64,
    /// 0.5..2.
    pub blur: f64,
    /// Depth of field focuses here; none until a point is picked.
    pub focus: Option<DVec3>,
    /// The user's own panoramas, listed after the built-in environments: `environment` =
    /// [`BUILTIN_ENVIRONMENTS`] + k picks the k-th. Each file is saved as images/<id>.<ext>.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub panoramas: Vec<Panorama>,
}

impl Default for VisualSettings {
    fn default() -> Self {
        Self {
            environment: 0,
            light_rotation: 0.0,
            sun_elevation: 56.5,
            light_intensity: 1.0,
            ground: Ground::Model,
            object_shadow: true,
            backgrounds: Vec::new(),
            aperture: 4.0,
            blur: 1.0,
            focus: None,
            panoramas: Vec::new(),
        }
    }
}

pub const LIGHT_ROTATION: (f64, f64) = (-180.0, 180.0);
pub const SUN_ELEVATION: (f64, f64) = (5.0, 90.0);
pub const LIGHT_INTENSITY: (f64, f64) = (0.5, 2.0);
pub const APERTURE: (f64, f64) = (1.4, 32.0);
pub const BLUR: (f64, f64) = (0.5, 2.0);

impl VisualSettings {
    /// Every value clamped into its range (hostile project files and commands).
    pub fn sanitized(mut self) -> Self {
        let fix = |v: f64, (lo, hi): (f64, f64), d: f64| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let d = Self::default();
        self.panoramas.truncate(MAX_PANORAMAS);
        // A custom environment that isn't there (deleted, or its file lost) falls back to Default.
        if self.environment >= BUILTIN_ENVIRONMENTS + self.panoramas.len() {
            self.environment = 0;
        }
        self.light_rotation = fix(self.light_rotation, LIGHT_ROTATION, d.light_rotation);
        self.sun_elevation = fix(self.sun_elevation, SUN_ELEVATION, d.sun_elevation);
        self.light_intensity = fix(self.light_intensity, LIGHT_INTENSITY, d.light_intensity);
        self.aperture = fix(self.aperture, APERTURE, d.aperture);
        self.blur = fix(self.blur, BLUR, d.blur);
        self.focus = self.focus.filter(|f| f.is_finite());
        self.backgrounds.retain(|(_, c)| c.iter().all(|v| v.is_finite()));
        self.backgrounds.truncate(64);
        for (_, c) in &mut self.backgrounds {
            *c = c.map(|v| v.clamp(0.0, 1.0));
        }
        self
    }

    /// The direction the sun comes from (unit, z up): it casts the shadow. At 0° it stands over −X, so the shadow falls
    /// towards +X; turning it 90° puts it in front (−Y), as observed (s13-003, s13-021).
    pub fn light_dir(&self) -> DVec3 {
        let az = (180.0 + self.light_rotation).to_radians();
        let el = self.sun_elevation.to_radians();
        DVec3::new(el.cos() * az.cos(), el.cos() * az.sin(), el.sin())
    }

    /// The selected environment's panorama, when it's one of the user's.
    pub fn panorama(&self) -> Option<&Panorama> {
        self.environment.checked_sub(BUILTIN_ENVIRONMENTS).and_then(|k| self.panoramas.get(k))
    }

    pub fn background(&self, env: usize) -> Option<[f32; 3]> {
        self.backgrounds.iter().find(|(e, _)| *e == env).map(|(_, c)| *c)
    }
}

/// The built-in environments (Default, Transparent, the moods, stages, exhibition spaces, carousels); custom
/// panoramas follow them in the list.
pub const BUILTIN_ENVIRONMENTS: usize = 9;
/// Custom panoramas a project holds at most.
pub const MAX_PANORAMAS: usize = 32;
/// The largest panorama accepted, width × height (8K equirectangular).
pub const PANORAMA_MAX: (u32, u32) = (8192, 4096);
/// Panoramas are kept at most this size (width × height) once read: plenty for the backdrop and the reflections.
pub const PANORAMA_WORK: (u32, u32) = (2048, 1024);

/// A panorama's pixels: linear RGB, rows top (straight up) to bottom (straight down), columns once around.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HdrPixels {
    pub width: u32,
    pub height: u32,
    /// `width * height` linear RGB values, finite, 0..=[`HdrPixels::MAX`].
    pub rgb: Vec<[f32; 3]>,
    /// Read from a high dynamic range file (.hdr, .exr): its brightness is normalized for display.
    pub hdr: bool,
}

impl HdrPixels {
    /// The brightest value kept (half-float textures stop at 65504; a sun brighter than this changes nothing).
    pub const MAX: f32 = 60000.0;

    /// The average color, linear (black for an empty picture); from a sample of the pixels, cheap enough per frame.
    pub fn average(&self) -> [f32; 3] {
        let step = (self.rgb.len() / 20_000).max(1);
        let (mut sum, mut n) = ([0f64; 3], 0f64);
        for c in self.rgb.iter().step_by(step) {
            for k in 0..3 {
                sum[k] += c[k] as f64;
            }
            n += 1.0;
        }
        sum.map(|v| (v / n.max(1.0)) as f32)
    }

    /// What a high dynamic range panorama's values are multiplied by to show: its log-average brightness brought to
    /// a mid grey, so a photograph shot at any exposure looks right. 1 for PNG and JPEG (shown as they are).
    pub fn exposure(&self) -> f32 {
        if !self.hdr || self.rgb.is_empty() {
            return 1.0;
        }
        let step = (self.rgb.len() / 20_000).max(1);
        let (mut sum, mut n) = (0f64, 0f64);
        for c in self.rgb.iter().step_by(step) {
            let l = 0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64;
            sum += (l + 1e-4).ln();
            n += 1.0;
        }
        let avg = (sum / n.max(1.0)).exp();
        let e = (0.3 / avg) as f32;
        if e.is_finite() { e.clamp(1.0 / 1024.0, 1024.0) } else { 1.0 }
    }
}

/// An environment made from the user's own panorama (an equirectangular picture: .hdr, .exr, .png or .jpg).
#[derive(Clone, Serialize, Deserialize)]
pub struct Panorama {
    pub id: ItemId,
    pub name: String,
    /// "hdr", "exr", "png" or "jpg": the file kept with the project.
    pub ext: String,
    #[serde(skip)]
    pub file: Arc<Vec<u8>>,
    #[serde(skip)]
    pub pixels: Arc<HdrPixels>,
}

impl PartialEq for Panorama {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.name == o.name && self.ext == o.ext && (Arc::ptr_eq(&self.file, &o.file) || self.file == o.file)
    }
}

impl std::fmt::Debug for Panorama {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Panorama")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("ext", &self.ext)
            .field("bytes", &self.file.len())
            .field("size", &(self.pixels.width, self.pixels.height))
            .finish()
    }
}

impl Panorama {
    /// Reads a panorama file: Radiance .hdr, OpenEXR, PNG or JPEG. Pictures over [`PANORAMA_MAX`] are refused, and
    /// what's kept is scaled down to at most [`PANORAMA_WORK`]. Any aspect is taken (an equirectangular panorama is
    /// 2:1; other shapes are stretched around the sphere). NaN, infinite and negative values become 0, huge ones
    /// [`HdrPixels::MAX`]. Returns the file's extension and the pixels.
    pub fn decode(file: &[u8]) -> Result<(String, HdrPixels), String> {
        // A decoder bug on a hostile file must not take the app down.
        std::panic::catch_unwind(|| Self::decode_inner(file)).unwrap_or_else(|_| Err("that panorama file is damaged".into()))
    }

    fn decode_inner(file: &[u8]) -> Result<(String, HdrPixels), String> {
        use image::ImageFormat;
        let format = image::guess_format(file).map_err(|_| "not a panorama picture (use .hdr, .exr, .png or .jpg)".to_string())?;
        let (ext, hdr) = match format {
            ImageFormat::Hdr => ("hdr", true),
            ImageFormat::OpenExr => ("exr", true),
            ImageFormat::Png => ("png", false),
            ImageFormat::Jpeg => ("jpg", false),
            _ => return Err("only .hdr, .exr, .png and .jpg panoramas can be imported".into()),
        };
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(file), format);
        reader.limits(limits_for_decode());
        let (w, h) = reader.into_dimensions().map_err(|e| format!("couldn't read the panorama: {e}"))?;
        if w == 0 || h == 0 {
            return Err("the panorama is empty".into());
        }
        if w > PANORAMA_MAX.0 || h > PANORAMA_MAX.1 {
            return Err(format!("the panorama is {w}×{h}; {}×{} at most", PANORAMA_MAX.0, PANORAMA_MAX.1));
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(file), format);
        reader.limits(limits_for_decode());
        let img = reader.decode().map_err(|e| format!("couldn't read the panorama: {e}"))?;
        // Scaled to fit the working size, keeping the aspect.
        let scale = (PANORAMA_WORK.0 as f64 / w as f64).min(PANORAMA_WORK.1 as f64 / h as f64).min(1.0);
        let (tw, th) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));
        let img = if (tw, th) != (w, h) { img.resize_exact(tw, th, image::imageops::FilterType::Triangle) } else { img };
        let rgb = img.to_rgb32f();
        let (width, height) = rgb.dimensions();
        let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        let clean = |v: f32| if v.is_finite() { v.clamp(0.0, HdrPixels::MAX) } else { 0.0 };
        let rgb = rgb.as_raw().as_chunks::<3>().0.iter().map(|c| c.map(|v| if hdr { clean(v) } else { lin(clean(v).min(1.0)) })).collect();
        Ok((ext.into(), HdrPixels { width, height, rgb, hdr }))
    }
}

fn limits_for_decode() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(PANORAMA_MAX.0);
    limits.max_image_height = Some(PANORAMA_MAX.0);
    // An 8192×4096 float panorama is 400 MB decoded.
    limits.max_alloc = Some(1 << 30);
    limits
}
