#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use wgpu::Device;
use std::sync::Arc;

/// GPU context - unified for both WASM and native
pub struct GpuContext {
    pub device: Arc<Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: wgpu::Surface<'static>,
    pub format: wgpu::TextureFormat,
    pub config: wgpu::SurfaceConfiguration,
}

/// Shared GPU initialization helper
/// Returns (adapter, device, queue) for both platforms
async fn init_device_and_queue(
    adapter: &wgpu::Adapter,
    features: wgpu::Features,
) -> (Arc<Device>, Arc<wgpu::Queue>) {
    let adapter_limits = adapter.limits();
    let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter_limits);

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: features,
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                trace: wgpu::Trace::default(),
            },
        )
        .await
        .expect("Failed to request device");

    (Arc::new(device), Arc::new(queue))
}

/// Shared surface configuration helper
fn configure_surface(
    device: &Device,
    adapter: &wgpu::Adapter,
    surface: &wgpu::Surface,
    width: u32,
    height: u32,
) -> (wgpu::TextureFormat, wgpu::SurfaceConfiguration) {
    let caps = surface.get_capabilities(adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(caps.formats[0]);

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(device, &config);

    (format, config)
}

#[cfg(target_arch = "wasm32")]
impl GpuContext {
    /// Initialize GPU for a given canvas surface (WASM)
    pub async fn new(
        canvas: &web_sys::HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<Self, wgpu::CreateSurfaceError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("No suitable GPU adapter found");

        let (device, queue) = init_device_and_queue(&adapter, wgpu::Features::empty()).await;
        let (format, config) = configure_surface(&device, &adapter, &surface, width, height);

        Ok(GpuContext {
            device,
            queue,
            surface,
            format,
            config,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl GpuContext {
    /// Initialize GPU for a given window surface (Native)
    pub async fn new_native(
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find adapter");

        let (device, queue) = init_device_and_queue(
            &adapter,
            wgpu::Features::POLYGON_MODE_LINE,
        ).await;
        
        let (format, config) = configure_surface(&device, &adapter, &surface, width, height);

        GpuContext {
            device,
            queue,
            surface,
            format,
            config,
        }
    }}