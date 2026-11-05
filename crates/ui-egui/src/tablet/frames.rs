//! Paints the tablet copy's frames offscreen, reads them back, and turns what changed into pictures for the page.
//!
//! The screen is cut into 64 px tiles. Tiles that changed are grouped into bands (one span per tile row, up to four
//! rows tall) and encoded on every core at once: JPEG when much of the screen moves (turning the view), PNG when only
//! a little does (a hover, a menu). Bands sent as JPEG are sent again as PNG once the screen is still, so what the
//! eye rests on is always exact.

use std::sync::mpsc::channel;
use std::time::Duration;

use egui_wgpu::{RenderState, ScreenDescriptor};

const TILE: usize = 64;
/// Bands are at most this many tile rows tall.
const BAND_ROWS: usize = 4;
/// Below this share of the screen, changes go straight out as PNG.
const LOSSLESS_SHARE: f64 = 0.12;
const JPEG_QUALITY: u8 = 84;

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    size: [u32; 2],
    /// Bytes per row in the read-back buffer (rounded up as wgpu requires).
    row: u32,
}

pub(super) struct Painter {
    rs: RenderState,
    target: Option<Target>,
}

impl Painter {
    pub fn new(rs: RenderState) -> Self {
        Self { rs, target: None }
    }

    /// Texture changes of a frame that isn't painted (no page connected): kept, so the next painted frame has them.
    pub fn textures_only(&mut self, delta: &egui::TexturesDelta) {
        let mut r = self.rs.renderer.write();
        for (id, deltas) in &delta.set {
            for d in deltas {
                r.update_texture(&self.rs.device, &self.rs.queue, *id, d);
            }
        }
        for id in &delta.free {
            r.free_texture(id);
        }
    }

    /// Paints the frame at `size` pixels and returns its RGBA bytes, row by row with nothing between rows.
    pub fn paint(
        &mut self,
        prims: &[egui::ClippedPrimitive],
        delta: &egui::TexturesDelta,
        size: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<Vec<u8>, String> {
        let device = self.rs.device.clone();
        let queue = self.rs.queue.clone();
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            self.target = Some(self.make_target(size));
        }
        let Some(t) = self.target.as_ref() else { return Err("no target".into()) };
        let screen = ScreenDescriptor { size_in_pixels: size, pixels_per_point };
        let mut renderer = self.rs.renderer.write();
        for (id, deltas) in &delta.set {
            for d in deltas {
                renderer.update_texture(&device, &queue, *id, d);
            }
        }
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tablet frame") });
        let extra = renderer.update_buffers(&device, &queue, &mut enc, prims, &screen);
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("tablet frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &t.view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                        depth_slice: None,
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, prims, &screen);
        }
        enc.copy_texture_to_buffer(
            t.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &t.buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(t.row), rows_per_image: None },
            },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        let index = queue.submit(extra.into_iter().chain(std::iter::once(enc.finish())));
        for id in &delta.free {
            renderer.free_texture(id);
        }
        drop(renderer);
        let slice = t.buffer.slice(..);
        let (send, got) = channel();
        slice.map_async(wgpu::MapMode::Read, move |r| drop(send.send(r)));
        device
            .poll(wgpu::PollType::Wait { submission_index: Some(index), timeout: Some(Duration::from_secs(5)) })
            .map_err(|e| format!("the GPU didn't finish: {e}"))?;
        got.recv_timeout(Duration::from_secs(5)).map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let (w, h, row) = (size[0] as usize * 4, size[1] as usize, t.row as usize);
        let mut px = Vec::with_capacity(w * h);
        {
            let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
            for y in 0..h {
                px.extend_from_slice(data.get(y * row..y * row + w).ok_or("short read-back")?);
            }
        }
        t.buffer.unmap();
        if self.rs.target_format == wgpu::TextureFormat::Bgra8Unorm {
            for p in px.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
        }
        Ok(px)
    }

    fn make_target(&self, size: [u32; 2]) -> Target {
        let texture = self.rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tablet frame"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.rs.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let row = (size[0] * 4).div_ceil(align) * align;
        let buffer = self.rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tablet read-back"),
            size: u64::from(row) * u64::from(size[1]),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Target { texture, view, buffer, size, row }
    }
}

