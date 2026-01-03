use glam::Vec3;
use std::collections::HashSet;
use crate::utils::WorldCoord;
use crate::model::Scene;

/// Handles player physics (gravity, collision, jumping)
pub struct PhysicsSystem {
    pub gravity: f32,
    pub max_fall_speed: f32,
}

impl PhysicsSystem {
    pub fn new() -> Self {
        Self {
            gravity: -9.8,
            max_fall_speed: 20.0,
        }
    }

    /// Update player position and velocity with physics
    pub fn update(
        &self,
        pos: &mut Vec3,
        vel: &mut Vec3,
        pressed: &HashSet<String>,
        world: &Scene,
        dt: f32,
    ) {
        // Apply gravity
        vel.y += self.gravity * dt;
        vel.y = vel.y.clamp(-self.max_fall_speed, 20.0);

        // Apply velocity
        let new_pos = *pos + *vel * dt;

        // Check for ground (block below)
        let below_block = world
            .get_block(&WorldCoord(
                new_pos.x as isize,
                (new_pos.y - 1.2).max(0.0) as isize,
                new_pos.z as isize,
            ))
            .unwrap();
        let on_ground = below_block.is_solid();

        // Jump handling
        if (pressed.contains(" ") || pressed.contains("Space")) && on_ground {
            vel.y = 8.0;
        }

        // Vertical collision
        if below_block.is_solid() && new_pos.y < pos.y {
            pos.y = ((new_pos.y - 1.0).floor() + 1.5).max(pos.y);
            vel.y = 0.0;
        } else if !below_block.is_solid() && new_pos.y < pos.y {
            pos.y = new_pos.y;
        } else {
            pos.y = new_pos.y;
        }

        // Horizontal collision (simple axis-aligned)
        let check_block = |x: isize, y: isize, z: isize| {
            world
                .get_block(&WorldCoord(x, y, z))
                .unwrap()
                .is_solid()
        };

        let x_next = new_pos.x;
        if !check_block(x_next as isize, new_pos.y as isize, pos.z as isize)
            && !check_block(x_next as isize, (new_pos.y + 1.5) as isize, pos.z as isize)
        {
            pos.x = x_next;
        }

        let z_next = new_pos.z;
        if !check_block(pos.x as isize, new_pos.y as isize, z_next as isize)
            && !check_block(pos.x as isize, (new_pos.y + 1.5) as isize, z_next as isize)
        {
            pos.z = z_next;
        }

        // Clamp to world bounds
        pos.y = pos.y.max(0.1).min(254.0);
        pos.x = pos.x.max(-50.0).min(250.0);
        pos.z = pos.z.max(-50.0).min(250.0);
    }
}
