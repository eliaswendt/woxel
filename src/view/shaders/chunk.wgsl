struct Camera {
    view_proj: mat4x4<f32>,
};

struct Lighting {
    sun_dir_x: f32,
    sun_dir_y: f32,
    sun_dir_z: f32,
    sun_intensity: f32,
    ambient: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
};

@group(0) @binding(0)
var<uniform> camera: Camera;
@group(0) @binding(1)
var<uniform> lighting: Lighting;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = camera.view_proj * vec4<f32>(in.pos, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    out.world_pos = in.pos;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Reconstruct sun direction from individual components
    let sun_dir = normalize(vec3<f32>(lighting.sun_dir_x, lighting.sun_dir_y, lighting.sun_dir_z));
    
    // Ambient lighting (always present, high base for soft look)
    var light_amount = lighting.ambient;
    
    // Sun directional lighting with soft falloff
    let normal = normalize(in.normal);
    // Use smoothstep for softer transitions instead of max(dot, 0)
    let sun_dot = dot(normal, sun_dir);
    let sun_light = smoothstep(-0.2, 0.8, sun_dot) * lighting.sun_intensity;
    light_amount = light_amount + sun_light;
    
    // Apply lighting to color (preserve alpha)
    let lit_color = vec3<f32>(in.color.x, in.color.y, in.color.z) * light_amount;
    return vec4<f32>(lit_color, in.color.w);
}
