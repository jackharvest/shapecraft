//! Decals: each library picture becomes a texture once; each decal keeps its frame in a small uniform and is drawn
//! over its body's triangles, the shader cutting out everything outside the picture.

use std::collections::HashMap;
use std::sync::Arc;

use shapecraft_doc::{Decal, Document, Projection};

use crate::gpu::{COLOR_FORMAT, DEPTH_FORMAT, SAMPLES};

struct Texture {
    pixels: Arc<(u32, u32, Vec<u8>)>,
    view: wgpu::TextureView,
    used: bool,
}

struct Slot {
    image: u64,
    params: wgpu::Buffer,
    bind: wgpu::BindGroup,
    used: bool,
}

pub(crate) struct Decals {
    pub pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<u64, Texture>,
    slots: HashMap<u64, Slot>,
}

impl Decals {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, mesh: wgpu::VertexBufferLayout) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("decal shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("decal.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("decal"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("decal"),
            bind_group_layouts: &[Some(globals), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("decals"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_decal"),
                compilation_options: Default::default(),
                buffers: &[Some(mesh)],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_decal"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("decal"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self { pipeline, layout, sampler, textures: HashMap::new(), slots: HashMap::new() }
    }

    /// Makes textures for new pictures and writes every decal's frame. Returns (slot, body id) to draw: custom
    /// materials first (slot = body id | MATERIAL), then decals (slot = decal id) on top of them.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, doc: &Document) -> Vec<(u64, u64)> {
        for t in self.textures.values_mut() {
            t.used = false;
        }
        for s in self.slots.values_mut() {
            s.used = false;
        }
        let mut draws = Vec::new();
        for b in &doc.bodies {
            let Some(look) = b.appearance.as_ref().filter(|_| b.visible) else { continue };
            let Some(image) = look.texture else { continue };
            let Some(img) = doc.decal_images.iter().find(|i| i.id == image && i.material) else { continue };
            let Some(bounds) = mesh_bounds(&b.mesh.positions) else { continue };
            let slot = b.id.0 | MATERIAL;
            if !self.ensure(device, queue, slot, img) {
                continue;
            }
            let Some(s) = self.slots.get(&slot) else { continue };
            queue.write_buffer(&s.params, 0, bytemuck::cast_slice(&material_uniform(bounds, look.scale, img.aspect())));
            draws.push((slot, b.id.0));
        }
        for d in &doc.decals {
            let Some(img) = doc.decal_images.iter().find(|i| i.id == d.image) else { continue };
            if !d.valid() || !self.ensure(device, queue, d.id.0, img) {
                continue;
            }
            let Some(s) = self.slots.get(&d.id.0) else { continue };
            queue.write_buffer(&s.params, 0, bytemuck::cast_slice(&uniform(d, img.aspect())));
            draws.push((d.id.0, d.body.0));
        }
        self.textures.retain(|_, t| t.used);
        self.slots.retain(|_, s| s.used);
        draws
    }

    /// Makes sure the picture has a texture and the slot a bind group showing it (both marked used).
    fn ensure(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, slot: u64, img: &shapecraft_doc::DecalImage) -> bool {
        let (w, h, ref px) = *img.pixels;
        if w == 0 || h == 0 || px.len() != (w * h * 4) as usize {
            return false;
        }
        let fresh = self.textures.get(&img.id.0).is_none_or(|t| !Arc::ptr_eq(&t.pixels, &img.pixels));
        if fresh {
            let texture = wgpu::util::DeviceExt::create_texture_with_data(
                device,
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("decal"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                px,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.textures.insert(img.id.0, Texture { pixels: img.pixels.clone(), view, used: true });
            // Slots showing the old picture get new bind groups.
            self.slots.retain(|_, s| s.image != img.id.0);
        }
        let Some(tex) = self.textures.get_mut(&img.id.0) else { return false };
        tex.used = true;
        if self.slots.get(&slot).is_none_or(|s| s.image != img.id.0) {
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("decal params"),
                size: std::mem::size_of::<[[f32; 4]; 6]>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("decal"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&tex.view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: params.as_entire_binding() },
                ],
            });
            self.slots.insert(slot, Slot { image: img.id.0, params, bind, used: true });
        }
        match self.slots.get_mut(&slot) {
            Some(s) => {
                s.used = true;
                true
            }
            None => false,
        }
    }

    pub fn bind(&self, decal: u64) -> Option<&wgpu::BindGroup> {
        self.slots.get(&decal).map(|s| &s.bind)
    }
}

/// Marks a slot as a body's custom material rather than a decal.
pub(crate) const MATERIAL: u64 = 1 << 62;

fn mesh_bounds(positions: &[[f32; 3]]) -> Option<(glam::Vec3, glam::Vec3)> {
    let first = glam::Vec3::from(*positions.first()?);
    Some(positions.iter().fold((first, first), |(lo, hi), p| (lo.min((*p).into()), hi.max((*p).into()))))
}

/// A custom material: the picture tiled by box projection (out.w = 2), one tile as wide as the body's largest
/// extent times the scale, starting at the body's low corner.
fn material_uniform((lo, hi): (glam::Vec3, glam::Vec3), scale: f32, aspect: f64) -> [[f32; 4]; 6] {
    let tile = ((hi - lo).max_element() * scale).max(1e-3);
    [
        [lo.x, lo.y, lo.z, 1.0],
        [0.0, 0.0, 0.0, tile],
        [0.0, 0.0, 0.0, tile / aspect.max(1e-3) as f32],
        [0.0, 0.0, 1.0, 2.0],
        [0.0; 4],
        [1.0, 0.0, -1.0, f32::MAX],
    ]
}

/// The shader's view of a decal (see decal.wgsl).
fn uniform(d: &Decal, aspect: f64) -> [[f32; 4]; 6] {
    let (right, up, out) = d.frame();
    let (w, h) = d.extent(aspect);
    let v = |p: glam::DVec3, w: f64| [p.x as f32, p.y as f32, p.z as f32, w as f32];
    let wrapped = d.projection == Projection::Cylindrical;
    let (axis, radius) = d.wrap_axis(aspect);
    let face = if d.extend { -1.0 } else { d.face as f64 };
    let (sin, cos) = d.rotate.to_radians().sin_cos();
    // Reaches through the body's walls only as far as the picture is big.
    let reach = w.max(h) * 0.5;
    [
        v(d.center, d.opacity as f64),
        v(right, w / 2.0),
        v(up, h / 2.0),
        v(out, if wrapped { 1.0 } else { 0.0 }),
        v(axis, radius),
        [cos as f32, sin as f32, face as f32, reach as f32],
    ]
}
