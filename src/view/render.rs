use wgpu::*;
use wgpu::util::DeviceExt;
use crate::model::Camera;
use crate::utils::{ChunkCoord, MeshBuffer, Vertex, create_outline_mesh};
use glam::Vec3;
use crate::model::scene::{ActiveEntry, MeshGenerationState};

// Shared graphics setup used by native and web
pub struct CameraResources {
    pub camera_buffer: wgpu::Buffer,
    pub lighting_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub camera_bind_group: wgpu::BindGroup,
}

pub struct PipelineResources {
    pub chunk_opaque: wgpu::RenderPipeline,
    pub chunk_transparent: wgpu::RenderPipeline,
}

pub struct OutlineResources {
    pub outline_pipeline: wgpu::RenderPipeline,
    pub outline_mesh_buffer: Option<MeshBuffer>,
    pub outline_buffer: wgpu::Buffer,
    pub outline_bind_group: wgpu::BindGroup,
}

pub fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth_texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
    (depth_texture, depth_view)
}


pub fn create_camera_resources(device: &wgpu::Device) -> CameraResources {
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera_buffer"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let lighting_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lighting_buffer"),
        size: 32,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
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

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera_bind_group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: lighting_buffer.as_entire_binding() },
        ],
    });

    CameraResources { camera_buffer, lighting_buffer, bind_group_layout, camera_bind_group }
}


pub fn create_chunk_pipelines(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    depth_format: wgpu::TextureFormat,
) -> PipelineResources {
    let shader_src = include_str!("shaders/chunk.wgsl");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("chunk_shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline_layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    let chunk_opaque = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("chunk_opaque"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                    wgpu::VertexAttribute { offset: 40, shader_location: 3, format: wgpu::VertexFormat::Float32x2 },
                ],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
        multiview: None,
        cache: None,
    });


    // same as opaque but with depth_write_enabled: false
    let chunk_transparent = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("chunk_transparent"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                    wgpu::VertexAttribute { offset: 40, shader_location: 3, format: wgpu::VertexFormat::Float32x2 },
                ],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
        multiview: None,
        cache: None,
    });

    PipelineResources { chunk_opaque, chunk_transparent }
}

pub fn create_outline_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    _camera_bind_group_layout: &wgpu::BindGroupLayout,
    camera_buffer: &wgpu::Buffer,
    depth_format: wgpu::TextureFormat,
) -> OutlineResources {
    let outline_mesh_buffer = Some(create_outline_mesh(1.0).upload(device));

    let outline_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("outline_transform"),
        contents: bytemuck::cast_slice(&glam::Mat4::IDENTITY.to_cols_array_2d()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let outline_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("outline_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
        ],
    });

    let outline_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("outline_bg"),
        layout: &outline_bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: outline_buffer.as_entire_binding() },
        ],
    });

    let outline_shader_src = include_str!("shaders/outline.wgsl");
    let outline_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("outline_shader"),
        source: wgpu::ShaderSource::Wgsl(outline_shader_src.into()),
    });

    let outline_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("outline_pipeline_layout"),
        bind_group_layouts: &[&outline_bgl],
        push_constant_ranges: &[],
    });

    let outline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("outline_pipeline"),
        layout: Some(&outline_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &outline_shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                    wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                ],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &outline_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
        multiview: None,
        cache: None,
    });

    OutlineResources { outline_pipeline, outline_mesh_buffer, outline_buffer, outline_bind_group }
}

///////////////////////////////////////////////////////////////////////////////

/// Consolidated render state to avoid parameter explosion
pub struct RenderState {
    // wgpu resources
    pub format: TextureFormat,
    pub alpha_mode: CompositeAlphaMode,
    pub width: u32,
    pub height: u32,
    
    // Pipelines
    pub chunk_opaque: RenderPipeline,
    pub chunk_transparent: RenderPipeline,
    pub outline_pipeline: RenderPipeline,
    
    // Meshes
    pub outline_mesh: MeshBuffer,
    pub show_outline: bool,
    pub chunk_border_mesh: MeshBuffer,
    pub show_chunk_borders: bool,
    
    // Camera state
    pub player_pos: Vec3,
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub camera_aspect: f32,
    pub camera_fov_y: f32,
    pub camera_z_near: f32,
    pub camera_z_far: f32,
    
    // UI
    pub egui_renderer: egui_wgpu::Renderer,
    pub egui_primitives: Option<Vec<egui::ClippedPrimitive>>,
    pub egui_full_output: Option<egui::FullOutput>,
    pub egui_dpr: f32,
    pub wireframe_mode: bool,
}

