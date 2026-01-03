use winit::{
    event::*,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};
use wgpu::util::DeviceExt;
use glam::Vec3;
use std::collections::HashSet;
use std::sync::Arc;

// Import from the library crate
use woxel::{
    logging, utils, ui,
    model, view, controller,
};

use model::Camera;
use model::Block;
use model::Scene;
use controller::{GameState, CameraController};
use controller::{InputState, InputProcessor};
use controller::PhysicsSystem;
use controller::{CameraUniform, LightingUniform, TransformUniform};

use model::CHUNK_SIZE;

struct App {
    // Core GPU resources
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,
    
    // Rendering state
    pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: Option<wgpu::RenderPipeline>,
    outline_pipeline: wgpu::RenderPipeline,
    outline_mesh: utils::MeshBuffer,
    outline_buffer: wgpu::Buffer,
    outline_bind_group: wgpu::BindGroup,
    chunk_border_mesh: utils::MeshBuffer,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    lighting_buffer: wgpu::Buffer,
    
    // egui
    egui_renderer: egui_wgpu::Renderer,
    egui_state: egui_winit::State,
    egui_ctx: egui::Context,
    
    // Game state
    camera: Camera,
    game_state: GameState,
    input_state: InputState,
    camera_controller: CameraController,
    physics_system: PhysicsSystem,
    core: Scene,
    raycast_target: Option<(i32, i32, i32)>,
    
    // Input handling
    pressed_keys: HashSet<KeyCode>,
    mouse_locked: bool,
    last_mouse_pos: Option<(f64, f64)>,
    wireframe_mode: bool,
    show_chunk_borders: bool,
    
    // Frame timing
    last_frame_time: std::time::Instant,
    fps: f32,
    frame_count: u32,
    fps_timer: f32,
}

impl App {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        
        // Initialize wgpu
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        
        let surface = instance.create_surface(window.clone()).unwrap();
        let gpu = gpu_init::GpuContext::new_native(surface, size.width, size.height).await;
        
        let device = gpu.device.clone();
        let queue = gpu.queue.clone();
        let config = gpu.config.clone();
        
        // Create depth texture
        let depth_format = wgpu::TextureFormat::Depth32Float;
        let (depth_texture, depth_view) = render::create_depth_texture(&device, size.width, size.height);
        
        // Create camera
        let mut camera = Camera::new(size.width, size.height);
        camera.eye = Vec3::new(16.0, 40.0, 16.0);
        camera.set_look_at(Vec3::new(16.0, 40.0, 25.0));
        
        // Camera, lighting buffers & bind groups - use unified function
        let camera_resources = render::create_camera_resources(&device);
        let camera_buffer = camera_resources.camera_buffer;
        let lighting_buffer = camera_resources.lighting_buffer;
        let camera_bgl = camera_resources.bind_group_layout;
        let camera_bind_group = camera_resources.camera_bind_group;
        
