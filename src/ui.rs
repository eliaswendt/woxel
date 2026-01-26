use egui::Context;
use std::rc::Rc;
use std::cell::RefCell;
use crate::model::Camera;
use crate::model::CHUNK_SIZE;
use crate::controller::GameState;
use crate::controller::InputState;
use crate::model::Scene;
use crate::model::Block;

const HOTBAR_ITEM_SIZE: f32 = 28.0;
const HOTBAR_ITEM_SPACING: f32 = 2.0;

/// Format large numbers with K/M suffixes for readability
fn format_number(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f32 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f32 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Build the complete UI and return egui output
pub fn build_ui(
    egui_ctx: &Context,
    cam: &Rc<RefCell<Camera>>,
    game_state: &Rc<RefCell<GameState>>,
    input_state: &Rc<RefCell<InputState>>,
    scene: &Rc<RefCell<Scene>>,
    canvas_width: u32,
    canvas_height: u32,
    dt: f32,
    now: f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let ppp = egui_ctx.pixels_per_point();
    let logical_width = canvas_width as f32 / ppp;
    let logical_height = canvas_height as f32 / ppp;
    
    let mut raw_input = egui::RawInput::default();
    raw_input.time = Some(now as f64 / 1000.0);
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::new(0.0, 0.0),
        egui::vec2(logical_width, logical_height),
    ));
    
    // Add pointer events (scaled to logical pixels)
    for event in events {
        let scaled_event = match event {
            egui::Event::PointerMoved(pos) => {
                egui::Event::PointerMoved(egui::pos2(pos.x / ppp, pos.y / ppp))
            }
            egui::Event::PointerButton { pos, button, pressed, modifiers } => {
                egui::Event::PointerButton {
                    pos: egui::pos2(pos.x / ppp, pos.y / ppp),
                    button,
                    pressed,
                    modifiers,
                }
            }
            other => other,
        };
        raw_input.events.push(scaled_event);
    }

    egui_ctx.run(raw_input, |ctx| {
        draw_crosshair(ctx, ppp);
        draw_debug_window(ctx, cam, game_state, scene, dt);
        draw_hotbar(ctx, input_state, canvas_width, canvas_height, ppp);
    })
}

fn draw_crosshair(ctx: &Context, _ppp: f32) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::TOP, egui::Id::new("crosshair")));
    let screen_rect = ctx.viewport_rect();
    let center_x = screen_rect.width() / 2.0;
    let center_y = screen_rect.height() / 2.0;
    let size = 10.0;
    painter.line_segment(
        [
            egui::Pos2::new(center_x - size, center_y),
            egui::Pos2::new(center_x + size, center_y),
        ],
        egui::Stroke::new(1.0, egui::Color32::WHITE),
    );
    painter.line_segment(
        [
            egui::Pos2::new(center_x, center_y - size),
            egui::Pos2::new(center_x, center_y + size),
        ],
        egui::Stroke::new(1.0, egui::Color32::WHITE),
    );
}

