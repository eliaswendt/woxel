use std::rc::Rc;
use std::cell::RefCell;
use web_sys::console::log_1;
use wgpu::{Device, Queue, Surface, TextureView};
use web_sys::Window;

use crate::camera::Camera;
use crate::camera_controller::{CameraController, GameState};
use crate::world::CHUNK_SIZE;
use crate::physics::PhysicsSystem;
use crate::input::InputState;
use crate::scene::Scene;
use crate::render::RenderState;
use crate::utils::WorldCoord;
use crate::ui;

/// Main game loop state and update logic
pub struct FrameLoopContext {
    pub cam: Rc<RefCell<Camera>>,
    pub cam_buf: wgpu::Buffer,
    pub cam_buf_data: Rc<RefCell<CameraUniform>>,
    pub lighting_buf: wgpu::Buffer,
    pub lighting_buf_data: Rc<RefCell<LightingUniform>>,
    pub depth_view_cell: Rc<RefCell<TextureView>>,
    pub core: Rc<RefCell<Scene>>,
    pub input_state: Rc<RefCell<InputState>>,
    pub game_state: Rc<RefCell<GameState>>,
    pub camera_controller: CameraController,
    pub physics_system: PhysicsSystem,
    pub raycast_target: Rc<RefCell<Option<(i32, i32, i32)>>>,
    pub outline_transform: Rc<RefCell<TransformUniform>>,
    pub outline_buf: wgpu::Buffer,
    pub egui_ctx: egui::Context,
    pub egui_events: Rc<RefCell<Vec<egui::Event>>>,
    pub last_time: Rc<RefCell<f64>>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightingUniform {
    pub sun_dir: [f32; 3],
    pub sun_intensity: f32,
    pub ambient: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub _pad3: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformUniform {
    pub transform: [[f32; 4]; 4],
}

impl FrameLoopContext {
    /// Update game state, physics, camera, and load chunks for one frame
    pub fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        window: &Window,
        surface: &Surface,
        render_state: &mut RenderState,
    ) {
        // Time step
        let now = window.performance().map(|p| p.now()).unwrap_or(0.0);
        let mut last = self.last_time.borrow_mut();
        let dt = ((now - *last) / 1000.0).clamp(0.0, 0.1) as f32;
        *last = now;
        drop(last);

        // Consume look input before taking immutable borrow
        let (dx, dy) = self.input_state.borrow_mut().consume_look();

        // Extract input data in a minimal scope
        let (pressed_keys, is_control) = {
            let input = self.input_state.borrow();
            (
                input.pressed_keys.clone(),
                input.is_key_pressed("Control") || input.is_key_pressed("control"),
            )
        };

        let mut game = self.game_state.borrow_mut();

        // Apply mouse look (always)
        self.camera_controller
            .apply_look(&mut self.cam.borrow_mut(), dx, dy);

        // Sync player orientation with camera if following
        if game.camera_follows_player {
            let c = self.cam.borrow();
            game.player_yaw = c.yaw;
            game.player_pitch = c.pitch;
        }

        // Update camera position (WASD, Space, Shift always control camera)
        self.camera_controller
            .update_movement(&mut self.cam.borrow_mut(), &pressed_keys, dt, is_control);

        // Sync player position with camera if following
        if game.camera_follows_player {
            let c = self.cam.borrow();
            game.player_pos = self.camera_controller.sync_player_from_camera(&c);
        }

        // Player physics (only in player active mode)
        if game.player_active && game.camera_follows_player {
            let mut pos = game.player_pos;
            let mut vel = game.player_vel;
            self.physics_system.update(&mut pos, &mut vel, &pressed_keys, &self.core.borrow(), dt);
            game.player_pos = pos;
            game.player_vel = vel;

            // Update camera to match player after physics
            self.camera_controller
                .sync_camera_from_player(&mut self.cam.borrow_mut(), game.player_pos);
        }

        // Update chunks based on player position
        let p_pos = game.player_pos;
        drop(game); // Release game_state borrow

        self.core.borrow_mut().update(
            &WorldCoord(p_pos.x as isize, p_pos.y as isize, p_pos.z as isize),
            device,
            100
        );

        // Resize handling
        self.handle_resize(window, device, surface, render_state);

        // Update camera uniform
        self.cam_buf_data.borrow_mut().view_proj =
            self.cam.borrow().view_proj().to_cols_array_2d();
        queue.write_buffer(&self.cam_buf, 0, bytemuck::bytes_of(&*self.cam_buf_data.borrow()));

        // Update sun position relative to player
        let player_eye = self.cam.borrow().eye;
        let sun_offset = glam::Vec3::new(50.0, 100.0, 50.0);
        let sun_pos = player_eye + sun_offset;
        let sun_dir = (sun_pos - player_eye).normalize();
        self.lighting_buf_data.borrow_mut().sun_dir = [sun_dir.x, sun_dir.y, sun_dir.z];
        queue.write_buffer(&self.lighting_buf, 0, bytemuck::bytes_of(&*self.lighting_buf_data.borrow()));

        // Raycast to find block under crosshair
        let raycast_result = self.cam.borrow().raycast(8.0, |x, y, z| {
            match self.core.borrow().get_block(&WorldCoord(x as isize, y as isize, z as isize)) {
                Some(b) => b.is_solid(),
                None => false,
            }
        });

        if let Some(((bx, by, bz), (face_nx, face_ny, face_nz))) = raycast_result {
            *self.raycast_target.borrow_mut() = Some((bx, by, bz));
            let outline_transform_mat =
                glam::Mat4::from_translation(glam::Vec3::new(bx as f32, by as f32, bz as f32));
            self.outline_transform.borrow_mut().transform = outline_transform_mat.to_cols_array_2d();
            queue.write_buffer(
                &self.outline_buf,
                0,
                bytemuck::bytes_of(&*self.outline_transform.borrow()),
            );
            render_state.show_outline = true;

            // Handle block removal (left click) and placement (right click)
            let input = self.input_state.borrow();
            if input.left_click {
                log_1(&format!("trying to remove block at ({}, {}, {})", bx, by, bz).into());
                // Remove block: delete the hit block
                if self.core.borrow_mut().set_block(
                    &WorldCoord(bx as isize, by as isize, bz as isize),
                    crate::world::Block::Empty,
                    true,
                    device
                ) {
                    log_1(&"removed block".into());
                    // Successfully removed block, reload chunk
                    // Calculate chunk world key: (chunk_index * chunk_size)

                    // TODO: Implement mesh update for block changes
                    // For now, the mesh will be regenerated when the player moves to a new chunk
                }
            } else if input.right_click {
                // Place block: calculate position adjacent to the raycast target using the face normal
                let placement_x = bx + face_nx;
                let placement_y = by + face_ny;
                let placement_z = bz + face_nz;
                
                if self.core.borrow_mut().set_block(
                    &WorldCoord(placement_x as isize, placement_y as isize, placement_z as isize),
                    input.selected_block,
                    true,
                    device
                ) {
                    log_1(&format!("set block to {:?}", input.selected_block).into());
                    // Successfully placed block
                    // TODO: Implement mesh update for block changes
                    // For now, the mesh will be regenerated when the player moves to a new chunk
                }
            }
            drop(input);
        } else {
            *self.raycast_target.borrow_mut() = None;
            render_state.show_outline = false;
        }

        // Update render state with game state data
        {
            let game = self.game_state.borrow();
            render_state.player_pos = game.player_pos;
            render_state.camera_yaw = game.player_yaw;
            render_state.camera_pitch = game.player_pitch;
            render_state.wireframe_mode = self.input_state.borrow().wireframe_mode;
            render_state.show_chunk_borders = self.input_state.borrow().show_chunk_borders;
        }

        // Build egui input from queued events
        let dpr = window.device_pixel_ratio() as f32;
        let mut raw_input = egui::RawInput::default();
        raw_input.time = Some(now as f64 / 1000.0);
        raw_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::new(0.0, 0.0),
            egui::vec2(
                render_state.width as f32 / dpr,
                render_state.height as f32 / dpr,
            ),
        ));
        raw_input.events.extend(self.egui_events.borrow_mut().drain(..));

