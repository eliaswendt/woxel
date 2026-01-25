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
    
    // Reconstruct sun direction from individual components
    let sun_dir = normalize(vec3<f32>(lighting.sun_dir_x, lighting.sun_dir_y, lighting.sun_dir_z));
    
    
    // === PER-BLOCK COLOR VARIATION ===
    // Add subtle random color shifts based on world position for organic look
    let block_pos = floor(in.world_pos);
    let hash1 = fract(sin(dot(block_pos.xz, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    let hash2 = fract(sin(dot(block_pos.xz + vec2<f32>(1.0, 0.0), vec2<f32>(12.9898, 78.233))) * 43758.5453);
    
    // Vary brightness and hue slightly per block
    let brightness_var = mix(0.96, 1.08, hash1);  // ±4% brightness
    let hue_shift = (hash2 - 0.5) * 0.05;         // ±1.5% hue
    var varied_color = base_color * brightness_var;
    // Subtle hue variation (warm/cool shift)
    varied_color.r += hue_shift;
    varied_color.b -= hue_shift;
    
    // === FACE-BASED AMBIENT OCCLUSION ===
    // Top faces (+Y) are brightest, bottom faces (-Y) are darkest, sides are medium
    let ao_top = 1.0;
    let ao_side = 0.82;
    let ao_bottom = 0.6;
    let face_ao = mix(ao_side, mix(ao_bottom, ao_top, max(normal.y, 0.0)), abs(normal.y));
    
    // === EDGE DARKENING (PSEUDO-AO) ===
    // Darken areas near block edges for more depth
    let frac_pos = fract(in.world_pos);
    // Distance from center of face (0 at center, 1 at edges)
    let edge_x = 1.0 - 4.0 * frac_pos.x * (1.0 - frac_pos.x);
    let edge_y = 1.0 - 4.0 * frac_pos.y * (1.0 - frac_pos.y);
    let edge_z = 1.0 - 4.0 * frac_pos.z * (1.0 - frac_pos.z);
    // Use edges perpendicular to the face normal
    var edge_factor: f32;
    if abs(normal.y) > 0.5 {
        edge_factor = max(edge_x, edge_z);
    } else if abs(normal.x) > 0.5 {
        edge_factor = max(edge_y, edge_z);
    } else {
        edge_factor = max(edge_x, edge_y);
    }
    let edge_ao = mix(1.0, 0.88, edge_factor * edge_factor);
    
    // === SUN LIGHTING ===
    // Warm sunlight with soft falloff
    let sun_dot = dot(normal, sun_dir);
    let sun_factor = smoothstep(-0.1, 1.0, sun_dot);
    let sun_color = vec3<f32>(1.0, 0.95, 0.85); // Warm white sunlight
    let sun_light = sun_factor * lighting.sun_intensity * sun_color;
    
    // === SPECULAR HIGHLIGHTS (glossy surfaces) ===
    // Blinn-Phong style specular based on roughness
    let roughness = in.roughness;
    let smoothness = 1.0 - roughness;
    
    // View direction (from surface to camera)
    let eye_y = 60.0; // Approximate camera height
    let view_dir = normalize(vec3<f32>(lighting.eye_x - in.world_pos.x, eye_y - in.world_pos.y, lighting.eye_z - in.world_pos.z));
    let half_vec = normalize(sun_dir + view_dir);
    let spec_angle = max(dot(normal, half_vec), 0.0);
    
    // Specular power: smooth = sharp highlights, rough = broad/none
    let spec_power = mix(4.0, 96.0, smoothness * smoothness);
    let spec_strength = pow(spec_angle, spec_power) * smoothness * 0.8;
    
    // Subtle Fresnel effect - edges of glossy surfaces are slightly more reflective
    let fresnel = pow(1.0 - max(dot(normal, view_dir), 0.0), 4.0) * smoothness * 0.2;
    
    let specular = (spec_strength + fresnel) * sun_color * lighting.sun_intensity;
    
    // === SKY LIGHT (Hemisphere) ===
    // Warm blue fill light from above
    let sky_factor = (normal.y + 1.0) * 0.5; // 0 at bottom, 1 at top
    let sky_color = vec3<f32>(0.55, 0.7, 0.9); // Warm sky blue
    let sky_light = sky_factor * 0.32 * sky_color;
    
    // === GROUND BOUNCE ===
    // Subtle warm bounce light from below
    let ground_factor = (-normal.y + 1.0) * 0.5; // 0 at top, 1 at bottom
    let ground_color = vec3<f32>(0.45, 0.38, 0.28); // Warm earth tones
    let ground_light = ground_factor * 0.14 * ground_color;
    
    // === HEIGHT-BASED COLOR SHIFT ===
    // Higher areas get slightly cooler/bluer tint, lower areas warmer
    let height_norm = clamp((in.world_pos.y - 30.0) / 100.0, 0.0, 1.0);
    let height_tint = mix(vec3<f32>(1.02, 1.0, 0.96), vec3<f32>(0.97, 0.98, 1.03), height_norm);
    
    // === COMBINE LIGHTING ===
    let ambient_color = vec3<f32>(1.0, 0.98, 0.95); // Slightly warm ambient
    let ambient_light = lighting.ambient * ambient_color;
    
    // Total illumination with edge and face AO
    let total_ao = face_ao * edge_ao;
    let total_light = (ambient_light + sun_light + sky_light + ground_light) * total_ao;
    
    // Apply lighting to varied base color with height tint, then add specular
    var lit_color = varied_color * height_tint * total_light + specular;
    
    // === DISTANCE + HEIGHT ATMOSPHERIC HAZE ===
    // Distance fog creates depth perception, height fog adds atmosphere
    let haze_color = vec3<f32>(0.75, 0.82, 0.92); // Warm blue-gray haze
    
    // Distance from camera (horizontal plane)
    let dx = in.world_pos.x - lighting.eye_x;
    let dz = in.world_pos.z - lighting.eye_z;
    let dist = sqrt(dx * dx + dz * dz);
    
    // Distance fog - increases with distance from camera
    let fog_near = 0.0;   // Start fading at this distance
    let fog_far = 1000.0;   // Fully fogged at this distance
    let dist_fog = smoothstep(fog_near, fog_far, dist);
    
    // Height fog - lower areas have more haze
    let height_fog = 1.0 - smoothstep(0.0, 140.0, in.world_pos.y);
    
    // Combine: distance fog is primary, height fog adds extra at low areas
    let total_fog = dist_fog * 0.40 + height_fog * 0.12;
    let fog_amount = clamp(total_fog, 0.0, 0.7); // Cap max fog
    
    lit_color = mix(lit_color, haze_color * (ambient_light.x + 0.35), fog_amount);
    
    // === SATURATION BOOST ===
    // Slightly boost color saturation for vibrancy
    let luminance = dot(lit_color, vec3<f32>(0.299, 0.587, 0.114));
    let saturation_boost = 1.18;
    lit_color = mix(vec3<f32>(luminance), lit_color, saturation_boost);
    
    // Clamp to valid range
    lit_color = clamp(lit_color, vec3<f32>(0.0), vec3<f32>(1.0));
    
    return vec4<f32>(lit_color, in.color.w);
}