/// A picture of part of the screen, in pixels.
pub(super) struct Piece {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub png: bool,
    pub bytes: Vec<u8>,
}

/// What the page has: the last frame sent, and which tiles of it it only has as JPEG.
#[derive(Default)]
pub(super) struct Stream {
    prev: Vec<u8>,
    size: [usize; 2],
    lossy: Vec<bool>,
}

/// A run of tiles: columns c0..c1, rows r0..r1.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    c0: usize,
    c1: usize,
    r0: usize,
    r1: usize,
}

impl Stream {
    /// The page lost what it had (a new connection): the next frame goes whole.
    pub fn reset(&mut self) {
        self.prev.clear();
        self.lossy.fill(false);
    }

    pub fn has_lossy(&self) -> bool {
        self.lossy.iter().any(|l| *l)
    }

    fn grid(&self) -> (usize, usize) {
        (self.size[0].div_ceil(TILE), self.size[1].div_ceil(TILE))
    }

    /// The pieces to send for a new frame (none if nothing changed).
    pub fn update(&mut self, px: Vec<u8>, size: [u32; 2]) -> Vec<Piece> {
        let size = [size[0] as usize, size[1] as usize];
        if size != self.size || self.prev.len() != px.len() {
            self.size = size;
            self.prev.clear();
            let (cols, rows) = self.grid();
            self.lossy = vec![false; cols * rows];
        }
        let (cols, rows) = self.grid();
        let dirty: Vec<bool> = if self.prev.is_empty() { vec![true; cols * rows] } else { changed(&self.prev, &px, size) };
        let bands = bands(&dirty, cols, rows);
        let area: usize = bands.iter().map(|b| (b.c1 - b.c0) * (b.r1 - b.r0)).sum::<usize>() * TILE * TILE;
        let png = (area as f64) < LOSSLESS_SHARE * (size[0] * size[1]) as f64;
        for b in &bands {
            for r in b.r0..b.r1 {
                for c in b.c0..b.c1 {
                    self.lossy[r * cols + c] = !png;
                }
            }
        }
        self.prev = px;
        self.encode(&bands, png)
    }

    /// The JPEG parts of the screen again, as PNG.
    pub fn refine(&mut self) -> Vec<Piece> {
        let (cols, rows) = self.grid();
        let bands = bands(&self.lossy, cols, rows);
        self.lossy.fill(false);
        self.encode(&bands, true)
    }

    fn encode(&self, bands: &[Band], png: bool) -> Vec<Piece> {
        if bands.is_empty() {
            return Vec::new();
        }
        let rects: Vec<[usize; 4]> = bands
            .iter()
            .map(|b| {
                let (x, y) = (b.c0 * TILE, b.r0 * TILE);
                [x, y, (b.c1 * TILE).min(self.size[0]) - x, (b.r1 * TILE).min(self.size[1]) - y]
            })
            .collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 12);
        let per = rects.len().div_ceil(threads);
        let (px, width) = (&self.prev, self.size[0]);
        std::thread::scope(|s| {
            let jobs: Vec<_> = rects
                .chunks(per.max(1))
                .map(|chunk| s.spawn(move || chunk.iter().filter_map(|r| piece(px, width, *r, png)).collect::<Vec<_>>()))
                .collect();
            jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
        })
    }
}

/// Which tiles differ between two frames of the same size.
fn changed(a: &[u8], b: &[u8], size: [usize; 2]) -> Vec<bool> {
    let (cols, rows) = (size[0].div_ceil(TILE), size[1].div_ceil(TILE));
    let stride = size[0] * 4;
    let mut out = vec![false; cols * rows];
    for r in 0..rows {
        for y in r * TILE..((r + 1) * TILE).min(size[1]) {
            let line = y * stride;
            for c in 0..cols {
                if out[r * cols + c] {
                    continue;
                }
                let x0 = line + c * TILE * 4;
                let x1 = line + ((c + 1) * TILE).min(size[0]) * 4;
                if a[x0..x1] != b[x0..x1] {
                    out[r * cols + c] = true;
                }
            }
        }
    }
    out
}

