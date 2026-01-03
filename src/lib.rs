// Re-export all public modules so they can be used from main.rs
pub mod logging;
pub mod utils;
pub mod ui;

// MVC Architecture
pub mod model;
pub mod view;
pub mod controller;

// Common imports
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
use web_sys::{Window, Document, HtmlCanvasElement, KeyboardEvent, MouseEvent, Event, HtmlElement};
use std::rc::Rc;
use std::cell::RefCell;
use glam::Vec3;

use controller::{GameState, CameraController, CameraUniform, LightingUniform, TransformUniform, InputState, FrameLoopContext, PhysicsSystem, InputProcessor};
use model::{Camera, Scene};
use view::render;
#[cfg(target_arch = "wasm32")]
use view::GpuContext;


#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    logging::init();
    let (window, document, canvas) = init_canvas(800, 600)?;
    setup_app(&window, &document, &canvas).await
}

/// Main application setup for WASM
#[cfg(target_arch = "wasm32")]
async fn setup_app(
    window: &Window,
    document: &Document,
    canvas: &HtmlCanvasElement,
) -> Result<(), JsValue> {
    // Initialize GPU
    let gpu = GpuContext::new(canvas, 800, 600)
        .await
        .map_err(|e| js_error(format!("GPU init failed: {e:?}")))?;

    let width = gpu.config.width;
    let height = gpu.config.height;

    // Camera setup
    let cam = Rc::new(RefCell::new(Camera::new(width, height)));
    {
        let mut cam_mut = cam.borrow_mut();
        cam_mut.eye = Vec3::new(16.0, 40.0, 16.0);
        cam_mut.set_look_at(Vec3::new(16.0, 40.0, 25.0));
    }

    // Camera, lighting buffers & bind groups - use unified function
    let camera_resources = render::create_camera_resources(gpu.device.as_ref());
    let cam_buf = camera_resources.camera_buffer;
    let cam_bgl = camera_resources.bind_group_layout;
    let cam_bg = camera_resources.camera_bind_group;
    
    // Initialize with actual camera data
    let cam_buf_data = Rc::new(RefCell::new(CameraUniform {
        view_proj: (cam.borrow().view_proj()).to_cols_array_2d(),
    }));
    gpu.queue.as_ref().write_buffer(&cam_buf, 0, bytemuck::bytes_of(&*cam_buf_data.borrow()));

    // Lighting uniform
    let lighting_buf_data = Rc::new(RefCell::new(LightingUniform {
        sun_dir: [0.5, 1.0, 0.5],
        sun_intensity: 0.3,
        ambient: 0.7,
        _pad1: 0.0,
        _pad2: 0.0,
        _pad3: 0.0,
    }));
    let lighting_buf = camera_resources.lighting_buffer;
    gpu.queue.as_ref().write_buffer(&lighting_buf, 0, bytemuck::bytes_of(&*lighting_buf_data.borrow()));

    // Depth texture
    let depth_format = wgpu::TextureFormat::Depth32Float;
    let (depth_tex, depth_view) = render::create_depth_texture(gpu.device.as_ref(), width, height);
    let depth_view_cell: Rc<RefCell<wgpu::TextureView>> = Rc::new(RefCell::new(depth_view));

    // Create chunk pipelines
    let pipes = render::create_chunk_pipelines(gpu.device.as_ref(), gpu.format, &cam_bgl, depth_format);
    let render_pipeline = pipes.pipeline;
    let wireframe_pipeline = pipes.wireframe_pipeline;
    let wireframe_available = wireframe_pipeline.is_some();

    // Outline resources
    let outline_res = render::create_outline_resources(gpu.device.as_ref(), gpu.format, &cam_bgl, &cam_buf, depth_format);
    let outline_mesh = outline_res.outline_mesh_buffer.unwrap();
    let outline_buf = outline_res.outline_buffer;
    let outline_bg = outline_res.outline_bind_group;
    let outline_pipeline = outline_res.outline_pipeline;

    // Create chunk border mesh
    let chunk_border_mesh = utils::create_chunk_border_mesh(16).upload(gpu.device.as_ref());

    // Create transform buffer for outline
    let outline_transform = Rc::new(RefCell::new(TransformUniform {
        transform: glam::Mat4::IDENTITY.to_cols_array_2d(),
    }));

    // World and game state
    let core = Rc::new(RefCell::new(Scene::new([128, 64, 128], gpu.device.as_ref())));
    let raycast_target: Rc<RefCell<Option<(i32, i32, i32)>>> = Rc::new(RefCell::new(None));
    let game_state = Rc::new(RefCell::new(GameState::new()));
    let input_state = Rc::new(RefCell::new(InputState::new()));
    let egui_events: Rc<RefCell<Vec<egui::Event>>> = Rc::new(RefCell::new(Vec::new()));

    // egui setup
    let egui_ctx = egui::Context::default();
    let egui_renderer = egui_wgpu::Renderer::new(gpu.device.as_ref(), gpu.format, egui_wgpu::RendererOptions::default());

    // Setup input listeners
    setup_input_listeners(
        document,
        window,
        canvas,
        input_state.clone(),
        game_state.clone(),
        egui_events.clone(),
        cam.clone(),
        wireframe_available,
    )?;

    // Create render state
    let mut render_state = render::RenderState {
        format: gpu.format,
        alpha_mode: gpu.config.alpha_mode,
        width,
        height,
        pipeline: render_pipeline,
        wireframe_pipeline: wireframe_pipeline.clone(),
        outline_pipeline,
        outline_mesh,
        show_outline: false,
        chunk_border_mesh,
        show_chunk_borders: false,
        player_pos: Vec3::new(8.0, 80.0, 8.0),
        camera_yaw: 0.0,
        camera_pitch: 0.0,
        camera_aspect: width as f32 / height as f32,
        camera_fov_y: cam.borrow().fov_y,
        camera_z_near: cam.borrow().z_near,
        camera_z_far: cam.borrow().z_far,
        egui_renderer,
        egui_primitives: None,
        egui_full_output: None,
        egui_dpr: 1.0,
        wireframe_mode: false,
    };

    // Setup frame loop
    let mut frame_ctx = FrameLoopContext {
        cam: cam.clone(),
        cam_buf: cam_buf.clone(),
        cam_buf_data,
        lighting_buf: lighting_buf.clone(),
        lighting_buf_data,
        depth_view_cell,
        core,
        input_state,
        game_state,
        camera_controller: CameraController::new(),
        physics_system: PhysicsSystem::new(),
        raycast_target,
        outline_transform,
        outline_buf,
        egui_ctx,
        egui_events,
        last_time: Rc::new(RefCell::new(window.performance().map(|p| p.now()).unwrap_or(0.0))),
    };

    // Continuous redraw using requestAnimationFrame
    let f = RcCellCallback::new(window.clone(), {
        let window_for_loop = window.clone();
        
        move || {
            frame_ctx.update(gpu.device.as_ref(), gpu.queue.as_ref(), &window_for_loop, &gpu.surface, &mut render_state);
            
            // Draw frame
            let core_borrow = frame_ctx.core.borrow();
            let dv = frame_ctx.depth_view_cell.borrow();
            render_state.draw_frame(
                gpu.device.as_ref(),
                gpu.queue.as_ref(),
                &gpu.surface,
                &core_borrow.active,
                &dv,
                &cam_bg,
                &outline_bg,
            );
        }
    });
    f.start();

    Ok(())
}

