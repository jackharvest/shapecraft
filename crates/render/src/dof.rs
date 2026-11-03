//! Depth of field for Visualization: the bodies' distances drawn into a float texture, then the sharp view mixed with
//! its blurred copy (the frosted-glass blur) by how far each pixel is from the focus.

use crate::gpu::{COLOR_FORMAT, DEPTH_FORMAT};

const DISTANCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

/// Whether this adapter can draw the distances. Some OpenGL drivers can't, and those simply go without the effect.
pub(crate) fn supported(adapter: &wgpu::Adapter) -> bool {
    adapter.get_texture_format_features(DISTANCE_FORMAT).allowed_usages.contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
}

pub(crate) struct Dof {
    /// Draws a body's distances (shader.wgsl's vs_mesh + fs_depth).
    pub depth_pipeline: wgpu::RenderPipeline,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    target: Option<Targets>,
}

/// The textures sized to the view.
pub(crate) struct Targets {
    size: (u32, u32),
    /// The view before depth of field.
    pub sharp: wgpu::TextureView,
    pub distance: wgpu::TextureView,
    pub depth: wgpu::TextureView,
}

impl Dof {
    pub fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        mesh_layout: &wgpu::PipelineLayout,
        mesh: wgpu::VertexBufferLayout,
    ) -> Self {
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("distances"),
            layout: Some(mesh_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_mesh"),
                compilation_options: Default::default(),
                buffers: &[Some(mesh)],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_depth"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: DISTANCE_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let dof_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dof shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("dof.wgsl").into()),
        });
        let tex = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dof"),
            entries: &[
                tex(0, true),
                tex(1, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex(3, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
            label: Some("dof"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dof"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &dof_shader,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &dof_shader,
                entry_point: Some("fs_dof"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("dof"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dof params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { depth_pipeline, pipeline, layout, sampler, params, target: None }
    }

    /// Makes the textures for a view of this size (again when it changes).
    pub fn ensure(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let tex = |label, format, usage| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            let both = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
            self.target = Some(Targets {
                size,
                sharp: tex("dof sharp", COLOR_FORMAT, both),
                distance: tex("dof distance", DISTANCE_FORMAT, both),
                depth: tex("dof depth", DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT),
            });
        }
    }

    pub fn current(&self) -> Option<&Targets> {
        self.target.as_ref()
    }

    /// Mixes the sharp view and `blurred` into `out`. `params`: focus distance, the distance differences are
    /// measured against, strength.
    pub fn composite(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        blurred: &wgpu::TextureView,
        out: &wgpu::TextureView,
        params: [f32; 4],
    ) {
        let Some(t) = &self.target else { return };
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dof"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.sharp) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(blurred) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&t.distance) },
                wgpu::BindGroupEntry { binding: 4, resource: self.params.as_entire_binding() },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dof"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
