use glam::{Mat4, Vec3};

pub struct Camera {
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub up: Vec3,
    pub fov_y: f32,
    pub aspect: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Camera {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            eye: Vec3::new(16.0, 16.0, 40.0),
            yaw: 0.0,
            pitch: 0.0,
            up: Vec3::Y,
            fov_y: 60f32.to_radians(),
            aspect: width as f32 / height as f32,
            z_near: 0.1,
            z_far: 1000.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        let cy = self.yaw;
        let cp = self.pitch.clamp(-1.5533, 1.5533); // Slightly less than π/2 to avoid gimbal lock
        Vec3::new(cy.cos() * cp.cos(), cp.sin(), cy.sin() * cp.cos()).normalize()
    }

    pub fn target(&self) -> Vec3 { self.eye + self.forward() }

    pub fn set_aspect(&mut self, width: u32, height: u32) { self.aspect = width as f32 / height as f32; }

    pub fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target(), self.up);
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.z_near, self.z_far);
        proj * view
    }

    pub fn set_look_at(&mut self, target: Vec3) {
        let dir = (target - self.eye).normalize();
        self.yaw = dir.z.atan2(dir.x);
        self.pitch = dir.y.asin().clamp(-1.4, 1.4);
    }
    
    // DDA raycast to find block intersection
    // Returns (block_pos, face_normal) or None if no hit within max_distance
    pub fn raycast<F>(&self, max_distance: f32, is_solid: F) -> Option<((i32, i32, i32), (i32, i32, i32))>
    where
        F: Fn(i32, i32, i32) -> bool,
    {
        let dir = self.forward();
        let mut pos = self.eye;
        
        let step_size = 0.1;
        let mut distance = 0.0;
        let mut last_air_block = (pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32);
        
        while distance < max_distance {
            pos += dir * step_size;
            distance += step_size;
            
            let block_x = pos.x.floor() as i32;
            let block_y = pos.y.floor() as i32;
            let block_z = pos.z.floor() as i32;
            
            if is_solid(block_x, block_y, block_z) {
                // Found a solid block, return it
                // Compute face normal based on which coordinate changed
                let (prev_x, prev_y, prev_z) = last_air_block;
                let face_normal = (
                    if block_x != prev_x { (block_x - prev_x).signum() } else { 0 },
                    if block_y != prev_y { (block_y - prev_y).signum() } else { 0 },
                    if block_z != prev_z { (block_z - prev_z).signum() } else { 0 },
                );
                return Some(((block_x, block_y, block_z), face_normal));
            }
            
            last_air_block = (block_x, block_y, block_z);
        }
        
        None
    }

    // Extract frustum planes from view-projection matrix for culling
    // Returns 6 planes (left, right, bottom, top, near, far) as [a,b,c,d] where ax+by+cz+d=0
    pub fn frustum_planes(eye: Vec3, yaw: f32, pitch: f32, aspect: f32, fov_y: f32, z_near: f32, z_far: f32) -> [[f32; 4]; 6] {
        let forward = {
            let cy = yaw;
            let cp = pitch.clamp(-1.5533, 1.5533);
            Vec3::new(cy.cos() * cp.cos(), cp.sin(), cy.sin() * cp.cos()).normalize()
        };
        let target = eye + forward;
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        let proj = Mat4::perspective_rh(fov_y, aspect, z_near, z_far);
        let vp = proj * view;
        let m = vp.to_cols_array();
        
        [
            // Left: row4 + row1
            [m[3] + m[0], m[7] + m[4], m[11] + m[8], m[15] + m[12]],
            // Right: row4 - row1
            [m[3] - m[0], m[7] - m[4], m[11] - m[8], m[15] - m[12]],
            // Bottom: row4 + row2
            [m[3] + m[1], m[7] + m[5], m[11] + m[9], m[15] + m[13]],
            // Top: row4 - row2
            [m[3] - m[1], m[7] - m[5], m[11] - m[9], m[15] - m[13]],
            // Near: row4 + row3
            [m[3] + m[2], m[7] + m[6], m[11] + m[10], m[15] + m[14]],
            // Far: row4 - row3
            [m[3] - m[2], m[7] - m[6], m[11] - m[10], m[15] - m[14]],
        ]
    }

    // Test if AABB (chunk bounding box) intersects frustum
    pub fn is_chunk_in_frustum(eye: Vec3, yaw: f32, pitch: f32, aspect: f32, fov_y: f32, z_near: f32, z_far: f32, cx: i32, cy: i32, cz: i32, chunk_size: f32) -> bool {
        let planes = Self::frustum_planes(eye, yaw, pitch, aspect, fov_y, z_near, z_far);
        let min_x = cx as f32 * chunk_size;
        let min_y = cy as f32 * chunk_size;
        let min_z = cz as f32 * chunk_size;
        let max_x = min_x + chunk_size;
        let max_y = min_y + chunk_size;
        let max_z = min_z + chunk_size;

        for plane in &planes {
            let [a, b, c, d] = plane;
            // Test all 8 corners, if all are outside this plane, cull the chunk
            let mut all_outside = true;
            for corner in &[
                [min_x, min_y, min_z],
                [max_x, min_y, min_z],
                [min_x, max_y, min_z],
                [max_x, max_y, min_z],
                [min_x, min_y, max_z],
                [max_x, min_y, max_z],
                [min_x, max_y, max_z],
                [max_x, max_y, max_z],
            ] {
                let dist = a * corner[0] + b * corner[1] + c * corner[2] + d;
                if dist > 0.0 {
                    all_outside = false;
                    break;
                }
            }
            if all_outside {
                return false;
            }
        }
        true
    }
}