fn draw_debug_window(ctx: &Context, cam: &Rc<RefCell<Camera>>, game_state: &Rc<RefCell<GameState>>, core: &Rc<RefCell<Scene>>, dt: f32) {
    let player_pos = game_state.borrow().player_pos;
    let chunk_x = (player_pos.x / CHUNK_SIZE as f32).floor() as i32;
    let chunk_y = (player_pos.y / CHUNK_SIZE as f32).floor() as i32;
    let chunk_z = (player_pos.z / CHUNK_SIZE as f32).floor() as i32;
    
    // Get GPU buffer stats
    let (total_vertices, total_faces) = core.borrow().get_n_vertices_and_faces();

    egui::Window::new("Debug")
        .default_pos([8.0, 8.0])
        .default_width(180.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("FPS: {:.0}", if dt > 0.0 { 1.0 / dt } else { 0.0 }));
            ui.label(format!("Pos: {:.0} {:.0} {:.0}", player_pos.x, player_pos.y, player_pos.z));
            ui.label(format!("Chunk: {} {} {}", chunk_x, chunk_y, chunk_z));
            ui.label(format!("Yaw: {:.1}° Pitch: {:.1}°", cam.borrow().yaw.to_degrees(), cam.borrow().pitch.to_degrees()));
            
            ui.separator();
            ui.label(format!("Vertices: {}", format_number(total_vertices)));
            ui.label(format!("Faces: {}", format_number(total_faces)));
            
            ui.separator();
            
            // FOV slider
            let mut fov_deg = cam.borrow().fov_y.to_degrees().clamp(30.0, 120.0);
            ui.horizontal(|ui| {
                ui.label("FOV");
                if ui.add(egui::Slider::new(&mut fov_deg, 30.0..=120.0).step_by(5.0)).changed() {
                    cam.borrow_mut().fov_y = fov_deg.to_radians();
                }
            });
            
            ui.separator();
            ui.label("Render Distance");
            
            // Render distance sliders
            let mut gs = game_state.borrow_mut();
            let mut rd = gs.render_distance;
            
            ui.horizontal(|ui| {
                ui.label("X:");
                ui.add(egui::Slider::new(&mut rd[0], 1..=64).step_by(1.0));
            });
            ui.horizontal(|ui| {
                ui.label("Y:");
                ui.add(egui::Slider::new(&mut rd[1], 1..=64).step_by(1.0));
            });
            ui.horizontal(|ui| {
                ui.label("Z:");
                ui.add(egui::Slider::new(&mut rd[2], 1..=64).step_by(1.0));
            });
            
            // Check if changed and update
            if rd != gs.render_distance {
                gs.set_render_distance(rd[0], rd[1], rd[2]);
            }
            
            ui.separator();
            ui.label("Compute Budget");
            ui.add(egui::Slider::new(&mut gs.compute_budget, 0..=500).step_by(1.0));
            
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Day Cycle");
                ui.add(egui::Slider::new(&mut gs.day_cycle_seconds, 0.0..=600.0)
                    .step_by(10.0)
                    .suffix("s"));
            });
            if gs.day_cycle_seconds == 0.0 {
                ui.label("(disabled - static sun)");
            }
        });
}

fn draw_hotbar(ctx: &Context, input_state: &Rc<RefCell<InputState>>, _canvas_width: u32, _canvas_height: u32, _ppp: f32) {
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

    let screen_rect = ctx.viewport_rect();
    let hotbar_width = blocks.len() as f32 * (HOTBAR_ITEM_SIZE + HOTBAR_ITEM_SPACING) + HOTBAR_ITEM_SPACING * 2.0;
    let hotbar_height = HOTBAR_ITEM_SIZE + HOTBAR_ITEM_SPACING * 2.0;
    let hotbar_x = (screen_rect.width() - hotbar_width) / 2.0;
    let hotbar_y = screen_rect.height() - hotbar_height - 8.0;

    egui::Area::new(egui::Id::new("hotbar"))
        .fixed_pos(egui::Pos2::new(hotbar_x, hotbar_y))
        .show(ctx, |ui| {
            let current_block = input_state.borrow().selected_block;
            
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(HOTBAR_ITEM_SPACING, 0.0);
                
                for (block, key) in blocks.iter() {
                    let is_selected = current_block == *block;
                    let color = block.color(0);
                    let color32 = egui::Color32::from_rgb(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                    );

                    let border_color = if is_selected {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::from_gray(100)
                    };
                    let border_width = if is_selected { 2.0 } else { 1.0 };

                    let frame = egui::Frame::NONE
                        .fill(color32)
                        .stroke(egui::Stroke::new(border_width, border_color))
                        .inner_margin(egui::Margin::same(2));

                    frame.show(ui, |ui| {
                        ui.set_min_size(egui::vec2(HOTBAR_ITEM_SIZE, HOTBAR_ITEM_SIZE));
                        ui.set_max_size(egui::vec2(HOTBAR_ITEM_SIZE, HOTBAR_ITEM_SIZE));
                        ui.vertical_centered(|ui| {
                            ui.add_space(HOTBAR_ITEM_SIZE / 2.0 - 6.0);
                            ui.label(egui::RichText::new(*key).size(9.0).color(egui::Color32::WHITE));
                        });
                    });
                }
            });
        });
}
