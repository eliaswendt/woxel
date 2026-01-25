struct Camera {
    view_proj: mat4x4<f32>,
};

struct Lighting {
    sun_dir_x: f32,
    sun_dir_y: f32,
    sun_dir_z: f32,
    sun_intensity: f32,
    ambient: f32,
    time: f32,
    eye_x: f32,
    eye_z: f32,
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
    @location(3) roughness: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = camera.view_proj * vec4<f32>(in.pos, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    out.world_pos = in.pos;
    out.roughness = in.uv.x;  // roughness stored in uv.x
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let base_color = vec3<f32>(in.color.x, in.color.y, in.color.z);
    let normal = normalize(in.normal);
    
    // Sun direction (already normalized on CPU)
    let sun_dir = vec3<f32>(lighting.sun_dir_x, lighting.sun_dir_y, lighting.sun_dir_z);
    
    // === PER-BLOCK VARIATION (combined hash calculation) ===
    let block_pos = floor(in.world_pos);
    // Single hash with vec3 output via swizzling trick
    let hash_input = block_pos.xz;
    let hash_base = sin(dot(hash_input, vec2<f32>(12.9898, 78.233))) * 43758.5453;
    let hash1 = fract(hash_base);
    let hash2 = fract(hash_base * 1.3);
    let hash3 = fract(hash_base * 1.7);
    
    // Vary brightness and hue slightly per block
    let brightness_var = mix(0.96, 1.08, hash1);
    let hue_shift = (hash2 - 0.5) * 0.05;
    var varied_color = base_color * brightness_var;
    varied_color.r += hue_shift;
    varied_color.b -= hue_shift;
    
    // === FACE-BASED AMBIENT OCCLUSION ===
    let face_ao = mix(0.82, mix(0.6, 1.0, max(normal.y, 0.0)), abs(normal.y));
    
    // === EDGE DARKENING (branchless) ===
    let frac_pos = fract(in.world_pos);
    let edge = vec3<f32>(
        1.0 - 4.0 * frac_pos.x * (1.0 - frac_pos.x),
        1.0 - 4.0 * frac_pos.y * (1.0 - frac_pos.y),
        1.0 - 4.0 * frac_pos.z * (1.0 - frac_pos.z)
    );
    let abs_n = abs(normal);
    // Select edges perpendicular to dominant normal axis (branchless)
    let edge_factor = select(
        select(max(edge.x, edge.y), max(edge.y, edge.z), abs_n.x > 0.5),
        max(edge.x, edge.z),
        abs_n.y > 0.5
    );
    let edge_ao = 1.0 - 0.12 * edge_factor * edge_factor;
    
    // === SUN LIGHTING ===
    let sun_factor = smoothstep(-0.1, 1.0, dot(normal, sun_dir));
    let sun_color = vec3<f32>(1.0, 0.95, 0.85);
    let sun_light = sun_factor * lighting.sun_intensity * sun_color;
    
    // === SPECULAR (with roughness variation) ===
    let roughness = clamp(in.roughness + (hash3 - 0.5) * 0.15, 0.0, 1.0);
    let smoothness = 1.0 - roughness;
    
    let view_dir = normalize(vec3<f32>(lighting.eye_x - in.world_pos.x, 60.0 - in.world_pos.y, lighting.eye_z - in.world_pos.z));
    let half_vec = normalize(sun_dir + view_dir);
    let NdotH = max(dot(normal, half_vec), 0.0);
    let NdotV = max(dot(normal, view_dir), 0.0);
    
    let spec_power = mix(4.0, 96.0, smoothness * smoothness);
    let spec_strength = pow(NdotH, spec_power) * smoothness * 0.8;
    let fresnel = pow(1.0 - NdotV, 4.0) * smoothness * 0.2;
    let specular = (spec_strength + fresnel) * sun_color * lighting.sun_intensity;
    
    // === HEMISPHERE LIGHTING ===
    let sky_light = (normal.y + 1.0) * 0.16 * vec3<f32>(0.55, 0.7, 0.9);
    let ground_light = (-normal.y + 1.0) * 0.07 * vec3<f32>(0.45, 0.38, 0.28);
    
    // === HEIGHT TINT ===
    let height_norm = clamp((in.world_pos.y - 30.0) * 0.01, 0.0, 1.0);
    let height_tint = mix(vec3<f32>(1.02, 1.0, 0.96), vec3<f32>(0.97, 0.98, 1.03), height_norm);
    
    // === COMBINE LIGHTING ===
    let ambient_light = lighting.ambient * vec3<f32>(1.0, 0.98, 0.95);
    let total_ao = face_ao * edge_ao;
    let total_light = (ambient_light + sun_light + sky_light + ground_light) * total_ao;
    var lit_color = varied_color * height_tint * total_light + specular;
    
    // === DISTANCE FOG (squared distance - no sqrt) ===
    let dx = in.world_pos.x - lighting.eye_x;
    let dz = in.world_pos.z - lighting.eye_z;
    let dist_sq = dx * dx + dz * dz;
    let dist_fog = smoothstep(0.0, 1000000.0, dist_sq);  // 1000^2
    let height_fog = 1.0 - smoothstep(0.0, 140.0, in.world_pos.y);
    let fog_amount = clamp(dist_fog * 0.40 + height_fog * 0.12, 0.0, 0.7);
    
    let haze_color = vec3<f32>(0.75, 0.82, 0.92) * (ambient_light.x + 0.35);
    lit_color = mix(lit_color, haze_color, fog_amount);
    
    // === SATURATION BOOST ===
    let luminance = dot(lit_color, vec3<f32>(0.299, 0.587, 0.114));
    lit_color = mix(vec3<f32>(luminance), lit_color, 1.18);
    
    return vec4<f32>(clamp(lit_color, vec3<f32>(0.0), vec3<f32>(1.0)), in.color.w);
}
