use glam::Vec3;
use crate::model::Camera;

/// Player/Game state - position, velocity, orientation
pub struct GameState {
    pub player_pos: Vec3,
    pub player_vel: Vec3,
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub player_active: bool,
    pub camera_follows_player: bool,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            player_pos: Vec3::new(8.0, 80.0, 8.0),
            player_vel: Vec3::ZERO,
            player_yaw: 0.0,
            player_pitch: 0.0,
            player_active: false,
            camera_follows_player: true,
        }
    }

    pub fn toggle_camera_follow(&mut self) {
        self.camera_follows_player = !self.camera_follows_player;
    }

    pub fn toggle_player_mode(&mut self) {
        self.player_active = !self.player_active;
        if self.player_active {
            self.player_vel = Vec3::ZERO;
        }
    }
}

/// Handles camera movement and orientation
pub struct CameraController {
    pub move_speed: f32,
    pub mouse_sensitivity: f32,
}

impl CameraController {
    pub fn new() -> Self {
        Self {
            move_speed: 10.0,
            mouse_sensitivity: 0.002,
        }
    }

    /// Apply mouse look delta to camera
    pub fn apply_look(&self, camera: &mut Camera, dx: f32, dy: f32) {
        camera.yaw += dx * self.mouse_sensitivity;
        let pi_half = std::f32::consts::PI / 2.0;
        camera.pitch = (camera.pitch - dy * self.mouse_sensitivity).clamp(-pi_half, pi_half);
    }

    /// Update camera position based on pressed keys
    pub fn update_movement(
        &self,
        camera: &mut Camera,
        pressed: &std::collections::HashSet<String>,
        dt: f32,
        speed_boost: bool,
    ) {
        let mut cam_move = Vec3::ZERO;
        let mut speed = self.move_speed * dt;

        if speed_boost {
            speed *= 20.0;
        }

        if pressed.contains("w") || pressed.contains("W") {
            cam_move += camera.forward();
        }
        if pressed.contains("s") || pressed.contains("S") {
            cam_move -= camera.forward();
        }

        let cam_right = camera.forward().cross(camera.up).normalize();
        if pressed.contains("a") || pressed.contains("A") {
            cam_move -= cam_right;
        }
        if pressed.contains("d") || pressed.contains("D") {
            cam_move += cam_right;
        }

        if pressed.contains(" ") {
            cam_move += Vec3::Y;
        }
        if pressed.contains("Shift") {
            cam_move -= Vec3::Y;
        }

        if cam_move.length_squared() > 0.0 {
            camera.eye += cam_move.normalize() * speed;
        }
    }

    /// Sync player position from camera (for free-cam mode)
    pub fn sync_player_from_camera(&self, camera: &Camera) -> Vec3 {
        camera.eye - Vec3::new(0.0, 1.6, 0.0)
    }

    /// Sync camera from player position (for player mode)
    pub fn sync_camera_from_player(&self, camera: &mut Camera, player_pos: Vec3) {
        camera.eye = player_pos + Vec3::new(0.0, 1.6, 0.0);
    }
}
