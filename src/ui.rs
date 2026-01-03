use egui::Context;
use std::rc::Rc;
use std::cell::RefCell;
use crate::model::Camera;
use crate::model::CHUNK_SIZE;
use crate::controller::GameState;
use crate::controller::InputState;
use crate::model::Scene;
use crate::model::Block;

/// Build the complete UI and return egui output
pub fn build_ui(
    egui_ctx: &Context,
    cam: &Rc<RefCell<Camera>>,
    game_state: &Rc<RefCell<GameState>>,
    input_state: &Rc<RefCell<InputState>>,
    core: &Rc<RefCell<Scene>>,
    canvas_width: u32,
    canvas_height: u32,
    dt: f32,
    now: f64,
) -> egui::FullOutput {
    let mut raw_input = egui::RawInput::default();
    raw_input.time = Some(now as f64 / 1000.0);
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::new(0.0, 0.0),
        egui::vec2(canvas_width as f32, canvas_height as f32),
    ));

    egui_ctx.run(raw_input, |ctx| {
        draw_crosshair(ctx);
        draw_debug_window(ctx, cam, game_state, core, dt);
        draw_settings_window(ctx, cam, canvas_width);
        draw_hotbar(ctx, input_state, canvas_height);
    })
}

fn draw_crosshair(ctx: &Context) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::TOP, egui::Id::new("crosshair")));
    let screen_size = ctx.available_rect();
    let center = screen_size.center();
    let size = 10.0;
    painter.line_segment(
        [
            egui::Pos2::new(center.x - size, center.y),
            egui::Pos2::new(center.x + size, center.y),
        ],
        egui::Stroke::new(1.0, egui::Color32::WHITE),
    );
    painter.line_segment(
        [
            egui::Pos2::new(center.x, center.y - size),
            egui::Pos2::new(center.x, center.y + size),
        ],
        egui::Stroke::new(1.0, egui::Color32::WHITE),
    );
}

fn draw_debug_window(ctx: &Context, cam: &Rc<RefCell<Camera>>, game_state: &Rc<RefCell<GameState>>, core: &Rc<RefCell<Scene>>, dt: f32) {

    let eye = cam.borrow().eye;
    let player_pos = game_state.borrow().player_pos;
    let chunk_x = (player_pos.x / CHUNK_SIZE as f32).floor() as i32;
    let chunk_y = (player_pos.y / CHUNK_SIZE as f32).floor() as i32;
    let chunk_z = (player_pos.z / CHUNK_SIZE as f32).floor() as i32;

    egui::Window::new("Debug")
        .default_pos([8.0, 8.0])
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("FPS: {:.0}", if dt > 0.0 { 1.0 / dt } else { 0.0 }))
                    .small(),
            );
            ui.label(egui::RichText::new(format!("Pos: x: {:.0} y: {:.0} z: {:.0}", player_pos.x, player_pos.y, player_pos.z)).small());
            ui.label(egui::RichText::new(format!("Chunk: x: {} y: {} z: {}", chunk_x, chunk_y, chunk_z)).small());
            ui.label(egui::RichText::new(format!("Yaw: {:.2} Pitch: {:.2}", cam.borrow().yaw.to_degrees(), cam.borrow().pitch.to_degrees())).small());
            ui.label(egui::RichText::new(format!("Chunks: 64x64x64 (fixed)")).small());
            ui.separator();
            ui.label(egui::RichText::new("Controls:").small());
            ui.label(egui::RichText::new("WASD - Move").small());
            ui.label(egui::RichText::new("Space - Up").small());
            ui.label(egui::RichText::new("Shift - Down").small());
            ui.label(egui::RichText::new("Ctrl - Speed boost").small());
            ui.label(egui::RichText::new("C - Toggle camera lock").small());
            ui.label(egui::RichText::new("P - Toggle player mode").small());
        });
}

fn draw_settings_window(ctx: &Context, cam: &Rc<RefCell<Camera>>, canvas_width: u32) {
    egui::Window::new("Settings")
        .default_pos([canvas_width as f32 - 140.0, 8.0])
        .default_size([130.0, 100.0])
        .show(ctx, |ui| {
            let mut fov_deg = cam.borrow().fov_y.to_degrees().clamp(30.0, 120.0);
            ui.label(egui::RichText::new("FOV").small());
            if ui.add(egui::Slider::new(&mut fov_deg, 30.0..=120.0).step_by(5.0)).changed() {
                cam.borrow_mut().fov_y = fov_deg.to_radians();
            }
        });
}

fn draw_hotbar(ctx: &Context, input_state: &Rc<RefCell<InputState>>, canvas_height: u32) {
    egui::Area::new(egui::Id::new("hotbar"))
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -8.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let blocks = [
                    (Block::Grass, "1"),
                    (Block::Dirt, "2"),
                    (Block::Stone, "3"),
                    (Block::Sand, "4"),
                    (Block::Gravel, "5"),
                    (Block::Cobblestone, "6"),
                    (Block::Bedrock, "7"),
                    (Block::OakLeaves, "8"),
                    (Block::Wood, "9"),
                    (Block::Water, "0"),
                    (Block::Cloud, "-"),
                ];
                let current = input_state.borrow().selected_block;
                for (block, key) in blocks.iter() {
                    let is_selected = current == *block;
                    let color = block.color(0);
                    let color32 = egui::Color32::from_rgb(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                    );
                    let size = if is_selected { 40.0 } else { 36.0 };
                    let frame = egui::Frame::NONE
                        .fill(color32)
                        .stroke(if is_selected {
                            egui::Stroke::new(2.0, egui::Color32::YELLOW)
                        } else {
                            egui::Stroke::new(0.5, egui::Color32::BLACK)
                        })
                        .inner_margin(2.0);
                    frame.show(ui, |ui| {
                        ui.set_min_size(egui::vec2(size, size));
                        ui.vertical_centered(|ui| {
                            ui.add_space(size / 2.0 - 6.0);
                            ui.label(egui::RichText::new(*key).size(10.0).color(egui::Color32::WHITE));
                        });
                    });
                }
            });
        });
}
