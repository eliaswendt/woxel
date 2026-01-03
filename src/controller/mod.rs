// CONTROLLER: Input, game logic, and update loop
pub mod input;
pub mod physics;
pub mod camera_controller;
pub mod frame_loop;

pub use input::{InputState, InputProcessor};
pub use physics::PhysicsSystem;
pub use camera_controller::{CameraController, GameState};
pub use frame_loop::{FrameLoopContext, CameraUniform, LightingUniform, TransformUniform};
