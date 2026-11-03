//! A blurred, 8× smaller copy of the viewport for the frosted sidebars that float over it (as observed: the
//! reference draws the 3D view under its sidebars and shows it through them heavily blurred).

use crate::gpu::{COLOR_FORMAT, DISPLAY_FORMAT};

pub const SHRINK: u32 = 8;
/// Horizontal + vertical gaussian passes; two rounds give a blur of roughly 30 px at full size.
const ROUNDS: usize = 2;

pub(crate) struct Blur {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    down: wgpu::RenderPipeline,
    h: wgpu::RenderPipeline,
    v: wgpu::RenderPipeline,
    target: Option<Targets>,
}

struct Targets {
    size: (u32, u32),
    a: wgpu::Texture,
    a_view: wgpu::TextureView,
    b_view: wgpu::TextureView,
    display: wgpu::TextureView,
}

impl Blur {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur"),
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make = |fs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("blur"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Blur { down: make("fs_down"), h: make("fs_h"), v: make("fs_v"), layout, sampler, target: None }
    }

    /// Blurs `src` (the viewport, `size` pixels) into the small texture. Returns whether that texture was
    /// recreated.
    /// `plain_display`: the display view gets a texture of its own (see [`crate::display_copy`]).
    pub fn run(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plain_display: bool,
        src: &wgpu::TextureView,
        size: (u32, u32),
    ) -> bool {
        let small = (size.0.div_ceil(SHRINK).max(1), size.1.div_ceil(SHRINK).max(1));
        let fresh = self.target.as_ref().is_none_or(|t| t.size != small);
        if fresh {
            let extent = wgpu::Extent3d { width: small.0, height: small.1, depth_or_array_layers: 1 };
            let tex = |label| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: COLOR_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                    view_formats: if plain_display { &[] } else { &[DISPLAY_FORMAT] },
                })
            };
            let a = tex("glass a");
            let b = tex("glass b");
            self.target = Some(Targets {
                size: small,
                a_view: a.create_view(&Default::default()),
                b_view: b.create_view(&Default::default()),
                display: if plain_display {
                    crate::display_copy::DisplayCopy::target(device, "glass display", extent)
                } else {
                    a.create_view(&wgpu::TextureViewDescriptor { format: Some(DISPLAY_FORMAT), ..Default::default() })
                },
                a,
            });
        }
        let Some(t) = self.target.as_ref() else { return fresh };
        let bind = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            })
        };
        let from_src = bind(src);
        let from_a = bind(&t.a_view);
        let from_b = bind(&t.b_view);
        let mut steps: Vec<(&wgpu::RenderPipeline, &wgpu::BindGroup, &wgpu::TextureView)> = vec![(&self.down, &from_src, &t.a_view)];
        for _ in 0..ROUNDS {
            steps.push((&self.h, &from_a, &t.b_view));
            steps.push((&self.v, &from_b, &t.a_view));
        }
        for (pipeline, group, out) in steps {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: out,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
        fresh
    }

    pub fn display(&self) -> Option<(&wgpu::TextureView, (u32, u32))> {
        self.target.as_ref().map(|t| (&t.display, t.size))
    }

    /// The blurred copy as color (sRGB) for sampling.
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|t| &t.a_view)
    }

    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|t| &t.a)
    }
}
