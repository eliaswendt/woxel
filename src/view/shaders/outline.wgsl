struct Camera {
    view_proj: mat4x4<f32>,
};
struct Transform {
    transform: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: Camera;
@group(0) @binding(1)
var<uniform> transform: Transform;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
};
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let world_pos = transform.transform * vec4<f32>(in.pos, 1.0);
    out.pos = camera.view_proj * world_pos;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
