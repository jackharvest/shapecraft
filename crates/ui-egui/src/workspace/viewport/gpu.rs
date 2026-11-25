//! The renderer and its textures (window GPU or headless), and screenshots and captures of the view.

use egui::TextureId;
use glam::DVec3;
use shapecraft_doc::SketchPlane;
use shapecraft_render::{Camera, Renderer, Rgba, SceneColors, SceneInput};

use crate::theme;
use crate::workspace::Workspace;

/// Renderer and texture bookkeeping. Uses the window's wgpu device when there is one; otherwise renders
/// headless and uploads pixels (offscreen snapshots).
#[derive(Default)]
pub struct ViewportGpu {
    renderer: Option<Renderer>,
    native: Option<(TextureId, u64)>,
    cpu: Option<egui::TextureHandle>,
    glass_native: Option<(TextureId, u64)>,
    glass_cpu: Option<egui::TextureHandle>,
    /// The blurred view for the frosted sidebars (set by `draw`).
    pub glass: Option<TextureId>,
    pub error: Option<String>,
    /// How much of the bodies the next frame draws.
    pub detail: shapecraft_render::Detail,
}

impl ViewportGpu {
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&egui_wgpu::RenderState>,
        scene: &SceneInput,
        camera: &Camera,
        px: (u32, u32),
    ) -> Option<TextureId> {
        if self.renderer.is_none() {
            self.renderer = match render_state {
                Some(rs) => Some(Renderer::new(rs.device.clone(), rs.queue.clone(), &rs.adapter)),
                None => match Renderer::headless() {
                    Ok(r) => Some(r),
                    Err(e) => {
                        self.error = Some(e.to_string());
                        None
                    }
                },
            };
        }
        let renderer = self.renderer.as_mut()?;
        renderer.detail = self.detail;
        let frame = match renderer.render(scene, camera, px) {
            Ok(f) => f,
            Err(e) => {
                self.error = Some(e.to_string());
                return None;
            }
        };
        if let Some(rs) = render_state {
            let generation = frame.generation;
            let mut egui_renderer = rs.renderer.write();
            self.glass = frame.glass.map(|(view, _)| match self.glass_native {
                Some((id, g)) if g == frame.glass_generation => id,
                Some((id, _)) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(&rs.device, view, wgpu::FilterMode::Linear, id);
                    self.glass_native = Some((id, frame.glass_generation));
                    id
                }
                None => {
                    let id = egui_renderer.register_native_texture(&rs.device, view, wgpu::FilterMode::Linear);
                    self.glass_native = Some((id, frame.glass_generation));
                    id
                }
            });
            match self.native {
                Some((id, g)) if g == generation => Some(id),
                Some((id, _)) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(&rs.device, frame.view, wgpu::FilterMode::Linear, id);
                    self.native = Some((id, generation));
                    Some(id)
                }
                None => {
                    let id = egui_renderer.register_native_texture(&rs.device, frame.view, wgpu::FilterMode::Linear);
                    self.native = Some((id, generation));
                    Some(id)
                }
            }
        } else {
            let (w, h, pixels) = match renderer.read_pixels() {
                Ok(p) => p,
                Err(e) => {
                    self.error = Some(e.to_string());
                    return None;
                }
            };
            self.glass = renderer.read_glass().ok().and_then(|(gw, gh, glass)| {
                let image = egui::ColorImage::from_rgba_unmultiplied([gw as usize, gh as usize], &glass);
                match &mut self.glass_cpu {
                    Some(t) => t.set(image, egui::TextureOptions::LINEAR),
                    None => self.glass_cpu = Some(ctx.load_texture("glass", image, egui::TextureOptions::LINEAR)),
                }
                self.glass_cpu.as_ref().map(|t| t.id())
            });
            let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
            match &mut self.cpu {
                Some(t) => t.set(image, egui::TextureOptions::LINEAR),
                None => self.cpu = Some(ctx.load_texture("viewport", image, egui::TextureOptions::LINEAR)),
            }
            self.cpu.as_ref().map(|t| t.id())
        }
    }

    /// What the last frame drew of the bodies, and whether coarser copies are still being made.
    pub fn drawn(&self) -> (shapecraft_render::Drawn, bool) {
        self.renderer.as_ref().map(|r| (r.drawn(), r.preparing())).unwrap_or_default()
    }

    /// Renders a thumbnail with the same renderer (RGBA8).
    pub fn thumbnail(&mut self, scene: &SceneInput, camera: &Camera, px: (u32, u32)) -> Option<(u32, u32, Vec<u8>)> {
        let mut own;
        let r = match self.renderer.as_mut() {
            Some(r) => r,
            None => {
                own = Renderer::headless().ok()?;
                &mut own
            }
        };
        // A picture always has all of it.
        r.detail = shapecraft_render::Detail { within_px: None, ..r.detail };
        r.render(scene, camera, px).ok()?;
        r.read_pixels().ok()
    }
}

