/// Platform-agnostic input handling system
use std::collections::HashSet;
use crate::model::Block;

/// Platform-independent input events
#[derive(Debug, Clone)]
pub enum InputEvent {
    // Keyboard events
    KeyDown(String),
    KeyUp(String),
    
    // Mouse events
    MouseMove { dx: f32, dy: f32 },
    MouseClick { button: MouseButton, is_down: bool, x: f32, y: f32 },
    MouseWheel { delta_y: f32 },
    
    // Window events
    FocusLost,
    VisibilityChanged { visible: bool },
    PointerLockChanged { locked: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub fn from_web_button(button: i16) -> Self {
        match button {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => MouseButton::Left,
        }
    }
}

/// Unified input state (replaces InputState in game_state.rs)
pub struct InputState {
    pub pressed_keys: HashSet<String>,
    pub look_delta: (f32, f32),
    pub pointer_locked: bool,
    pub selected_block: Block,
    pub wireframe_mode: bool,
    pub show_chunk_borders: bool,
    pub mouse_pos: (f32, f32),
    pub left_click: bool,
    pub right_click: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            pressed_keys: HashSet::new(),
            look_delta: (0.0, 0.0),
            pointer_locked: false,
            selected_block: Block::Grass,
            wireframe_mode: false,
            show_chunk_borders: false,
            mouse_pos: (0.0, 0.0),
            left_click: false,
            right_click: false,
        }
    }

    /// Process an input event and update state
    pub fn process_event(&mut self, event: &InputEvent) {
        match event {
            InputEvent::KeyDown(key) => {
                self.pressed_keys.insert(key.clone());
            }
            InputEvent::KeyUp(key) => {
                self.pressed_keys.remove(key.as_str());
            }
            InputEvent::MouseMove { dx, dy } => {
                if self.pointer_locked {
                    self.look_delta.0 += dx;
                    self.look_delta.1 += dy;
                }
                // Mouse position tracking could be added here
            }
            InputEvent::MouseClick { button, is_down, .. } => {
                match button {
                    MouseButton::Left => self.left_click = *is_down,
                    MouseButton::Right => self.right_click = *is_down,
                    _ => {},
                }
            }
            InputEvent::MouseWheel { delta_y } => {
                if *delta_y < 0.0 {
                    self.cycle_selected_block(false); // Up: previous
                } else if *delta_y > 0.0 {
                    self.cycle_selected_block(true); // Down: next
                }
            }
            InputEvent::FocusLost => {
                self.clear_keys();
            }
            InputEvent::VisibilityChanged { visible: _ } => {
                self.clear_keys();
            }
            InputEvent::PointerLockChanged { locked } => {
                self.pointer_locked = *locked;
            }
            _ => {}
        }
    }

    pub fn is_key_pressed(&self, key: &str) -> bool {
        self.pressed_keys.contains(key)
    }

    pub fn clear_keys(&mut self) {
        self.pressed_keys.clear();
    }

    pub fn consume_look(&mut self) -> (f32, f32) {
        let result = self.look_delta;
        self.look_delta = (0.0, 0.0);
        result
    }

    pub fn toggle_wireframe(&mut self) {
        self.wireframe_mode = !self.wireframe_mode;
    }

    pub fn toggle_chunk_borders(&mut self) {
        self.show_chunk_borders = !self.show_chunk_borders;
    }

    pub fn set_selected_block(&mut self, block: Block) {
        self.selected_block = block;
    }

    pub fn cycle_selected_block(&mut self, forward: bool) {
        let blocks = [
            Block::Grass, Block::Dirt, Block::Stone, Block::Sand, Block::Gravel,
            Block::Cobblestone, Block::Bedrock, Block::OakLeaves, Block::Wood,
            Block::Water, Block::Cloud,
        ];
        let current_idx = blocks.iter().position(|&b| b == self.selected_block).unwrap_or(0);
        let next_idx = if forward {
            if current_idx < blocks.len() - 1 { current_idx + 1 } else { 0 }
        } else {
            if current_idx > 0 { current_idx - 1 } else { blocks.len() - 1 }
        };
        self.selected_block = blocks[next_idx];
    }
}