        // Note: we need to reinit these with actual data
        let cam_buf_data = frame_loop::CameraUniform {
            view_proj: camera.view_proj().to_cols_array_2d(),
        };
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&cam_buf_data));
        
        let lighting_buf_data = frame_loop::LightingUniform {
            sun_dir: [0.5, -1.0, 0.3],
            sun_intensity: 1.0,
            ambient: 0.35,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
        };
        queue.write_buffer(&lighting_buffer, 0, bytemuck::bytes_of(&lighting_buf_data));
        
        // Create chunk pipelines
        let pipes = render::create_chunk_pipelines(&device, config.format, &camera_bgl, depth_format);
        let pipeline = pipes.pipeline;
        let wireframe_pipeline = pipes.wireframe_pipeline;
        
        // Outline resources
        let outline_res = render::create_outline_resources(&device, config.format, &camera_bgl, &camera_buffer, depth_format);
        let outline_mesh = outline_res.outline_mesh_buffer.unwrap();
        let outline_buffer = outline_res.outline_buffer;
        let outline_bind_group = outline_res.outline_bind_group;
        let outline_pipeline = outline_res.outline_pipeline;
        
        // Create chunk border mesh
        let chunk_border_mesh = utils::create_chunk_border_mesh(16).upload_to_gpu(&device);
        
        // Initialize egui
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );
        
        // Initialize game systems
        let core = Scene::new([64, 64, 64], &device);
        let game_state = GameState::new();
        let input_state = InputState::new();
        let camera_controller = CameraController::new();
        let physics_system = PhysicsSystem::new();
        
        Self {
            surface: gpu.surface,
            device,
            queue,
            config,
            size,
            window,
            pipeline,
            wireframe_pipeline,
            outline_pipeline,
            outline_mesh,
            outline_buffer,
            outline_bind_group,
            chunk_border_mesh,
            depth_texture,
            depth_view,
            camera_buffer,
            camera_bind_group,
            lighting_buffer,
            egui_renderer,
            egui_state,
            egui_ctx,
            camera,
            game_state,
            input_state,
            camera_controller,
            physics_system,
            core,
            raycast_target: None,
            pressed_keys: HashSet::new(),
            mouse_locked: false,
            last_mouse_pos: None,
            wireframe_mode: false,
            show_chunk_borders: false,
            last_frame_time: std::time::Instant::now(),
            fps: 0.0,
            frame_count: 0,
            fps_timer: 0.0,
        }
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        // First let egui process the event
        let egui_captured = self.egui_state.on_window_event(self.window.as_ref(), event).consumed;
        if egui_captured {
            return true;
        }

        match event {
            WindowEvent::KeyboardInput { event: KeyEvent { state, physical_key, .. }, .. } => {
                if let PhysicalKey::Code(code) = physical_key {
                    match state {
                        ElementState::Pressed => {
                            self.pressed_keys.insert(*code);
                            
                            // Toggle wireframe on Q
                            if *code == KeyCode::KeyQ {
                                self.wireframe_mode = !self.wireframe_mode;
                            }
                            // Toggle chunk borders on B
                            if *code == KeyCode::KeyB {
                                self.show_chunk_borders = !self.show_chunk_borders;
                            }
                            // Toggle camera follow on C
                            if *code == KeyCode::KeyC {
                                self.game_state.toggle_camera_follow();
                            }
                            // Unlock mouse on Escape
                            if *code == KeyCode::Escape {
                                self.mouse_locked = false;
                                let _ = self.window.set_cursor_visible(true);
                                let _ = self.window.set_cursor_grab(winit::window::CursorGrabMode::None);
                            }
                        }
                        ElementState::Released => {
                            self.pressed_keys.remove(code);
                        }
                    }
                }
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                match state {
                    ElementState::Pressed => {
                        if *button == MouseButton::Left {
                            self.mouse_locked = true;
                            let _ = self.window.set_cursor_visible(false);
                            let _ = self.window.set_cursor_grab(winit::window::CursorGrabMode::Locked);
                        }
                    }
                    ElementState::Released => {}
                }
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.mouse_locked {
                    if let Some((lx, ly)) = self.last_mouse_pos {
                        let dx = position.x - lx;
                        let dy = position.y - ly;
                        let sens = 0.002;
                        self.camera.yaw += dx as f32 * sens;
                        let pi_half = std::f32::consts::PI / 2.0;
                        self.camera.pitch = (self.camera.pitch - dy as f32 * sens).clamp(-pi_half, pi_half);
                    }
                    self.last_mouse_pos = Some((position.x, position.y));
                }
                true
            }
            _ => false,
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            
            let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width: new_size.width,
                    height: new_size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
            
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;
            self.camera.set_aspect(new_size.width, new_size.height);
        }
    }

    fn handle_mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.mouse_locked {
            let sens = 0.002;
            self.camera.yaw += dx as f32 * sens;
            let pi_half = std::f32::consts::PI / 2.0;
            self.camera.pitch = (self.camera.pitch - dy as f32 * sens).clamp(-pi_half, pi_half);
        }
    }
    
    fn update(&mut self, dt: f32) {
        // Update FPS
        self.frame_count += 1;
        self.fps_timer += dt;
        if self.fps_timer >= 1.0 {
            self.fps = self.frame_count as f32 / self.fps_timer;
            self.frame_count = 0;
            self.fps_timer = 0.0;
        }
        
        // Camera movement from input
        let mut speed = 10.0 * dt;
        if self.pressed_keys.contains(&KeyCode::ControlLeft) || self.pressed_keys.contains(&KeyCode::ControlRight) {
            speed *= 10.0;
        }
        
        let mut movement = Vec3::ZERO;
        if self.pressed_keys.contains(&KeyCode::KeyW) {
            movement += self.camera.forward();
        }
        if self.pressed_keys.contains(&KeyCode::KeyS) {
            movement -= self.camera.forward();
        }
        if self.pressed_keys.contains(&KeyCode::KeyA) {
            let right = self.camera.forward().cross(self.camera.up).normalize();
            movement -= right;
        }
        if self.pressed_keys.contains(&KeyCode::KeyD) {
            let right = self.camera.forward().cross(self.camera.up).normalize();
            movement += right;
        }
        if self.pressed_keys.contains(&KeyCode::Space) {
            movement += Vec3::Y;
        }
        if self.pressed_keys.contains(&KeyCode::ShiftLeft) {
            movement -= Vec3::Y;
        }
        
        if movement.length_squared() > 0.0 {
            self.camera.eye += movement.normalize() * speed;
        }
        
        // Update chunks around camera position
        let camera_coord = utils::WorldCoord(
            self.camera.eye.x as isize,
            self.camera.eye.y as isize,
            self.camera.eye.z as isize,
        );
        self.core.update(&camera_coord, &self.device, 500);
        
        // Update camera buffer
        let view_proj = self.camera.view_proj();
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(view_proj.as_ref()));
    }
    
    fn render_ui(&mut self) -> (Vec<egui::epaint::ClippedShape>, egui::TexturesDelta) {
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let output = self.egui_ctx.run(raw_input, |ctx| {
            // Debug info panel
            egui::Window::new("Debug")
                .default_pos([8.0, 8.0])
                .default_size([140.0, 100.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(format!("FPS: {:.0}", self.fps)).small());
                    let px = self.camera.eye.x;
                    let py = self.camera.eye.y;
                    let pz = self.camera.eye.z;
                    let cx = (px / 8.0).floor() as i32;
                    let cy = (py / 8.0).floor() as i32;
                    let cz = (pz / 8.0).floor() as i32;
                    ui.label(egui::RichText::new(format!("Pos: {:.1}, {:.1}, {:.1}", px, py, pz)).small());
                    ui.label(egui::RichText::new(format!("Chunk: {}, {}, {}", cx, cy, cz)).small());
                });

            // Settings (FOV)
            egui::Window::new("Settings")
                .default_pos([self.config.width as f32 - 140.0, 8.0])
                .default_size([130.0, 80.0])
                .show(ctx, |ui| {
                    let mut fov_deg = self.camera.fov_y.to_degrees().clamp(30.0, 120.0);
                    ui.label(egui::RichText::new("FOV").small());
                    if ui.add(egui::Slider::new(&mut fov_deg, 30.0..=120.0).step_by(5.0)).changed() {
                        self.camera.fov_y = fov_deg.to_radians();
                    }
                });
        });
        
        self.egui_state.handle_platform_output(&self.window, output.platform_output);
        (output.shapes, output.textures_delta)
    }
    
    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        let (shapes, textures_delta) = self.render_ui();
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };
        let mut primitives = self.egui_ctx.tessellate(shapes, self.window.scale_factor() as f32);
        
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });
        
        // Upload egui textures
        for (id, image_delta) in &textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }
        self.egui_renderer.update_buffers(&self.device, &self.queue, &mut encoder, &primitives, &screen_descriptor);
        
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.8,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            
            // Use wireframe pipeline if enabled
            if self.wireframe_mode {
                if let Some(ref wf) = self.wireframe_pipeline {
                    render_pass.set_pipeline(wf);
                } else {
                    render_pass.set_pipeline(&self.pipeline);
                }
            } else {
                render_pass.set_pipeline(&self.pipeline);
            }
            
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            
            // Render visible chunks
            for x_chunks in &self.core.active {
                for y_chunks in x_chunks {
                    for chunk_entry in y_chunks {
                        let (_, mesh_buffer_opt) = chunk_entry;
                        if let Some((_, mesh)) = mesh_buffer_opt {
                            if mesh.index_count == 0 {
                                continue;
                            }
                            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                            render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                            render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                        }
                    }
                }
            }
            
            // Render chunk borders if enabled
            if self.show_chunk_borders {
                render_pass.set_pipeline(&self.pipeline);
                render_pass.set_vertex_buffer(0, self.chunk_border_mesh.vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.chunk_border_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                
                // Calculate current chunk based on player position
                let player_pos = self.game_state.player_pos;
                let player_chunk_x = (player_pos.x / 16.0).floor() as i32;
                let player_chunk_y = (player_pos.y / 16.0).floor() as i32;
                let player_chunk_z = (player_pos.z / 16.0).floor() as i32;
                
                // Only render border for current chunk
                // The border mesh is already at the current player chunk position
                render_pass.draw_indexed(0..self.chunk_border_mesh.index_count, 0, 0..1);
            }
        }
        
        // Render egui on top
        {
            let mut egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            
            self.egui_renderer.render(&mut egui_pass.forget_lifetime(), &primitives, &screen_descriptor);
        }
        
        // Cleanup egui textures
        for id in &textures_delta.free {
            self.egui_renderer.free_texture(&id);
        }
        
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        Ok(())
    }
}

fn main() {
    logging::init();
    
    let event_loop = EventLoop::new().unwrap();
    let window_attributes = Window::default_attributes()
        .with_title("WASM MC - Native")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
    let window = event_loop.create_window(window_attributes).unwrap();
    let window = Arc::new(window);
    
    let mut app = pollster::block_on(App::new(window.clone()));
    
    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == app.window.id() => {
                if !app.input(event) {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::Resized(physical_size) => {
                            app.resize(*physical_size);
                        }
                        WindowEvent::RedrawRequested => {
                            let now = std::time::Instant::now();
                            let dt = (now - app.last_frame_time).as_secs_f32();
                            app.last_frame_time = now;
                            
                            app.update(dt);
                            
                            match app.render() {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::Lost) => app.resize(app.size),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("{:?}", e),
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::DeviceEvent { event: winit::event::DeviceEvent::MouseMotion { delta }, .. } => {
                app.handle_mouse_motion(delta.0, delta.1);
            }
            Event::AboutToWait => {
                app.window.request_redraw();
            }
            _ => {}
        }
    }).unwrap();
}