impl Workspace {
    /// The view as it is (no selection, hover or tool overlays), rendered at twice the viewport's size and saved
    /// as a PNG: Screenshot in Modeling, Capture in Visualization.
    pub fn capture_png(&mut self, path: &std::path::Path) -> Result<(), String> {
        let (w, h, pixels) = self.capture_rgba()?;
        crate::app::write_rgba(path, w, h, &pixels)
    }

    /// Copy Screenshot to Clipboard (⇧⌘C): the same picture as Screenshot, on the clipboard.
    pub fn copy_screenshot(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let (w, h, pixels) = self.capture_rgba()?;
        ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels));
        Ok(())
    }

    /// What a plugin's button gets (Generative Render): the view as it looks, and where the model is (drawn over black
    /// and over white without its shadow, so the mask is the model alone with soft edges).
    pub fn plugin_capture(&mut self) -> Result<shapecraft_plugin_host::Capture, String> {
        use shapecraft_plugin_host::images::{Mask, Picture};
        let (w, h, view) = self.capture_scene(None, true, true)?;
        let (bw, bh, black) = self.capture_scene(Some(Rgba::hex(0x000000)), false, true)?;
        let (_, _, white) = self.capture_scene(Some(Rgba::hex(0xffffff)), false, true)?;
        if (bw, bh) != (w, h) {
            return Err("the view changed size while it was captured".into());
        }
        let alpha: Vec<u8> = unmix(&black, &white).as_chunks::<4>().0.iter().map(|px| px[3]).collect();
        Ok(shapecraft_plugin_host::Capture {
            view: Picture { width: w, height: h, pixels: view },
            model: Mask { width: w, height: h, values: alpha },
            selection: None,
        })
    }

    /// Visualization's Generative Render button: the plugin's sheet over a fresh capture of the view, or (without
    /// the plugin) a card offering to add it.
    pub(crate) fn generative_render(&mut self) {
        match crate::plugins::visualization_plugin() {
            Some((id, panel)) => {
                let capture = self.plugin_capture().map_err(|e| log::warn!("capture for {id}: {e}")).ok();
                crate::plugins::open_sheet(&id, &panel, capture);
            }
            None => crate::plugins::offer_generative(),
        }
    }

    fn capture_rgba(&mut self) -> Result<(u32, u32, Vec<u8>), String> {
        // The Transparent environment (or the Screenshot tool's Transparency) captures with a see-through backdrop:
        // drawn over black and over white, what differs between the two is how see-through each pixel is (soft
        // shadows stay half see-through).
        let see_through = if self.visualizing { self.session.doc.visual.environment == 1 } else { self.settings.shot_transparent };
        if see_through {
            let (w, h, black) = self.capture_over(Some(Rgba::hex(0x000000)), true)?;
            let (_, _, white) = self.capture_over(Some(Rgba::hex(0xffffff)), true)?;
            return Ok((w, h, unmix(&black, &white)));
        }
        self.capture_over(None, true)
    }

    fn capture_over(&mut self, backdrop: Option<Rgba>, shadow: bool) -> Result<(u32, u32, Vec<u8>), String> {
        self.capture_scene(backdrop, shadow, false)
    }

    /// `bare`: the bodies alone, no sketches (what a plugin sees: sketch lines would end up in a generated picture).
    fn capture_scene(&mut self, backdrop: Option<Rgba>, shadow: bool, bare: bool) -> Result<(u32, u32, Vec<u8>), String> {
        let rect = self.view_rect;
        // The Screenshot tool's size: the view's own pixels, twice that, or Full HD; Visualization's Capture is
        // the view's pixels.
        let ppp = if self.ppp > 0.0 { self.ppp } else { 2.0 };
        let modeling = !self.visualizing;
        let res = if modeling { self.variant("Resolution") } else { 0 };
        let scale = if res == 1 { ppp * 2.0 } else { ppp };
        let mut px =
            ((rect.width() * scale).round().clamp(16.0, 8192.0) as u32, (rect.height() * scale).round().clamp(16.0, 8192.0) as u32);
        // Full HD, 4K and 8K: that many pixels across, as tall as the view's shape gives.
        if let Some(w) = [None, None, Some(1920.0), Some(3840.0), Some(7680.0)].get(res).copied().flatten() {
            let h = (w * rect.height() / rect.width().max(1.0)).round().clamp(16.0, 8192.0);
            px = (w as u32, h as u32);
        }
        let grid = (self.view_opts.grid && !self.visualizing && self.settings.shot_grid).then(|| {
            [SketchPlane::XY, SketchPlane::YZ, SketchPlane { origin: DVec3::ZERO, x: DVec3::Z, y: DVec3::X }][self.grid_plane.min(2)]
        });
        let mut doc = self.session.doc.clone();
        if bare {
            for sk in &mut doc.sketches {
                sk.visible = false;
            }
        }
        let scene = SceneInput {
            doc: &doc,
            selection: &[],
            hover: None,
            active_sketch: if bare { None } else { self.session.active_sketch },
            plane_tiles: &[],
            overlays: &[],
            colors: SceneColors {
                xray: self.xray_opacity,
                ..if theme::is_light() { SceneColors::light() } else { SceneColors::default() }
            },
            show_axes: self.view_opts.axes && !self.visualizing,
            show_edges: self.view_opts.edges && (self.visualizing || self.settings.shot_edges),
            show_hidden_edges: self.view_opts.hidden_edges && (self.visualizing || self.settings.shot_edges),
            show_decals: self.view_opts.decals,
            grid,
            display: self.display,
            section: self.section.map(|s| (s.origin(), s.normal)),
            presentation: self.visualizing,
            backdrop: backdrop.unwrap_or_else(|| Rgba::hex(crate::workspace::visual::backdrop(&self.session.doc.visual))),
            shadow: if shadow { crate::workspace::visual::shadow(&self.session.doc.visual) } else { 0.0 },
            ghosts: &[],
            tints: &[],
            drawn: &[],
            defined: &[],
            boxing: false,
            bare: false,
        };
        let camera = self.camera;
        self.gpu.thumbnail(&scene, &camera, px).ok_or_else(|| "the view couldn't be rendered".to_string())
    }
}

/// The picture with a see-through backdrop, from the same view drawn over black and over white: alpha is one minus
/// how much the backdrop shows (white − black), the color what's left over black divided by that alpha.
pub(crate) fn unmix(black: &[u8], white: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(black.len());
    for (b, w) in black.as_chunks::<4>().0.iter().zip(white.as_chunks::<4>().0) {
        let show = (0..3).map(|k| w[k] as f32 - b[k] as f32).sum::<f32>() / 3.0 / 255.0;
        let alpha = (1.0 - show).clamp(0.0, 1.0);
        for &v in &b[..3] {
            let c = if alpha > 1e-3 { v as f32 / alpha } else { 0.0 };
            out.push(c.round().clamp(0.0, 255.0) as u8);
        }
        out.push((alpha * 255.0).round() as u8);
    }
    out
}