/// Key mapping configuration
#[derive(Clone)]
pub struct KeyBindings {
    pub forward: String,
    pub backward: String,
    pub left: String,
    pub right: String,
    pub jump: String,
    pub sprint: String,
    pub toggle_camera: String,
    pub toggle_player: String,
    pub toggle_wireframe: String,
    pub toggle_chunk_borders: String,
    pub escape: String,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            forward: "w".to_string(),
            backward: "s".to_string(),
            left: "a".to_string(),
            right: "d".to_string(),
            jump: " ".to_string(),
            sprint: "Shift".to_string(),
            toggle_camera: "c".to_string(),
            toggle_player: "p".to_string(),
            toggle_wireframe: "g".to_string(),
            toggle_chunk_borders: "b".to_string(),
            escape: "Escape".to_string(),
        }
    }
}

/// High-level input processor
#[derive(Clone)]
pub struct InputProcessor {
    bindings: KeyBindings,
}

impl InputProcessor {
    pub fn new(bindings: KeyBindings) -> Self {
        Self { bindings }
    }

    pub fn default() -> Self {
        Self::new(KeyBindings::default())
    }

    pub fn is_moving_forward(&self, input: &InputState) -> bool {
        input.is_key_pressed(&self.bindings.forward) || input.is_key_pressed("ArrowUp")
    }

    pub fn is_moving_backward(&self, input: &InputState) -> bool {
        input.is_key_pressed(&self.bindings.backward) || input.is_key_pressed("ArrowDown")
    }

    pub fn is_moving_left(&self, input: &InputState) -> bool {
        input.is_key_pressed(&self.bindings.left) || input.is_key_pressed("ArrowLeft")
    }

    pub fn is_moving_right(&self, input: &InputState) -> bool {
        input.is_key_pressed(&self.bindings.right) || input.is_key_pressed("ArrowRight")
    }

    pub fn is_jumping(&self, input: &InputState) -> bool {
        input.is_key_pressed(&self.bindings.jump)
    }

    pub fn is_sprinting(&self, input: &InputState) -> bool {
        input.is_key_pressed(&self.bindings.sprint)
    }

    pub fn wants_to_toggle_camera(&self, key: &str) -> bool {
        key.eq_ignore_ascii_case(&self.bindings.toggle_camera)
    }

    pub fn wants_to_toggle_player(&self, key: &str) -> bool {
        key.eq_ignore_ascii_case(&self.bindings.toggle_player)
    }

    pub fn wants_to_toggle_wireframe(&self, key: &str) -> bool {
        key.eq_ignore_ascii_case(&self.bindings.toggle_wireframe)
    }

    pub fn wants_to_toggle_chunk_borders(&self, key: &str) -> bool {
        key.eq_ignore_ascii_case(&self.bindings.toggle_chunk_borders)
    }

    pub fn is_escape(&self, key: &str) -> bool {
        key == self.bindings.escape
    }

    pub fn block_from_key(&self, key: &str) -> Option<Block> {
        match key {
            "1" => Some(Block::Grass),
            "2" => Some(Block::Dirt),
            "3" => Some(Block::Stone),
            "4" => Some(Block::Sand),
            "5" => Some(Block::Gravel),
            "6" => Some(Block::Cobblestone),
            "7" => Some(Block::Bedrock),
            "8" => Some(Block::OakLeaves),
            "9" => Some(Block::Wood),
            "0" => Some(Block::Water),
            "-" | "_" => Some(Block::Cloud),
            _ => None,
        }
    }
}

pub mod wasm {
    use super::*;
    use web_sys::{KeyboardEvent, MouseEvent, Event};

    pub fn keyboard_event_to_input(e: &KeyboardEvent, is_down: bool) -> InputEvent {
        let key = e.key();
        if is_down {
            InputEvent::KeyDown(key)
        } else {
            InputEvent::KeyUp(key)
        }
    }

    pub fn mouse_move_to_input(dx: f32, dy: f32) -> InputEvent {
        InputEvent::MouseMove { dx, dy }
    }

    pub fn mouse_click_to_input(e: &MouseEvent, is_down: bool) -> InputEvent {
        InputEvent::MouseClick {
            button: MouseButton::from_web_button(e.button()),
            is_down,
            x: e.client_x() as f32,
            y: e.client_y() as f32,
        }
    }

    pub fn mouse_wheel_to_input(e: &Event) -> Option<InputEvent> {
        let js_val = wasm_bindgen::JsValue::from(e.clone());
        if let Ok(delta_y) = js_sys::Reflect::get(&js_val, &wasm_bindgen::JsValue::from_str("deltaY")) {
            if let Some(dy) = delta_y.as_f64() {
                return Some(InputEvent::MouseWheel { delta_y: dy as f32 });
            }
        }
        None
    }
}
