use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::DVec2;
use shapecraft_doc::Pick;
use shapecraft_kernel::Mesh;
use wgpu::util::DeviceExt;

use crate::blur::Blur;
use crate::scene::{Built, Segment, body_edges, build, edges_key};
use crate::{Camera, DisplayMode, SceneInput};

pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub(crate) const DISPLAY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const SAMPLES: u32 = 4;
const MAX_SELECTED_FACES: usize = 16;

const MESH_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32, 3 => Float32];
const LINE_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4];
const DOT_ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x4];
const FILL_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
const IMAGE_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ImageVertex {
    pos: [f32; 3],
    uv: [f32; 2],
}

/// A reference picture on the GPU: its texture (made once per picture), corners and opacity.
struct ImageGpu {
    pixels: Arc<(u32, u32, Vec<u8>)>,
    vertices: wgpu::Buffer,
    params: wgpu::Buffer,
    bind: wgpu::BindGroup,
    used: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no graphics adapter: {0}")]
    NoAdapter(String),
    #[error("graphics device: {0}")]
    Device(String),
    #[error("nothing has been rendered yet")]
    NoFrame,
    #[error("reading pixels back failed: {0}")]
    Readback(String),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    viewport: [f32; 4],
    light: [f32; 4],
    /// Section plane: normal and offset (n·p > w is cut away); `light.w` = 1 when on.
    section: [f32; 4],
    /// Surface analysis: mode (0 none, 1 zebra, 2 curvature), vertical, scale.
    analysis: [f32; 4],
    /// Visualization: x light intensity, y the backdrop's brightness, z the light rotation (radians), w 1 when on.
    visual: [f32; 4],
    /// The camera's screen-right, screen-up and backward directions; right.w and up.w are the tangents of half the
    /// field of view across and up (the panorama backdrop is seen through a perspective even in an orthographic
    /// view), back.w the backdrop's mip level.
    cam_right: [f32; 4],
    cam_up: [f32; 4],
    cam_back: [f32; 4],
    /// A custom environment: x 1 when one is selected, y log2 of its map's width, z its highest mip level.
    envp: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BodyUniform {
    color: [f32; 4],
    sel_color: [f32; 4],
    hover_color: [f32; 4],
    /// metal, gloss, 1 when it has an appearance, 1 in Visualization
    material: [f32; 4],
    flags: [u32; 4],
    sel: [u32; MAX_SELECTED_FACES],
    /// x: number of painted faces (up to MAX_PAINTED).
    paint: [u32; 4],
    paint_faces: [u32; MAX_PAINTED],
    paint_colors: [[f32; 4]; MAX_PAINTED],
}

/// Faces per body that can carry their own color on the GPU.
const MAX_PAINTED: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    face: u32,
    /// The triangle's Gaussian curvature (for the Curvature Map).
    curv: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineVertex {
    a: [f32; 3],
    b: [f32; 3],
    color: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DotVertex {
    center: [f32; 3],
    color: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FillVertex {
    pos: [f32; 3],
    color: [f32; 4],
}

struct BodyGpu {
    mesh: Arc<Mesh>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    uniform: wgpu::Buffer,
    /// What `uniform` holds, so an unchanged body writes nothing.
    written: Vec<u8>,
    bind: wgpu::BindGroup,
    used: bool,
}

/// Two triangles per segment, as (the depth-tested ones, the ones on top).
fn line_vertices(segments: &[Segment]) -> (Vec<LineVertex>, Vec<LineVertex>) {
    let mut lines = Vec::new();
    let mut overlays = Vec::new();
    for s in segments {
        let color = s.color.linear();
        let (a, b) = (s.a.as_vec3().to_array(), s.b.as_vec3().to_array());
        let quad = [(0.0, -1.0), (1.0, -1.0), (1.0, 1.0), (0.0, -1.0), (1.0, 1.0), (0.0, 1.0)];
        let dst = if s.overlay { &mut overlays } else { &mut lines };
        let clip = if s.clip { 1.0 } else { 0.0 };
        for (t, side) in quad {
            dst.push(LineVertex { a, b, color, params: [t, side, s.width, clip] });
        }
    }
    (lines, overlays)
}

/// A vertex buffer that grows as needed.
struct Stream {
    buf: Option<wgpu::Buffer>,
    cap: u64,
    len: u32,
    label: &'static str,
}

impl Stream {
    fn new(label: &'static str) -> Self {
        Self { buf: None, cap: 0, len: 0, label }
    }

    fn upload<T: Pod>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[T]) {
        self.len = data.len() as u32;
        if data.is_empty() {
            return;
        }
        let bytes: &[u8] = bytemuck::cast_slice(data);
        let need = bytes.len() as u64;
        if self.buf.is_none() || need > self.cap {
            let cap = need.next_power_of_two().max(4096);
            self.buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.cap = cap;
        }
        if let Some(buf) = &self.buf {
            queue.write_buffer(buf, 0, bytes);
        }
    }

    fn slice(&self) -> Option<wgpu::BufferSlice<'_>> {
        if self.len == 0 { None } else { self.buf.as_ref().map(|b| b.slice(..)) }
    }
}

struct Target {
    size: (u32, u32),
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    /// The same pixels viewed as plain UNORM: the bytes are already sRGB-encoded, which is what egui
    /// expects from a texture (it would otherwise linearize them a second time).
    display_view: wgpu::TextureView,
}

/// What the app needs to show the viewport.
pub struct Frame<'a> {
    pub view: &'a wgpu::TextureView,
    pub size: (u32, u32),
    /// Changes whenever the texture is recreated, so the UI knows to re-register it.
    pub generation: u64,
    /// The view blurred and 8× smaller, for the frosted sidebars, and its own generation.
    pub glass: Option<(&'a wgpu::TextureView, (u32, u32))>,
    pub glass_generation: u64,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    mesh_pipeline: wgpu::RenderPipeline,
    xray_pipeline: wgpu::RenderPipeline,
    ghost_pipeline: wgpu::RenderPipeline,
    /// The hidden part of what's selected (or hovered in the Items list), faintly over what hides it.
    through_pipeline: wgpu::RenderPipeline,
    /// Sketching: bodies write depth only, then their nearest surface is blended over what's behind.
    depth_only_pipeline: wgpu::RenderPipeline,
    fade_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    line_overlay_pipeline: wgpu::RenderPipeline,
    dot_pipeline: wgpu::RenderPipeline,
    fill_pipeline: wgpu::RenderPipeline,
    fill_overlay_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    image_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    images: HashMap<u64, ImageGpu>,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    globals_layout: wgpu::BindGroupLayout,
    /// Visualization's custom environment (see `envmap`), bound beside the globals.
    env: crate::envmap::EnvGpu,
    env_sampler: wgpu::Sampler,
    backdrop_pipeline: wgpu::RenderPipeline,
    body_layout: wgpu::BindGroupLayout,
    bodies: HashMap<usize, BodyGpu>,
    /// The bodies' edges, kept from frame to frame (see `scene::edges_key`).
    edges: Stream,
    edge_overlays: Stream,
    edges_key: Option<u64>,
    /// The grid and axes: under the edges, so an axis along an edge doesn't show over it.
    under: Stream,
    under_overlays: Stream,
    lines: Stream,
    line_overlays: Stream,
    dots: Stream,
    fills: Stream,
    fill_overlays: Stream,
    target: Option<Target>,
    generation: u64,
    blur: Blur,
    decals: crate::decal::Decals,
    /// None where the adapter can't draw the distance texture.
    dof: Option<crate::dof::Dof>,
    /// Set where textures can't be viewed in another format.
    display_copy: Option<crate::display_copy::DisplayCopy>,
    glass_generation: u64,
}

/// The globals uniform, and beside it the custom environment's radiance and irradiance maps and their sampler.
fn globals_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let tex = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("globals"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
            tex(1),
            tex(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn globals_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    env: &crate::envmap::EnvGpu,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("globals"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&env.radiance) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&env.irradiance) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn uniform_layout(device: &wgpu::Device, label: &str, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: vis,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    })
}