/// Setup all input event listeners with platform-agnostic abstractions
#[cfg(target_arch = "wasm32")]
fn setup_input_listeners(
    document: &web_sys::Document,
    window: &web_sys::Window,
    canvas: &web_sys::HtmlCanvasElement,
    input_state: Rc<RefCell<InputState>>,
    game_state: Rc<RefCell<GameState>>,
    egui_events: Rc<RefCell<Vec<egui::Event>>>,
    cam: Rc<RefCell<Camera>>,
    wireframe_available: bool,
) -> Result<(), JsValue> {
    let input_processor = InputProcessor::default();

    // Keyboard down
    {
        let input_state = input_state.clone();
        let game_state = game_state.clone();
        let document_for_exit = document.clone();
        let cam = cam.clone();
        let input_processor = input_processor.clone();
        let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
            let key = e.key();

            // Handle special keys
            if input_processor.is_escape(&key) {
                document_for_exit.exit_pointer_lock();
            } else if input_processor.wants_to_toggle_camera(&key) {
                game_state.borrow_mut().toggle_camera_follow();
                e.prevent_default();
            } else if input_processor.wants_to_toggle_player(&key) {
                let mut gs = game_state.borrow_mut();
                gs.toggle_player_mode();
                if gs.player_active {
                    let cam_eye = cam.borrow().eye;
                    gs.player_pos = cam_eye - Vec3::new(0.0, 1.6, 0.0);
                }
                drop(gs);
                e.prevent_default();
            } else if input_processor.wants_to_toggle_wireframe(&key) {
                if wireframe_available {
                    input_state.borrow_mut().toggle_wireframe();
                } else {
                    web_sys::console::log_1(&"Wireframe mode not available on WebGPU/WASM".into());
                }
                e.prevent_default();
            } else if input_processor.wants_to_toggle_chunk_borders(&key) {
                input_state.borrow_mut().toggle_chunk_borders();
                e.prevent_default();
            }

            // Handle block selection keys
            if let Some(block) = input_processor.block_from_key(&key) {
                input_state.borrow_mut().set_selected_block(block);
                e.prevent_default();
            }

            // Prevent default for navigation keys
            if matches!(
                key.as_str(),
                "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "w" | "a" | "s" | "d"
                    | "W" | "A" | "S" | "D" | " " | "Shift"
            ) {
                e.prevent_default();
            }

            input_state.borrow_mut().pressed_keys.insert(key);
        }) as Box<dyn FnMut(KeyboardEvent)>);
        document.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
        keydown.forget();
    }

    // Keyboard up
    {
        let input_state = input_state.clone();
        let keyup = Closure::wrap(Box::new(move |e: KeyboardEvent| {
            input_state.borrow_mut().pressed_keys.remove(e.key().as_str());
        }) as Box<dyn FnMut(KeyboardEvent)>);
        document.add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref())?;
        keyup.forget();
    }

    // Focus loss - clear all keys
    {
        let input_state = input_state.clone();
        let blur = Closure::wrap(Box::new(move |_e: Event| {
            input_state.borrow_mut().clear_keys();
        }) as Box<dyn FnMut(Event)>);
        window.add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref())?;
        blur.forget();
    }

    // Visibility change - clear all keys
    {
        let input_state = input_state.clone();
        let visibility = Closure::wrap(Box::new(move |_e: Event| {
            input_state.borrow_mut().clear_keys();
        }) as Box<dyn FnMut(Event)>);
        document.add_event_listener_with_callback("visibilitychange", visibility.as_ref().unchecked_ref())?;
        visibility.forget();
    }

    // Pointer lock change
    {
        let input_state = input_state.clone();
        let doc_pl = document.clone();
        let plc = Closure::wrap(Box::new(move |_e: Event| {
            input_state.borrow_mut().pointer_locked = doc_pl.pointer_lock_element().is_some();
        }) as Box<dyn FnMut(Event)>);
        document.add_event_listener_with_callback("pointerlockchange", plc.as_ref().unchecked_ref())?;
        plc.forget();
    }

    // Canvas click to enter pointer lock
    {
        let canvas_click = canvas.clone();
        let click = Closure::wrap(Box::new(move |_e: MouseEvent| {
            if let Ok(html_el) = canvas_click.clone().dyn_into::<HtmlElement>() {
                html_el.request_pointer_lock();
            }
        }) as Box<dyn FnMut(MouseEvent)>);
        canvas.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
        click.forget();
    }

    // Mouse move
    {
        let input_state = input_state.clone();
        let egui_events_q = egui_events.clone();
        let mm = Closure::wrap(Box::new(move |e: MouseEvent| {
            if input_state.borrow().pointer_locked {
                let dx = e.movement_x() as f32;
                let dy = e.movement_y() as f32;
                input_state.borrow_mut().look_delta.0 += dx;
                input_state.borrow_mut().look_delta.1 += dy;
            } else {
                let px = e.client_x() as f32;
                let py = e.client_y() as f32;
                egui_events_q.borrow_mut().push(egui::Event::PointerMoved(egui::pos2(px, py)));
            }
        }) as Box<dyn FnMut(MouseEvent)>);
        document.add_event_listener_with_callback("mousemove", mm.as_ref().unchecked_ref())?;
        mm.forget();
    }

    // Mouse down - detect block placement/removal
    {
        let input_state = input_state.clone();
        let mousedown = Closure::wrap(Box::new(move |e: MouseEvent| {
            let button = e.button();
            match button {
                0 => input_state.borrow_mut().left_click = true,   // Left click
                2 => input_state.borrow_mut().right_click = true,  // Right click
                _ => {},
            }
            e.prevent_default();
        }) as Box<dyn FnMut(MouseEvent)>);
        document.add_event_listener_with_callback("mousedown", mousedown.as_ref().unchecked_ref())?;
        mousedown.forget();
    }

    // Mouse up - clear clicks
    {
        let input_state = input_state.clone();
        let mouseup = Closure::wrap(Box::new(move |_e: MouseEvent| {
            let mut state = input_state.borrow_mut();
            state.left_click = false;
            state.right_click = false;
        }) as Box<dyn FnMut(MouseEvent)>);
        document.add_event_listener_with_callback("mouseup", mouseup.as_ref().unchecked_ref())?;
        mouseup.forget();
    }

    // Context menu prevention
    {
        let contextmenu = Closure::wrap(Box::new(move |e: MouseEvent| {
            e.prevent_default();
        }) as Box<dyn FnMut(MouseEvent)>);
        document.add_event_listener_with_callback("contextmenu", contextmenu.as_ref().unchecked_ref())?;
        contextmenu.forget();
    }

    // Mouse wheel
    {
        let input_state = input_state.clone();
        let wheel = Closure::wrap(Box::new(move |e: Event| {
            if let Some(_event) = controller::input::wasm::mouse_wheel_to_input(&e) {
                let js_val = wasm_bindgen::JsValue::from(e.clone());
                if let Ok(delta_y) = js_sys::Reflect::get(&js_val, &wasm_bindgen::JsValue::from_str("deltaY")) {
                    if let Some(dy) = delta_y.as_f64() {
                        if dy < 0.0 {
                            input_state.borrow_mut().cycle_selected_block(false);
                        } else if dy > 0.0 {
                            input_state.borrow_mut().cycle_selected_block(true);
                        }
                        e.prevent_default();
                    }
                }
            }
        }) as Box<dyn FnMut(Event)>);
        document.add_event_listener_with_callback("wheel", wheel.as_ref().unchecked_ref())?;
        wheel.forget();
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn init_canvas(width: u32, height: u32) -> Result<(Window, Document, HtmlCanvasElement), JsValue> {
    let window = web_sys::window().ok_or(js_error("no global `window`"))?;
    let document = window.document().ok_or(js_error("no document on window"))?;
    let body = document.body().ok_or(js_error("no body on document"))?;
    let canvas_el = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| js_error("failed to create canvas"))?;
    canvas_el.set_width(width);
    canvas_el.set_height(height);
    body.append_child(&canvas_el)?;
    Ok((window, document, canvas_el))
}

#[cfg(target_arch = "wasm32")]
fn js_error<E: Into<String>>(msg: E) -> JsValue {
    JsValue::from_str(&msg.into())
}

struct RcCellCallback {
    inner: Rc<RefCell<Box<dyn FnMut()>>>,
    window: Window,
}

impl RcCellCallback {
    fn new(window: Window, f: impl FnMut() + 'static) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Box::new(f))),
            window,
        }
    }

    fn start(self) {
        let inner = self.inner.clone();
        let window = self.window.clone();

        let callback = Rc::new(RefCell::new(None::<Closure<dyn FnMut()>>));
        let callback_clone = callback.clone();

        *callback.borrow_mut() = Some(Closure::wrap(Box::new(move || {
            inner.borrow_mut().as_mut()();

            // Recursively schedule next frame
            let cb_ref = callback_clone.borrow();
            window
                .request_animation_frame(cb_ref.as_ref().unwrap().as_ref().unchecked_ref())
                .expect("RAF failed");
        }) as Box<dyn FnMut()>));

        self.window
            .request_animation_frame(
                callback.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
            )
            .expect("RAF start failed");

        // Leak the closure to keep it alive
        std::mem::forget(callback);
    }
}