        // Set DPI scale for egui
        self.egui_ctx.set_pixels_per_point(dpr);

        // Build UI and store output for rendering
        let mut full_output = ui::build_ui(
            &self.egui_ctx,
            &self.cam,
            &self.game_state,
            &self.input_state,
            &self.core,
            render_state.width,
            render_state.height,
            dt,
            now,
        );

        // Tessellate and store for rendering in next step
        let dpr = window.device_pixel_ratio() as f32;
        let primitives = self.egui_ctx.tessellate(std::mem::take(&mut full_output.shapes), dpr);
        render_state.egui_primitives = Some(primitives);
        render_state.egui_full_output = Some(full_output);
        render_state.egui_dpr = dpr;
    }

    fn handle_resize(
        &self,
        window: &Window,
        device: &Device,
        surface: &Surface,
        render_state: &mut RenderState,
    ) {
        if let (Ok(w), Ok(h)) = (window.inner_width(), window.inner_height()) {
            let nw = w.as_f64().unwrap_or(800.0) as u32;
            let nh = h.as_f64().unwrap_or(600.0) as u32;
            if nw != render_state.width || nh != render_state.height {
                self.cam.borrow_mut().set_aspect(nw, nh);
                render_state.width = nw;
                render_state.height = nh;
                render_state.camera_aspect = nw as f32 / nh as f32;

                let config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: render_state.format,
                    width: nw,
                    height: nh,
                    present_mode: wgpu::PresentMode::Fifo,
                    alpha_mode: render_state.alpha_mode,
                    view_formats: vec![],
                    desired_maximum_frame_latency: 2,
                };
                surface.configure(device, &config);

                // Recreate depth texture & view to match new size
                let new_depth = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("depth"),
                    size: wgpu::Extent3d {
                        width: nw,
                        height: nh,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                *self.depth_view_cell.borrow_mut() =
                    new_depth.create_view(&wgpu::TextureViewDescriptor::default());
            }
        }
    }
}
