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
    let base_color = in.color.xyz;
    let normal = normalize(in.normal);
    
    // Reconstruct sun direction from individual components (already normalized on CPU)
    let sun_dir = vec3<f32>(lighting.sun_dir_x, lighting.sun_dir_y, lighting.sun_dir_z);
    
    // Detect cloud blocks by their near-white color (0.95, 0.95, 0.95)
    let is_cloud = step(0.9, min(min(base_color.r, base_color.g), base_color.b));
    
    // === FACE-BASED AMBIENT OCCLUSION ===
    // Top faces (+Y) are brightest, bottom faces (-Y) are darkest, sides are medium
    // Clouds get minimal AO (they're fluffy and lit from all sides)
    let ao_top = 1.0;
    let ao_side = mix(0.85, 0.98, is_cloud);
    let ao_bottom = mix(0.65, 0.92, is_cloud);
    let face_ao = mix(ao_side, mix(ao_bottom, ao_top, max(normal.y, 0.0)), abs(normal.y));
    
    // === SUN LIGHTING ===
    // Warm sunlight with soft falloff
    let sun_dot = dot(normal, sun_dir);
    let sun_factor = smoothstep(-0.1, 1.0, sun_dot);
    let sun_color = vec3<f32>(1.0, 0.95, 0.85); // Warm white sunlight
    let sun_light = sun_factor * lighting.sun_intensity * sun_color;
    
    // === SKY LIGHT ===
    // Warm blue fill light from above (hemisphere lighting)
    let sky_factor = (normal.y + 1.0) * 0.5; // 0 at bottom, 1 at top
    let sky_color = vec3<f32>(0.55, 0.7, 0.9); // Warm sky blue (less saturated, hint of warmth)
    let sky_light = sky_factor * 0.3 * sky_color;
    
    // === GROUND BOUNCE ===
    // Subtle warm bounce light from below
    let ground_factor = (-normal.y + 1.0) * 0.5; // 0 at top, 1 at bottom
    let ground_color = vec3<f32>(0.45, 0.38, 0.28); // Warm earth tones
    let ground_light = ground_factor * 0.12 * ground_color;
    
    // === COMBINE LIGHTING ===
    let ambient_color = vec3<f32>(1.0, 0.98, 0.95); // Slightly warm ambient
    let ambient_light = lighting.ambient * ambient_color;
    
    // Total illumination
    let total_light = (ambient_light + sun_light + sky_light + ground_light) * face_ao;
    
    // Apply lighting to base color
    var lit_color = base_color * total_light;
    
    // === DISTANCE + HEIGHT ATMOSPHERIC HAZE ===
    // Distance fog creates depth perception, height fog adds atmosphere
    let haze_color = vec3<f32>(0.75, 0.82, 0.92); // Warm blue-gray haze
    
    // Squared distance from camera (horizontal plane) - avoids expensive sqrt
    let dx = in.world_pos.x - lighting.eye_x;
    let dz = in.world_pos.z - lighting.eye_z;
    let dist_sq = dx * dx + dz * dz;
    
    // Distance fog - increases with distance from camera (using squared distances)
    let fog_near_sq = 40000.0;   // 200^2 - Start fading at this distance
    let fog_far_sq = 250000.0;   // 500^2 - Fully fogged at this distance
    let dist_fog = smoothstep(fog_near_sq, fog_far_sq, dist_sq);
    
    // Height fog - lower areas have more haze
    let height_fog = 1.0 - smoothstep(0.0, 140.0, in.world_pos.y);
    
    // Combine: distance fog is primary, height fog adds extra at low areas
    let total_fog = dist_fog * 0.55 + height_fog * 0.12;
    let fog_amount = clamp(total_fog, 0.0, 0.7); // Cap max fog
    
    lit_color = mix(lit_color, haze_color * (ambient_light.x + 0.35), fog_amount);
    
    // === SATURATION BOOST ===
    // Slightly boost color saturation for vibrancy
    let luminance = dot(lit_color, vec3<f32>(0.299, 0.587, 0.114));
    let saturation_boost = 1.15;
    lit_color = mix(vec3<f32>(luminance), lit_color, saturation_boost);
    
    // Clamp to valid range
    lit_color = clamp(lit_color, vec3<f32>(0.0), vec3<f32>(1.0));
    
    return vec4<f32>(lit_color, in.color.w);
}