impl RenderState {
    pub fn draw_frame(
        &mut self,
        device: &Device,
        queue: &Queue,
        surface: &Surface,
        scene_chunks: &Vec<ActiveEntry>,
        depth_view: &TextureView,
        cam_bg: &BindGroup,
        outline_bg: &BindGroup,
        chunk_size: f32,
    ) {
        // Pre-compute frustum planes once for culling
        let frustum_planes = Camera::frustum_planes(
            self.player_pos,
            self.camera_yaw,
            self.camera_pitch,
            self.camera_aspect,
            self.camera_fov_y,
            self.camera_z_near,
            self.camera_z_far,
        );

        let (egui_primitives, egui_full_output) = match (self.egui_primitives.take(), self.egui_full_output.take()) {
            (Some(prim), Some(output)) => (prim, output),
            _ => return, // No UI to render
        };

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.width, self.height],
            pixels_per_point: self.egui_dpr,
        };

        let frame = match surface.get_current_texture() {
            Ok(frame) => frame,
            Err(SurfaceError::Lost) => {
                surface.configure(
                    device,
                    &SurfaceConfiguration {
                        usage: TextureUsages::RENDER_ATTACHMENT,
                        format: self.format,
                        width: self.width,
                        height: self.height,
                        present_mode: PresentMode::Fifo,
                        alpha_mode: self.alpha_mode,
                        view_formats: vec![],
                        desired_maximum_frame_latency: 2,
                    },
                );
                surface
                    .get_current_texture()
                    .expect("Failed to acquire frame after reconfigure")
            }
            Err(e) => panic!("Surface error: {e:?}"),
        };

        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("encoder"),
        });

        {
            let mut rp = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.5,
                            g: 0.8,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });


            
            // Render block outline
            if self.show_outline {
                rp.set_pipeline(&self.outline_pipeline);
                rp.set_bind_group(0, outline_bg, &[]);
                rp.set_vertex_buffer(0, self.outline_mesh.vertex_buffer.slice(..));
                rp.set_index_buffer(self.outline_mesh.index_buffer.slice(..), IndexFormat::Uint32);
                rp.draw_indexed(0..self.outline_mesh.index_count, 0, 0..1);
            }
            
            
            // use chunk_opaque pipeline to render chunk borders, outline, and opaque chunks
            rp.set_pipeline(&self.chunk_opaque);
            rp.set_bind_group(0, cam_bg, &[]);
            // DRAW CHUNKS with frustum culling
            for entry in scene_chunks.iter() {
                // Render mesh if this chunk has one
                if let ActiveEntry::Loaded { coord: chunk_coord, mesh: MeshGenerationState::Completed { buffer: mesh_buffer, .. }, .. } = entry {
                    if mesh_buffer.0.index_count == 0 {
                        continue; // Skip empty meshes
                    }
                    
                    // Frustum culling - skip chunks outside view
                    if Self::is_chunk_visible(&frustum_planes, chunk_coord, chunk_size) {
                        rp.set_vertex_buffer(0, mesh_buffer.0.vertex_buffer.slice(..));
                        rp.set_index_buffer(mesh_buffer.0.index_buffer.slice(..), IndexFormat::Uint32);
                        rp.draw_indexed(0..mesh_buffer.0.index_count, 0, 0..1);
                    }
                    
                }
            }

            // use chunk_transparent pipeline to render transparent chunks
            rp.set_pipeline(&self.chunk_transparent);

            // DRAW CHUNKS with frustum culling
            for entry in scene_chunks.iter() {
                // Render mesh if this chunk has one
                if let ActiveEntry::Loaded { coord: chunk_coord, mesh: MeshGenerationState::Completed { buffer: mesh_buffer, .. }, .. } = entry {
                    if mesh_buffer.1.index_count == 0 {
                        continue; // Skip empty meshes
                    }
                    
                    // Frustum culling - skip chunks outside view
                    if Self::is_chunk_visible(&frustum_planes, chunk_coord, chunk_size) {
                        rp.set_vertex_buffer(0, mesh_buffer.1.vertex_buffer.slice(..));
                        rp.set_index_buffer(mesh_buffer.1.index_buffer.slice(..), IndexFormat::Uint32);
                        rp.draw_indexed(0..mesh_buffer.1.index_count, 0, 0..1);
                    }
                    
                }
            }


        }

        // Upload egui textures
        for (id, image_delta) in &egui_full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(device, queue, *id, image_delta);
        }

        // Update egui buffers
        self.egui_renderer
            .update_buffers(device, queue, &mut encoder, &egui_primitives, &screen_descriptor);

        // Render egui overlay
        {
            let egui_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("egui_render_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.egui_renderer
                .render(&mut egui_pass.forget_lifetime(), &egui_primitives, &screen_descriptor);
        }

        // Free egui textures
        for id in &egui_full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }

    /// Fast frustum culling check for a chunk AABB
    /// Uses pre-computed frustum planes to avoid recalculating per-chunk
    #[inline]
    fn is_chunk_visible(planes: &[[f32; 4]; 6], coord: &ChunkCoord, chunk_size: f32) -> bool {
        let min_x = coord.0 as f32 * chunk_size;
        let min_y = coord.1 as f32 * chunk_size;
        let min_z = coord.2 as f32 * chunk_size;
        let max_x = min_x + chunk_size;
        let max_y = min_y + chunk_size;
        let max_z = min_z + chunk_size;

        // Test against each frustum plane
        for plane in planes {
            let [a, b, c, d] = *plane;
            
            // Find the corner most in the direction of the plane normal (p-vertex)
            // If this corner is outside, the whole AABB is outside
            let px = if a >= 0.0 { max_x } else { min_x };
            let py = if b >= 0.0 { max_y } else { min_y };
            let pz = if c >= 0.0 { max_z } else { min_z };
            
            // If p-vertex is outside this plane, chunk is culled
            if a * px + b * py + c * pz + d < 0.0 {
                return false;
            }
        }
        true
    }
}