struct PipelineSpec<'a> {
    label: &'a str,
    vs: &'a str,
    fs: &'a str,
    buffers: &'a [wgpu::VertexBufferLayout<'a>],
    topology: wgpu::PrimitiveTopology,
    depth_write: bool,
    depth_compare: wgpu::CompareFunction,
    blend: Option<wgpu::BlendState>,
}

impl Renderer {
    /// Uses a device the host (the app's window) already has.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, adapter: &wgpu::Adapter) -> Self {
        // wgpu panics on a validation error unless it has a handler; a bad effect must not take the app down.
        device.on_uncaptured_error(std::sync::Arc::new(|e| eprintln!("graphics error: {e}")));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shapecraft shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals_layout = globals_layout(&device);
        let body_layout = uniform_layout(&device, "body", wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT);
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let env = crate::envmap::EnvGpu::empty(&device, &queue);
        let env_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("environment"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let globals_bind = globals_bind(&device, &globals_layout, &globals, &env, &env_sampler);
        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&globals_layout), Some(&body_layout)],
            immediate_size: 0,
        });
        let flat_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("flat"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });

        let make = |layout: &wgpu::PipelineLayout, spec: PipelineSpec| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(spec.label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(spec.vs),
                    compilation_options: Default::default(),
                    buffers: &spec.buffers.iter().cloned().map(Some).collect::<Vec<_>>(),
                },
                primitive: wgpu::PrimitiveState { topology: spec.topology, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(spec.depth_write),
                    depth_compare: Some(spec.depth_compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(spec.fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: spec.blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        let layout_of = |stride: usize, attrs: &'static [wgpu::VertexAttribute]| wgpu::VertexBufferLayout {
            array_stride: stride as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: attrs,
        };
        let mesh_buf = [layout_of(std::mem::size_of::<MeshVertex>(), &MESH_ATTRS)];
        let line_buf = [layout_of(std::mem::size_of::<LineVertex>(), &LINE_ATTRS)];
        let dot_buf = [layout_of(std::mem::size_of::<DotVertex>(), &DOT_ATTRS)];
        let fill_buf = [layout_of(std::mem::size_of::<FillVertex>(), &FILL_ATTRS)];
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        let tri = wgpu::PrimitiveTopology::TriangleList;
        use wgpu::CompareFunction::{Always, LessEqual};

        let mesh_pipeline = make(
            &mesh_layout,
            PipelineSpec {
                label: "mesh",
                vs: "vs_mesh",
                fs: "fs_mesh",
                buffers: &mesh_buf,
                topology: tri,
                depth_write: true,
                depth_compare: LessEqual,
                blend: None,
            },
        );
        let xray_pipeline = make(
            &mesh_layout,
            PipelineSpec {
                label: "x-ray",
                vs: "vs_mesh",
                fs: "fs_mesh",
                buffers: &mesh_buf,
                topology: tri,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
            },
        );
        // A boolean's bodies: see-through and drawn over everything, so all of each shows.
        let ghost_pipeline = make(
            &mesh_layout,
            PipelineSpec {
                label: "ghost bodies",
                vs: "vs_mesh",
                fs: "fs_mesh",
                buffers: &mesh_buf,
                topology: tri,
                depth_write: false,
                depth_compare: Always,
                blend: alpha,
            },
        );
        let through_pipeline = make(
            &mesh_layout,
            PipelineSpec {
                label: "hidden selection",
                vs: "vs_mesh",
                fs: "fs_through",
                buffers: &mesh_buf,
                topology: tri,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Greater,
                blend: alpha,
            },
        );
        let keep = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let depth_only_pipeline = make(
            &mesh_layout,
            PipelineSpec {
                label: "depth only",
                vs: "vs_mesh",
                fs: "fs_mesh",
                buffers: &mesh_buf,
                topology: tri,
                depth_write: true,
                depth_compare: LessEqual,
                blend: Some(wgpu::BlendState { color: keep, alpha: keep }),
            },
        );
        let fade_pipeline = make(
            &mesh_layout,
            PipelineSpec {
                label: "faded bodies",
                vs: "vs_mesh",
                fs: "fs_mesh",
                buffers: &mesh_buf,
                topology: tri,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Equal,
                blend: alpha,
            },
        );
        let line_pipeline = make(
            &flat_layout,
            PipelineSpec {
                label: "lines",
                vs: "vs_line",
                fs: "fs_line",
                buffers: &line_buf,
                topology: tri,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
            },
        );
        let line_overlay_pipeline = make(
            &flat_layout,
            PipelineSpec {
                label: "line overlays",
                vs: "vs_line",
                fs: "fs_line",
                buffers: &line_buf,
                topology: tri,
                depth_write: false,
                depth_compare: Always,
                blend: alpha,
            },
        );
        let dot_pipeline = make(
            &flat_layout,
            PipelineSpec {
                label: "dots",
                vs: "vs_dot",
                fs: "fs_dot",
                buffers: &dot_buf,
                topology: tri,
                depth_write: false,
                depth_compare: Always,
                blend: alpha,
            },
        );
        let fill_pipeline = make(
            &flat_layout,
            PipelineSpec {
                label: "fills",
                vs: "vs_fill",
                fs: "fs_fill",
                buffers: &fill_buf,
                topology: tri,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
            },
        );
        // The panorama behind everything (Visualization with a custom environment): one triangle over the view.
        let backdrop_pipeline = make(
            &flat_layout,
            PipelineSpec {
                label: "panorama backdrop",
                vs: "vs_backdrop",
                fs: "fs_backdrop",
                buffers: &[],
                topology: tri,
                depth_write: false,
                depth_compare: Always,
                blend: None,
            },
        );
        let fill_overlay_pipeline = make(
            &flat_layout,
            PipelineSpec {
                label: "fill overlays",
                vs: "vs_fill",
                fs: "fs_fill",
                buffers: &fill_buf,
                topology: tri,
                depth_write: false,
                depth_compare: Always,
                blend: alpha,
            },
        );

        // Reference pictures have their own small shader: globals, then the texture, sampler and opacity.
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("image.wgsl").into()),
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image"),
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
        let image_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image"),
            bind_group_layouts: &[Some(&globals_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        let image_buf = [layout_of(std::mem::size_of::<ImageVertex>(), &IMAGE_ATTRS)];
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("images"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_image"),
                compilation_options: Default::default(),
                buffers: &image_buf.iter().cloned().map(Some).collect::<Vec<_>>(),
            },
            primitive: wgpu::PrimitiveState { topology: tri, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(LessEqual),
                stencil: Default::default(),
                // Pushed back a little, so a face lying on the picture (a part modeled on its blueprint) wins.
                bias: wgpu::DepthBiasState { constant: 4, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_image"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend: alpha, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blur = Blur::new(&device);
        let decals = crate::decal::Decals::new(&device, &globals_layout, mesh_buf[0].clone());
        let view_formats = adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VIEW_FORMATS);
        let display_copy = (!view_formats).then(|| crate::display_copy::DisplayCopy::new(&device));
        let dof = crate::dof::supported(adapter).then(|| crate::dof::Dof::new(&device, &shader, &mesh_layout, mesh_buf[0].clone()));

        Self {
            device,
            queue,
            mesh_pipeline,
            xray_pipeline,
            through_pipeline,
            ghost_pipeline,
            depth_only_pipeline,
            fade_pipeline,
            line_pipeline,
            line_overlay_pipeline,
            dot_pipeline,
            fill_pipeline,
            fill_overlay_pipeline,
            image_pipeline,
            image_layout,
            sampler,
            images: HashMap::new(),
            globals,
            globals_bind,
            globals_layout,
            env,
            env_sampler,
            backdrop_pipeline,
            body_layout,
            bodies: HashMap::new(),
            edges: Stream::new("edges"),
            edge_overlays: Stream::new("edge overlays"),
            edges_key: None,
            under: Stream::new("lines under edges"),
            under_overlays: Stream::new("overlays under edges"),
            lines: Stream::new("lines"),
            line_overlays: Stream::new("line overlays"),
            dots: Stream::new("dots"),
            fills: Stream::new("fills"),
            fill_overlays: Stream::new("fill overlays"),
            target: None,
            generation: 0,
            blur,
            decals,
            dof,
            display_copy,
            glass_generation: 0,
        }
    }

    /// Creates its own device, for the CLI, MCP and thumbnails. OpenGL only gets a turn alone: merely loading it beside
    /// Vulkan crashes AMD's Windows driver, in native code that no error handling can catch.
    pub fn headless() -> Result<Self, RenderError> {
        Self::headless_on(wgpu::Backends::PRIMARY).or_else(|_| Self::headless_on(wgpu::Backends::GL))
    }

    fn headless_on(backends: wgpu::Backends) -> Result<Self, RenderError> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backends;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("shapecraft headless"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;
        Ok(Self::new(device, queue, &adapter))
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    fn ensure_target(&mut self, size: (u32, u32)) {
        let size = (size.0.clamp(1, 8192), size.1.clamp(1, 8192));
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let extent = wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 };
        let tex = |label, format, samples, usage, view_formats: &[wgpu::TextureFormat]| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats,
            })
        };
        let msaa = tex("viewport msaa", COLOR_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT, &[]);
        let depth = tex("viewport depth", DEPTH_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT, &[]);
        let resolve = tex(
            "viewport",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            if self.display_copy.is_some() { &[] } else { &[DISPLAY_FORMAT] },
        );
        let display_view = if self.display_copy.is_some() {
            crate::display_copy::DisplayCopy::target(&self.device, "viewport display", extent)
        } else {
            resolve.create_view(&wgpu::TextureViewDescriptor { format: Some(DISPLAY_FORMAT), ..Default::default() })
        };
        self.target = Some(Target {
            size,
            msaa: msaa.create_view(&Default::default()),
            depth: depth.create_view(&Default::default()),
            resolve_view: resolve.create_view(&Default::default()),
            display_view,
            resolve,
        });
        self.generation += 1;
    }

    fn body_gpu(&mut self, id: usize, mesh: &Arc<Mesh>) -> Option<&mut BodyGpu> {
        let stale = self.bodies.get(&id).is_none_or(|b| !Arc::ptr_eq(&b.mesh, mesh));
        if stale {
            let gpu = self.upload_body(mesh);
            self.bodies.insert(id, gpu);
        }
        let b = self.bodies.get_mut(&id)?;
        b.used = true;
        Some(b)
    }

    fn upload_body(&self, mesh: &Arc<Mesh>) -> BodyGpu {
        // Faces are per triangle in the mesh but flat per vertex in the shader, so every triangle gets its
        // own three vertices.
        let mut verts = Vec::with_capacity(mesh.indices.len());
        for (t, tri) in mesh.indices.chunks(3).enumerate() {
            let face = mesh.triangle_faces.get(t).copied().unwrap_or(0);
            let curv = gaussian_curvature(mesh, tri);
            for &i in tri {
                if let Some(p) = mesh.positions.get(i as usize) {
                    let normal = mesh.normals.get(i as usize).copied().unwrap_or([0.0, 0.0, 1.0]);
                    verts.push(MeshVertex { pos: *p, normal, face, curv });
                }
            }
        }
        let indices: Vec<u32> = (0..verts.len() as u32).collect();
        let vertices = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("body vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("body indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("body uniform"),
            size: std::mem::size_of::<BodyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("body"),
            layout: &self.body_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        BodyGpu {
            mesh: mesh.clone(),
            vertices,
            indices: index_buf,
            count: indices.len() as u32,
            uniform,
            written: Vec::new(),
            bind,
            used: true,
        }
    }

    /// Draws the scene into the offscreen viewport texture.
    pub fn render(&mut self, scene: &SceneInput, camera: &Camera, size: (u32, u32)) -> Result<Frame<'_>, RenderError> {
        self.ensure_target(size);
        let size = self.target.as_ref().map(|t| t.size).unwrap_or((1, 1));
        let px = DVec2::new(size.0 as f64, size.1 as f64);
        let c = scene.colors;

        // An orthographic view has no eye point: w = 0 makes `eye` the direction (times the distance) instead.
        let (eye, eye_w) =
            if camera.is_ortho() { ((camera.back() * camera.distance).as_vec3(), 0.0) } else { (camera.eye().as_vec3(), 1.0) };
        // Bodies are shaded from above and from the viewer's front right: the light turns with the camera (a face
        // turned to the viewer brightens, s20-055) while tops keep the body colour. Fitted to the reference's default
        // view (s20-030: walls 66 and 114 for the default grey). In Visualization the environment's sun only casts
        // the shadow: the reference's faces keep their tones as the sun turns, s13-021.
        let back = camera.back().as_vec3();
        let toward = glam::Vec2::new(back.x, back.y).try_normalize().unwrap_or(glam::Vec2::new(0.0, -1.0));
        let right = glam::Vec2::new(-toward.y, toward.x);
        let flat = right * 0.284 + toward * 0.508;
        let light = glam::Vec3::new(flat.x, flat.y, 1.0).normalize();
        let intensity = if scene.presentation { scene.doc.visual.light_intensity as f32 } else { 1.0 };
        let (section, on) = match scene.section {
            Some((o, n)) => {
                let n = n.normalize_or(glam::DVec3::Z);
                ([n.x as f32, n.y as f32, n.z as f32, n.dot(o) as f32], 1.0)
            }
            None => ([0.0; 4], 0.0),
        };
        // A custom environment: its maps made once per panorama, bound beside the globals.
        let panorama = scene.doc.visual.panorama().filter(|_| scene.presentation);
        if let Some(p) = panorama
            && self.env.key != Some((p.id.0, Arc::as_ptr(&p.pixels) as usize))
        {
            self.env = crate::envmap::EnvGpu::new(&self.device, &self.queue, p.id.0, &p.pixels);
            self.globals_bind = globals_bind(&self.device, &self.globals_layout, &self.globals, &self.env, &self.env_sampler);
        }
        let (cam_r, cam_u) = camera.basis();
        let tan_y = (camera.fov_y.clamp(0.05, 3.0) / 2.0).tan() as f32;
        let aspect = (px.x / px.y.max(1.0)) as f32;
        // The backdrop's mip level: about one texel per pixel.
        let texel = std::f32::consts::TAU / self.env.log2_width.exp2().max(1.0);
        let pixel = 2.0 * tan_y / px.y.max(1.0) as f32;
        let backdrop_lod = (pixel / texel).log2().clamp(0.0, self.env.max_lod);
        let globals = Globals {
            view_proj: camera.view_proj(px).to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, eye_w],
            viewport: [px.x as f32, px.y as f32, 1.0 / px.x as f32, 1.0 / px.y as f32],
            light: [light.x, light.y, light.z, on],
            section,
            analysis: if scene.presentation { [0.0; 4] } else { scene.display.analysis() },
            visual: if scene.presentation {
                let bd = scene.backdrop.linear();
                [intensity, 0.2126 * bd[0] + 0.7152 * bd[1] + 0.0722 * bd[2], (scene.doc.visual.light_rotation as f32).to_radians(), 1.0]
            } else {
                [1.0, 0.0, 0.0, 0.0]
            },
            cam_right: [cam_r.x as f32, cam_r.y as f32, cam_r.z as f32, tan_y * aspect],
            cam_up: [cam_u.x as f32, cam_u.y as f32, cam_u.z as f32, tan_y],
            cam_back: [back.x, back.y, back.z, backdrop_lod],
            envp: [if panorama.is_some() { 1.0 } else { 0.0 }, self.env.log2_width, self.env.max_lod, 0.0],
        };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        for b in self.bodies.values_mut() {
            b.used = false;
        }
        let sketching = scene.active_sketch.is_some() && !scene.presentation && scene.display.solid();
        let mut draws = Vec::new();
        let mut ghost_draws = Vec::new();
        let mut see_draws = Vec::new();
        let mut through_draws = Vec::new();
        for body in scene.doc.bodies.iter().filter(|b| b.visible) {
            let key = body.id.0 as usize;
            let mut color = c.body.linear();
            let mut material = [0.0, 0.0, 0.0, if scene.presentation { 1.0 } else { 0.0 }];
            if let Some(a) = &body.appearance {
                let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
                color = [lin(a.color[0]), lin(a.color[1]), lin(a.color[2]), 1.0];
                material = [a.metal, a.gloss, 1.0, material[3]];
            }
            let ghost = scene.ghosts.iter().find(|(id, _)| *id == body.id).map(|(_, g)| g.linear());
            if let Some((_, t)) = scene.tints.iter().find(|(id, _)| *id == body.id) {
                color = t.linear();
            }
            // A selected body is drawn see-through cyan (observed 329, 377), so what's inside and behind shows.
            // A tint with alpha (a face growing out under Extrude) is see-through the same way, at its own alpha.
            let tinted_through = color[3] < 0.999;
            let see_through = ghost.is_none()
                && !sketching
                && scene.display.solid()
                && (tinted_through || crate::scene::fully_selected(body, scene.selection));
            if let Some(g) = ghost {
                color = [g[0], g[1], g[2], 0.3];
            } else if see_through {
                color[3] = color[3].min(0.85);
            } else if scene.display == DisplayMode::XRay {
                color[3] = c.xray.clamp(0.02, 0.95);
            } else if sketching {
                // As observed, bodies fade while sketching so the sketch plane's grid shows through them.
                color[3] = 0.4;
            }
            let mut uni = BodyUniform {
                color,
                sel_color: c.selected.linear(),
                hover_color: c.hover.linear(),
                material,
                flags: [0; 4],
                sel: [0; MAX_SELECTED_FACES],
                paint: [0; 4],
                paint_faces: [0; MAX_PAINTED],
                paint_colors: [[0.0; 4]; MAX_PAINTED],
            };
            let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
            for (k, (face, c)) in body.face_colors.iter().take(MAX_PAINTED).enumerate() {
                uni.paint_faces[k] = *face;
                uni.paint_colors[k] = [lin(c[0]), lin(c[1]), lin(c[2]), 1.0];
                uni.paint[0] = k as u32 + 1;
            }
            let mut n = 0usize;
            for p in scene.selection {
                match *p {
                    Pick::Body { body: b } if b == body.id => uni.flags[0] = 1,
                    Pick::Face { body: b, face } if b == body.id && n < MAX_SELECTED_FACES => {
                        uni.sel[n] = face;
                        n += 1;
                    }
                    _ => {}
                }
            }
            uni.flags[2] = n as u32;
            if let Some(Pick::Face { body: b, face }) = scene.hover
                && b == body.id
            {
                uni.flags[1] = face + 1;
            }
            if scene.hover == Some(Pick::Body { body: body.id }) {
                uni.flags[3] = 1;
            }
            let Some(gpu) = self.body_gpu(key, &body.mesh) else { continue };
            let bytes = bytemuck::bytes_of(&uni);
            if gpu.written != bytes {
                gpu.written = bytes.to_vec();
                let uniform = gpu.uniform.clone();
                self.queue.write_buffer(&uniform, 0, bytes);
            }
            // Anything picked or hovered from the Items list also shows where other faces hide it.
            if ghost.is_none() && !sketching && scene.display.solid() && (n > 0 || uni.flags[0] == 1 || uni.flags[3] == 1) {
                through_draws.push(key);
            }
            if ghost.is_some() {
                ghost_draws.push(key);
            } else if see_through {
                see_draws.push(key);
            } else {
                draws.push(key);
            }
        }
        self.bodies.retain(|_, b| b.used);
        let image_draws = self.prepare_images(scene);
        // Decals show on shaded bodies (View ▸ Show Decals), not while sketching.
        let decal_draws: Vec<(u64, usize)> = if scene.display.solid() && !sketching && scene.show_decals {
            self.decals
                .prepare(&self.device, &self.queue, scene.doc)
                .into_iter()
                .map(|(d, b)| (d, b as usize))
                .filter(|(d, b)| draws.contains(b) || (d & crate::decal::MATERIAL == 0 && see_draws.contains(b)))
                .collect()
        } else {
            Vec::new()
        };

        let key = edges_key(scene);
        if self.edges_key != Some(key) {
            self.edges_key = Some(key);
            let (lines, overlays) = line_vertices(&body_edges(scene));
            self.edges.upload(&self.device, &self.queue, &lines);
            self.edge_overlays.upload(&self.device, &self.queue, &overlays);
        }
        let built = build(scene, camera, px);
        self.upload(&built);

        // Depth of field once a focus point is set (Visualization ▸ Camera, s13-025): the view is drawn aside first.
        let focus = scene.doc.visual.focus.filter(|_| scene.presentation && scene.display.solid() && self.dof.is_some());
        if focus.is_some()
            && let Some(dof) = &mut self.dof
        {
            dof.ensure(&self.device, size);
        }
        let sharp = focus.and_then(|_| self.dof.as_ref()?.current());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("viewport") });
        if let Some(t) = &self.target {
            let bg = if scene.presentation { scene.backdrop.linear() } else { c.background.linear() };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    depth_slice: None,
                    resolve_target: Some(sharp.map_or(&t.resolve_view, |d| &d.sharp)),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: bg[0] as f64, g: bg[1] as f64, b: bg[2] as f64, a: 1.0 }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            if panorama.is_some() {
                pass.set_pipeline(&self.backdrop_pipeline);
                pass.draw(0..3, 0..1);
            }
            let mesh_pipeline = match scene.display {
                DisplayMode::Shaded | DisplayMode::Zebra { .. } | DisplayMode::Curvature { .. } | DisplayMode::Visualized => {
                    Some(&self.mesh_pipeline)
                }
                DisplayMode::XRay => Some(&self.xray_pipeline),
                DisplayMode::Wireframe => None,
            };
            let mesh_passes: Vec<&wgpu::RenderPipeline> = match mesh_pipeline {
                Some(_) if sketching => vec![&self.depth_only_pipeline, &self.fade_pipeline],
                Some(mp) => vec![mp],
                None => Vec::new(),
            };
            for mp in mesh_passes {
                pass.set_pipeline(mp);
                for key in &draws {
                    if let Some(b) = self.bodies.get(key) {
                        pass.set_bind_group(1, &b.bind, &[]);
                        pass.set_vertex_buffer(0, b.vertices.slice(..));
                        pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..b.count, 0, 0..1);
                    }
                }
            }
            // What's picked but hidden, faintly over the solid bodies that hide it (before the see-through
            // selected bodies write their depth, so those only hide it where they're not themselves picked).
            if !through_draws.is_empty() {
                pass.set_pipeline(&self.through_pipeline);
                for key in &through_draws {
                    if let Some(b) = self.bodies.get(key) {
                        pass.set_bind_group(1, &b.bind, &[]);
                        pass.set_vertex_buffer(0, b.vertices.slice(..));
                        pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..b.count, 0, 0..1);
                    }
                }
            }
            // Selected bodies: only their nearest surface, a little see-through (depth first, then blended).
            for mp in [&self.depth_only_pipeline, &self.fade_pipeline] {
                pass.set_pipeline(mp);
                for key in &see_draws {
                    if let Some(b) = self.bodies.get(key) {
                        pass.set_bind_group(1, &b.bind, &[]);
                        pass.set_vertex_buffer(0, b.vertices.slice(..));
                        pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..b.count, 0, 0..1);
                    }
                }
            }
            if !decal_draws.is_empty() {
                pass.set_pipeline(&self.decals.pipeline);
                for (decal, key) in &decal_draws {
                    if let (Some(b), Some(bind)) = (self.bodies.get(key), self.decals.bind(*decal)) {
                        pass.set_bind_group(1, bind, &[]);
                        pass.set_vertex_buffer(0, b.vertices.slice(..));
                        pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..b.count, 0, 0..1);
                    }
                }
            }
            // See-through bodies after the solid ones, so those show behind them.
            if !ghost_draws.is_empty() && scene.display != DisplayMode::Wireframe {
                pass.set_pipeline(&self.ghost_pipeline);
                for key in &ghost_draws {
                    if let Some(b) = self.bodies.get(key) {
                        pass.set_bind_group(1, &b.bind, &[]);
                        pass.set_vertex_buffer(0, b.vertices.slice(..));
                        pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..b.count, 0, 0..1);
                    }
                }
            }
            if !image_draws.is_empty() {
                pass.set_pipeline(&self.image_pipeline);
                for id in &image_draws {
                    if let Some(img) = self.images.get(id) {
                        pass.set_bind_group(1, &img.bind, &[]);
                        pass.set_vertex_buffer(0, img.vertices.slice(..));
                        pass.draw(0..6, 0..1);
                    }
                }
            }
            let streams: [(&wgpu::RenderPipeline, &Stream); 9] = [
                (&self.line_pipeline, &self.under),
                (&self.line_pipeline, &self.edges),
                (&self.line_pipeline, &self.lines),
                (&self.fill_pipeline, &self.fills),
                (&self.fill_overlay_pipeline, &self.fill_overlays),
                (&self.line_overlay_pipeline, &self.under_overlays),
                (&self.line_overlay_pipeline, &self.edge_overlays),
                (&self.line_overlay_pipeline, &self.line_overlays),
                (&self.dot_pipeline, &self.dots),
            ];
            for (pipeline, stream) in streams {
                if let Some(slice) = stream.slice() {
                    pass.set_pipeline(pipeline);
                    pass.set_vertex_buffer(0, slice);
                    pass.draw(0..stream.len, 0..1);
                }
            }
        }
        // Each body pixel's distance from the eye, for depth of field.
        if let Some(d) = sharp {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("distances"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &d.distance,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &d.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            if let Some(dof) = &self.dof {
                pass.set_pipeline(&dof.depth_pipeline);
            }
            for key in draws.iter().chain(&see_draws) {
                if let Some(b) = self.bodies.get(key) {
                    pass.set_bind_group(1, &b.bind, &[]);
                    pass.set_vertex_buffer(0, b.vertices.slice(..));
                    pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..b.count, 0, 0..1);
                }
            }
        }
        if let Some(t) = self.target.as_ref()
            && self.blur.run(&self.device, &mut encoder, self.display_copy.is_some(), sharp.map_or(&t.resolve_view, |d| &d.sharp), t.size)
        {
            self.glass_generation += 1;
        }
        if let (Some(f), Some(t), Some(blurred), Some(dof)) = (focus, self.target.as_ref(), self.blur.view(), self.dof.as_ref()) {
            let v = &scene.doc.visual;
            // Distances as fs_depth measures them; differences count against the focus distance (the camera's in an
            // orthographic view). A wider aperture (smaller ƒ) and more Blur Intensity blur more.
            let (fd, against) = if camera.is_ortho() {
                (100000.0 - f.dot(camera.back()), camera.distance)
            } else {
                ((f - camera.eye()).length(), (f - camera.eye()).length())
            };
            let strength = v.blur * 10.0 / v.aperture.max(0.1);
            dof.composite(
                &self.device,
                &self.queue,
                &mut encoder,
                blurred,
                &t.resolve_view,
                [fd as f32, against as f32, strength as f32, 0.0],
            );
        }
        if let Some(copy) = &self.display_copy {
            if let Some(t) = &self.target {
                copy.run(&self.device, &mut encoder, &t.resolve_view, &t.display_view);
            }
            if let (Some(from), Some((to, _))) = (self.blur.view(), self.blur.display()) {
                copy.run(&self.device, &mut encoder, from, to);
            }
        }
        self.queue.submit([encoder.finish()]);

        let t = self.target.as_ref().ok_or(RenderError::NoFrame)?;
        Ok(Frame {
            view: &t.display_view,
            size: t.size,
            generation: self.generation,
            glass: self.blur.display(),
            glass_generation: self.glass_generation,
        })
    }

    /// Keeps a texture per visible picture (made when the picture is new) and writes its corners and opacity.
    /// Returns the pictures to draw.
    fn prepare_images(&mut self, scene: &SceneInput) -> Vec<u64> {
        for img in self.images.values_mut() {
            img.used = false;
        }
        let mut draws = Vec::new();
        for im in scene.doc.images.iter().filter(|i| i.visible) {
            let (w, h, ref px) = *im.pixels;
            if w == 0 || h == 0 || px.len() != (w * h * 4) as usize {
                continue;
            }
            let fresh = self.images.get(&im.id.0).is_none_or(|g| !Arc::ptr_eq(&g.pixels, &im.pixels));
            if fresh {
                let texture = self.device.create_texture_with_data(
                    &self.queue,
                    &wgpu::TextureDescriptor {
                        label: Some("image"),
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
                let params = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("image params"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let vertices = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("image quad"),
                    size: (std::mem::size_of::<ImageVertex>() * 6) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("image"),
                    layout: &self.image_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                        wgpu::BindGroupEntry { binding: 2, resource: params.as_entire_binding() },
                    ],
                });
                self.images.insert(im.id.0, ImageGpu { pixels: im.pixels.clone(), vertices, params, bind, used: true });
            }
            let Some(gpu) = self.images.get_mut(&im.id.0) else { continue };
            gpu.used = true;
            let c = im.corners().map(|p| p.as_vec3().to_array());
            // Picture rows run top down; the plane's y runs up.
            let quad =
                [(c[0], [0.0, 1.0]), (c[1], [1.0, 1.0]), (c[2], [1.0, 0.0]), (c[0], [0.0, 1.0]), (c[2], [1.0, 0.0]), (c[3], [0.0, 0.0])];
            let verts: Vec<ImageVertex> = quad.iter().map(|(p, uv)| ImageVertex { pos: *p, uv: *uv }).collect();
            self.queue.write_buffer(&gpu.vertices, 0, bytemuck::cast_slice(&verts));
            self.queue.write_buffer(&gpu.params, 0, bytemuck::cast_slice(&[im.opacity, 0.0, 0.0, 0.0]));
            draws.push(im.id.0);
        }
        self.images.retain(|_, g| g.used);
        draws
    }

    fn upload(&mut self, built: &Built) {
        let (under, rest) = built.segments.split_at(built.under_edges.min(built.segments.len()));
        let (under, under_overlays) = line_vertices(under);
        let (lines, line_overlays) = line_vertices(rest);
        self.under.upload(&self.device, &self.queue, &under);
        self.under_overlays.upload(&self.device, &self.queue, &under_overlays);
        let mut dots = Vec::new();
        for (p, col, size) in &built.dots {
            let center = p.as_vec3().to_array();
            let color = col.linear();
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                dots.push(DotVertex { center, color, params: [x, y, size.abs(), if *size < 0.0 { 1.6 } else { 0.0 }] });
            }
        }
        let mut fills = Vec::new();
        let mut fill_overlays = Vec::new();
        for (tri, col, overlay) in &built.fills {
            let color = col.linear();
            let dst = if *overlay { &mut fill_overlays } else { &mut fills };
            for p in tri {
                dst.push(FillVertex { pos: p.as_vec3().to_array(), color });
            }
        }
        self.lines.upload(&self.device, &self.queue, &lines);
        self.line_overlays.upload(&self.device, &self.queue, &line_overlays);
        self.dots.upload(&self.device, &self.queue, &dots);
        self.fills.upload(&self.device, &self.queue, &fills);
        self.fill_overlays.upload(&self.device, &self.queue, &fill_overlays);
    }

    /// Reads the last rendered frame back as tightly packed RGBA8 (sRGB) pixels.
    pub fn read_pixels(&self) -> Result<(u32, u32, Vec<u8>), RenderError> {
        let t = self.target.as_ref().ok_or(RenderError::NoFrame)?;
        self.read_texture(&t.resolve, t.size)
    }

    /// Reads the blurred glass copy of the last frame (RGBA8, sRGB).
    pub fn read_glass(&self) -> Result<(u32, u32, Vec<u8>), RenderError> {
        match (self.blur.texture(), self.blur.display()) {
            (Some(tex), Some((_, size))) => self.read_texture(tex, size),
            _ => Err(RenderError::NoFrame),
        }
    }

    fn read_texture(&self, texture: &wgpu::Texture, (w, h): (u32, u32)) -> Result<(u32, u32, Vec<u8>), RenderError> {
        let row = w * 4;
        let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: padded as u64 * h as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        rx.recv().map_err(|e| RenderError::Readback(e.to_string()))?.map_err(|e| RenderError::Readback(e.to_string()))?;
        let data = slice.get_mapped_range().map_err(|e| RenderError::Readback(e.to_string()))?;
        let mut out = Vec::with_capacity((row * h) as usize);
        for y in 0..h as usize {
            let start = y * padded as usize;
            out.extend_from_slice(data.get(start..start + row as usize).unwrap_or(&[]));
        }
        drop(data);
        buf.unmap();
        Ok((w, h, out))
    }
}

/// A triangle's Gaussian curvature from its vertex normals: the area the normals sweep on the unit sphere over the
/// triangle's own area (signed, so saddles come out negative, flat faces and cylinders zero).
fn gaussian_curvature(mesh: &Mesh, tri: &[u32]) -> f32 {
    let v = |i: usize| -> Option<(glam::Vec3, glam::Vec3)> {
        let k = *tri.get(i)? as usize;
        Some((glam::Vec3::from(*mesh.positions.get(k)?), glam::Vec3::from(*mesh.normals.get(k)?)))
    };
    let (Some((p0, n0)), Some((p1, n1)), Some((p2, n2))) = (v(0), v(1), v(2)) else { return 0.0 };
    let area = (p1 - p0).cross(p2 - p0);
    let len2 = area.length_squared();
    if len2 < 1e-12 {
        return 0.0;
    }
    let k = (n1 - n0).cross(n2 - n0).dot(area) / len2;
    if k.is_finite() { k } else { 0.0 }
}