/// Dirty tiles as bands: each row's first to last dirty column, with rows that share a span joined.
fn bands(dirty: &[bool], cols: usize, rows: usize) -> Vec<Band> {
    let mut out: Vec<Band> = Vec::new();
    for r in 0..rows {
        let row = &dirty[r * cols..(r + 1) * cols];
        let (Some(c0), Some(c1)) = (row.iter().position(|d| *d), row.iter().rposition(|d| *d)) else { continue };
        let span = (c0, c1 + 1);
        match out.last_mut() {
            Some(b) if b.r1 == r && (b.c0, b.c1) == span && b.r1 - b.r0 < BAND_ROWS => b.r1 = r + 1,
            _ => out.push(Band { c0: span.0, c1: span.1, r0: r, r1: r + 1 }),
        }
    }
    out
}

fn piece(px: &[u8], width: usize, [x, y, w, h]: [usize; 4], png: bool) -> Option<Piece> {
    let mut rgb = Vec::with_capacity(w * h * 3);
    for row in y..y + h {
        let start = (row * width + x) * 4;
        for p in px.get(start..start + w * 4)?.as_chunks::<4>().0 {
            rgb.extend_from_slice(&p[..3]);
        }
    }
    let mut bytes = Vec::new();
    if png {
        let mut enc = png::Encoder::new(&mut bytes, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgb).ok()?;
        writer.finish().ok()?;
    } else {
        let enc = jpeg_encoder::Encoder::new(&mut bytes, JPEG_QUALITY);
        enc.encode(&rgb, u16::try_from(w).ok()?, u16::try_from(h).ok()?, jpeg_encoder::ColorType::Rgb).ok()?;
    }
    Some(Piece { x: x as u32, y: y as u32, w: w as u32, h: h as u32, png, bytes })
}

/// One frame for the page: `[1][seq u32][count u16]`, then per piece `[x y w h u16][png u8][len u32][bytes]`, all
/// little-endian.
pub(super) fn message(seq: u32, pieces: &[Piece]) -> Vec<u8> {
    let mut out = Vec::with_capacity(7 + pieces.iter().map(|p| p.bytes.len() + 13).sum::<usize>());
    out.push(1);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(pieces.len().min(u16::MAX as usize) as u16).to_le_bytes());
    for p in pieces.iter().take(u16::MAX as usize) {
        for v in [p.x, p.y, p.w, p.h] {
            out.extend_from_slice(&(v as u16).to_le_bytes());
        }
        out.push(u8::from(p.png));
        out.extend_from_slice(&(p.bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&p.bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_with_the_same_span_join_up_to_four() {
        let cols = 3;
        let mut d = vec![false; cols * 6];
        for r in 0..6 {
            d[r * cols + 1] = true;
        }
        let b = bands(&d, cols, 6);
        assert_eq!(b, vec![Band { c0: 1, c1: 2, r0: 0, r1: 4 }, Band { c0: 1, c1: 2, r0: 4, r1: 6 }]);
    }

    #[test]
    fn a_small_change_goes_lossless_and_a_big_one_lossy_then_refined() {
        let size = [256u32, 256];
        let mut s = Stream::default();
        let mut px = vec![30u8; 256 * 256 * 4];
        let first = s.update(px.clone(), size);
        assert!(!first.is_empty() && first.iter().all(|p| !p.png), "the first frame is the whole screen, as JPEG");
        assert!(s.has_lossy());
        let refined = s.refine();
        assert!(refined.iter().all(|p| p.png) && !s.has_lossy());
        assert!(s.update(px.clone(), size).is_empty(), "nothing changed, nothing sent");
        px[(10 * 256 + 10) * 4] = 200;
        let small = s.update(px.clone(), size);
        assert_eq!(small.len(), 1);
        assert!(small[0].png && small[0].w == 64 && small[0].h == 64);
        let m = message(7, &small);
        assert_eq!(m[0], 1);
        assert_eq!(u32::from_le_bytes([m[1], m[2], m[3], m[4]]), 7);
    }

    #[test]
    fn edge_tiles_are_clipped_to_the_screen() {
        let size = [100u32, 70];
        let mut s = Stream::default();
        let p = s.update(vec![0u8; 100 * 70 * 4], size);
        let covered: u32 = p.iter().map(|p| p.w * p.h).sum();
        assert_eq!(covered, 100 * 70);
    }
}
