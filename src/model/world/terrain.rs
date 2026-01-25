// terrain.rs - Complete terrain generation system
// Combines noise functions, density-based generation, biome detection, and surface generation
//
// ============================================================================
// COMPLETE TERRAIN GENERATION PIPELINE
// ============================================================================
// 
// The terrain is generated chunk-by-chunk using a comprehensive pipeline:
//
// STEP 1: Biome Determination (2D Noise)
//   → Uses 2D perlin noise to determine biome type from temperature & humidity
//   → Biomes: Tundra, Mountain, Forest, Desert, Beach, Plain, Ocean, Jungle, etc.
//   → Called by: get_biome_type()
//
// STEP 2: Terrain Height (Multi-layered 2D Noise)
//   → Continental noise: large-scale landmasses vs oceans
//   → Erosion noise: mountains vs valleys
//   → Ridge noise: dramatic mountain ridges
//   → Detail noise: small-scale terrain variation
//   → Creates diverse terrain: mountains, canyons, plateaus, valleys
//
// STEP 3: Cave System (3D Worm Caves + Cheese Caves)
//   → Worm caves: long, winding tunnels using 3D noise
//   → Cheese caves: large underground caverns
//   → Cave entrances on hillsides
//   → Depth-based cave frequency
//
// STEP 4: Water/Terrain Filling (Y-Level Checks)
//   → Below sea level: water
//   → Underground lakes in large caves
//   → Rivers following terrain depressions
//
// STEP 5: Forest System (Noise-based Clustering)
//   → Forest density noise determines tree clustering
//   → Natural clearings and dense forest areas
//   → Sophisticated tree structures with branches
//   → Tree variety based on biome and local conditions
//
// STEP 6: Clouds and Atmosphere
//   → 2D noise determines cloud coverage at high altitudes
//
// Result: Rich, varied terrain with forests, mountains, caves, and natural features!
//

use super::block::Block;
use super::chunk::CHUNK_SIZE;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Tree properties calculated from biome and 2D noise
struct TreeData {
    tree_type: TreeType,
    tree_height: i32,
    should_spawn: bool,
}

// ============================================================================
// NOISE FUNCTIONS
// ============================================================================

/// 2D Perlin Noise using gradient hash
fn noise2d(x: f32, z: f32) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let fx = x - ix as f32;
    let fz = z - iz as f32;
    
    // Hash function: converts 2D integer to pseudo-random [-1, 1]
    let hash = |x: i32, z: i32| -> f32 {
        let mut n = x.wrapping_mul(374761393).wrapping_add(z.wrapping_mul(668265263));
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        ((n ^ (n >> 16)) as u32 as f32 / 4294967296.0) * 2.0 - 1.0
    };
    
    // Fade curve: smooth interpolation
    let fade = |t: f32| t * t * (3.0 - 2.0 * t);
    let u = fade(fx);
    let v = fade(fz);
    
    // Sample 4 corner gradients and interpolate
    let a = hash(ix, iz);
    let b = hash(ix + 1, iz);
    let c = hash(ix, iz + 1);
    let d = hash(ix + 1, iz + 1);
    
    let x1 = a * (1.0 - u) + b * u;
    let x2 = c * (1.0 - u) + d * u;
    x1 * (1.0 - v) + x2 * v
}

/// 3D Noise by combining 2D slices at different Y levels
fn noise3d(x: f32, y: f32, z: f32) -> f32 {
    // Blend three 2D noise samples at different XZ offsets based on Y
    let n1 = noise2d(x * 0.5 + y * 0.3, z * 0.5 - y * 0.3);
    let n2 = noise2d(x * 0.7 - y * 0.2, z * 0.7 + y * 0.2);
    let n3 = noise2d(x * 0.3, z * 0.3);
    n1 * 0.5 + n2 * 0.3 + n3 * 0.2
}

/// 2D FBM (Fractional Brownian Motion): layered noise for detail
pub fn fbm(x: f32, z: f32, base_freq: f32, gain: f32, octaves: u32) -> f32 {
    let mut result = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = base_freq;
    let mut max_amplitude = 0.0;
    
    for _ in 0..octaves {
        result += noise2d(x * frequency, z * frequency) * amplitude;
        max_amplitude += amplitude;
        amplitude *= gain;
        frequency *= 2.0;
    }
    
    if max_amplitude > 0.0 { result / max_amplitude } else { 0.0 }
}

/// 3D FBM for terrain density calculation
pub fn fbm_3d(x: f32, y: f32, z: f32, base_freq: f32, gain: f32, octaves: u32) -> f32 {
    let mut result = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = base_freq;
    let mut max_amplitude = 0.0;
    
    for _ in 0..octaves {
        result += noise3d(x * frequency, y * frequency, z * frequency) * amplitude;
        max_amplitude += amplitude;
        amplitude *= gain;
        frequency *= 2.0;
    }
    
    if max_amplitude > 0.0 { result / max_amplitude } else { 0.0 }
}

// ============================================================================
// BIOME TYPES AND TREE GENERATION
// ============================================================================

#[derive(Clone, Copy, Debug)]
pub enum BiomeType {
    Ocean,
    Beach,
    Plain,
    Forest,
    Mountain,
    Tundra,
    Desert,
    Cliff,      // Steile Klippen mit Basalt
    Lake,       // Seen/Seen-Biom
    Jungle,     // Dschungel mit Acacia/DarkOak
}

#[derive(Clone, Copy, Debug)]
pub enum TreeType {
    Oak,
    Spruce,
    Birch,
    Acacia,     // Baum für Trockengebiete/Jungle
    DarkOak,    // Großer Baum
}

pub struct Tree {
    pub pos: (i32, i32),  // (x, z) in chunk
    pub tree_type: TreeType,
    pub trunk_height: i32,
}

// ============================================================================
// TERRAIN CONFIGURATION
// ============================================================================

/// Configuration for terrain generation parameters
/// 
/// Usage:
///   // Use default configuration
///   let gen = VoxelDensityGenerator::new();
///   
///   // Or customize:
///   let mut config = TerrainConfig::default();
///   config.tree_spawn_threshold = 0.2;  // Fewer trees
///   config.base_height = 30.0;           // Lower terrain
///   let gen = VoxelDensityGenerator::with_config(config);
#[derive(Clone, Copy, Debug)]
pub struct TerrainConfig {
    // Noise frequencies for terrain shape
    pub continentalness_freq: f32,
    pub erosion_freq: f32,
    pub temperature_freq: f32,
    pub humidity_freq: f32,
    pub base_3d_freq: f32,
    pub cave_freq: f32,
    
    // Height and density modulation
    pub base_height: f32,
    pub continental_height_amplitude: f32,
    pub erosion_height_amplitude: f32,
    pub y_gradient_scale: f32,
    pub base_3d_noise_strength: f32,
    
    // Cave generation
    pub cave_noise_min: f32,
    pub cave_noise_max: f32,
    
    // Tree generation
    pub tree_noise_frequency: f32,
    pub tree_spawn_threshold: f32,
    pub tree_height_variation: i32,
    
    // Lake generation
    pub lake_frequency: f32,
    pub lake_threshold: f32,
    
    // Cliff generation
    pub cliff_threshold: f32,
    pub cliff_steepness: f32,
    
    // Vegetation placement
    pub plant_frequency: f32,
    pub plant_density: f32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            // Noise frequencies - lower = larger features
            continentalness_freq: 0.006,    // Larger landmasses
            erosion_freq: 0.010,            // Smoother erosion
            temperature_freq: 0.004,        // Larger biome regions
            humidity_freq: 0.004,
            base_3d_freq: 0.028,
            cave_freq: 0.035,               // Slightly larger caves
            
            // Height parameters
            base_height: 50.0,              // Higher base for more terrain
            continental_height_amplitude: 100.0,  // More dramatic height variation
            erosion_height_amplitude: 50.0,
            y_gradient_scale: 80.0,
            base_3d_noise_strength: 0.40,
            
            // Cave parameters - improved for better cave systems
            cave_noise_min: -0.12,
            cave_noise_max: 0.18,
            
            // Tree parameters - adjusted for better forest clustering
            tree_noise_frequency: 0.15,     // Lower for larger forest patches
            tree_spawn_threshold: 0.1,      // Adjusted threshold
            tree_height_variation: 4,       // More height variation
            
            // Lake parameters
            lake_frequency: 0.35,
            lake_threshold: -0.5,
            
            // Cliff parameters
            cliff_threshold: 0.75,
            cliff_steepness: 2.0,
            
            // Plant parameters
            plant_frequency: 0.8,
            plant_density: 0.6,
        }
    }
}

// ============================================================================
// VOXEL DENSITY GENERATOR
// ============================================================================

pub struct VoxelDensityGenerator {
    pub config: TerrainConfig,
}

impl VoxelDensityGenerator {
    pub fn new() -> Self {
        Self {
            config: TerrainConfig::default(),
        }
    }
    
    pub fn with_config(config: TerrainConfig) -> Self {
        Self { config }
    }

    /// Calculate 3D density at position (x, y, z) - STEP 2 OF GENERATION PIPELINE
    /// 
    /// This function implements the core terrain generation with gravity:
    /// 1. Uses 2D FBM noise to determine continental shape (height above sea level)
    /// 2. Uses Y-gradient to create natural terrain with gravity (no floating blocks)
    /// 3. Adds 3D noise for surface detail and overhangs
    /// 4. CARVES CAVES by forcing air in certain noise ranges (STEP 3)
    /// 
    /// Returns a density value where:
    ///   > 0 = solid block
    ///   <= 0 = air/empty/caves
    pub fn calculate_density(&self, x: f32, y: f32, z: f32) -> f32 {
        // 1. Continentalness: determines mountain vs plateau heights
        let continentalness = fbm(x, z, self.config.continentalness_freq, 0.55, 4);
        // Range: -1 to 1

        // 2. Erosion: determines flatness vs jaggedness
        let erosion = fbm(x * 1.5, z * 1.5, self.config.erosion_freq, 0.55, 3);
        // Range: -1 to 1

        // 3. Temperature & Humidity for biome (calculated here for consistency, used in get_biome_type)
        let _temperature = fbm(x, z, self.config.temperature_freq, 0.55, 3);
        let _humidity = fbm(x + 5000.0, z - 5000.0, self.config.humidity_freq, 0.55, 3);

        // 4. Calculate terrain height baseline - gravity-based terrain
        let continental_height = continentalness * self.config.continental_height_amplitude;
        let erosion_height = erosion * self.config.erosion_height_amplitude;
        let base_height = continental_height + erosion_height + self.config.base_height;

        // 5. Y-gradient: density DECREASES as you go UP (gravity - no floating terrain!)
        let y_diff = y - base_height;
        let mut density = 0.5 - (y_diff / self.config.y_gradient_scale).clamp(-1.0, 1.0);

        // 6. Base 3D Noise: add surface distortion for overhangs and detail
        let base_3d = fbm_3d(x, y, z, self.config.base_3d_freq, 0.55, 3);
        density += base_3d * self.config.base_3d_noise_strength;

        // 7. STEP 3 - Cave carving: if cave noise is in narrow band, force air
        let cave_noise = fbm_3d(x, y, z, self.config.cave_freq, 0.55, 3);
        if cave_noise > self.config.cave_noise_min && cave_noise < self.config.cave_noise_max {
            return -1.0; // Force air (caves)
        }

        density
    }

    /// Determine biome type based on temperature, humidity, and height - STEP 1 OF GENERATION PIPELINE
    /// 
    /// Uses 2D noise to determine biome type from three factors:
    /// - Temperature (cold → hot)
    /// - Humidity (dry → wet)  
    /// - Height (elevation)
    /// 
    /// Results in biomes: Tundra, Mountain, Forest, Desert, Beach, Plain, Ocean, Lake, Cliff, Jungle
    pub fn get_biome_type(&self, x: f32, z: f32, y: f32) -> BiomeType {
        let temperature = fbm(x, z, self.config.temperature_freq, 0.55, 3);
        let humidity = fbm(x + 5000.0, z - 5000.0, self.config.humidity_freq, 0.55, 3);
        let continentalness = fbm(x, z, self.config.continentalness_freq, 0.55, 4);
        let erosion = fbm(x, z, self.config.erosion_freq, 0.55, 3);
        let lake_noise = fbm(x + 2000.0, z + 2000.0, self.config.lake_frequency, 0.55, 3);

        // Lakes: depressions with moderate-high humidity and low continentalness
        if lake_noise < self.config.lake_threshold && humidity > 0.3 && y < 30.0 {
            return BiomeType::Lake;
        }

        // Cliffs: high erosion and steep mountains
        if erosion > self.config.cliff_threshold && y > 60.0 && continentalness > 0.4 {
            return BiomeType::Cliff;
        }

        // Hot jungle - hot and very humid
        if temperature > 0.5 && humidity > 0.6 && continentalness > 0.1 {
            return BiomeType::Jungle;
        }

        // High mountains (snow-covered peaks)
        if y > 80.0 && continentalness > 0.3 {
            if temperature < -0.6 {
                return BiomeType::Tundra;
            } else {
                return BiomeType::Mountain;
            }
        }

        // Moderate elevation mountains
        if y > 50.0 && continentalness > 0.2 {
            return BiomeType::Mountain;
        }

        // Cold regions - tundra
        if temperature < -0.7 {
            return BiomeType::Tundra;
        }

        // Hot, dry regions - desert
        if temperature > 0.7 && humidity < -0.5 {
            return BiomeType::Desert;
        }

        // Wet regions - forest
        if humidity > 0.0 {
            return BiomeType::Forest;
        }

        // Coastal areas
        if continentalness < 0.0 && continentalness > -0.3 {
            return BiomeType::Beach;
        }

        // Default: grassland/plain
        BiomeType::Plain
    }

    /// Get surface block type based on biome and height
    pub fn get_surface_block_for_biome(
        &self,
        x: f32,
        z: f32,
        y: f32,
        biome: BiomeType,
    ) -> super::block::Block {
        use super::block::Block;

        match biome {
            BiomeType::Ocean => Block::Water,
            BiomeType::Beach => {
                // Beach/sand transition zone
                if y > 5.0 {
                    Block::Grass
                } else {
                    Block::Sand
                }
            }
            BiomeType::Plain => {
                // Grassland with some variation
                let variety = fbm(x * 0.3, z * 0.3, 0.01, 0.55, 2);
                if variety < -0.3 {
                    Block::Moss
                } else if variety < 0.3 {
                    Block::Grass
                } else {
                    Block::Dirt
                }
            }
            BiomeType::Forest => {
                // Forest floor - mostly grass and moss
                let variety = fbm(x * 0.3, z * 0.3, 0.01, 0.55, 2);
                if variety < 0.0 {
                    Block::Moss
                } else {
                    Block::Grass
                }
            }
            BiomeType::Mountain => {
                // Rocky peaks with bare stone at top, grassed slopes below
                if y > 100.0 {
                    // Bare rocky peak - variety of stone types
                    let variety = fbm(x * 0.4, z * 0.4, 0.02, 0.55, 2) as i32 % 3;
                    match variety {
                        0 => Block::Stone,
                        1 => Block::Granite,
                        _ => Block::Cobblestone,
                    }
                } else if y > 70.0 {
                    // Upper slopes - grass for trees
                    Block::Grass
                } else {
                    // Lower slopes - grass
                    Block::Grass
                }
            }
            BiomeType::Tundra => {
                // Frozen terrain - moss for trees, snow at peaks
                if y > 60.0 {
                    Block::Snow
                } else if y > 40.0 {
                    Block::Moss  // Allow trees on moss
                } else if y > 20.0 {
                    let variety = fbm(x * 0.3, z * 0.3, 0.01, 0.55, 2);
                    if variety < -0.2 {
                        Block::Snow
                    } else {
                        Block::Moss
                    }
                } else {
                    Block::Moss
                }
            }
            BiomeType::Desert => Block::Sand,
            BiomeType::Lake => {
                // Lake shores - sandy/grassy with water plants
                if y > 5.0 {
                    Block::Grass
                } else {
                    Block::Sand
                }
            }
            BiomeType::Cliff => {
                // Cliff faces - dark stone, mostly basalt
                let variety = fbm(x * 0.5, z * 0.5, 0.02, 0.55, 2);
                if variety > 0.5 {
                    Block::Basalt
                } else if variety > 0.0 {
                    Block::BlackStone
                } else {
                    Block::Stone
                }
            }
            BiomeType::Jungle => {
                // Jungle floor - grass and moss, very green
                let variety = fbm(x * 0.4, z * 0.4, 0.01, 0.55, 2);
                if variety < -0.1 {
                    Block::Moss
                } else {
                    Block::Grass
                }
            }
        }
    }

    /// Get subsurface block based on depth and biome
    pub fn get_subsurface_block(&self, x: f32, z: f32, y: f32, biome: BiomeType) -> super::block::Block {
        use super::block::Block;

        // Deep underground = stone
        if y < -20.0 {
            return Block::Bedrock;
        }

        match biome {
            BiomeType::Desert => {
                // Desert has sandstone layers
                if y < 40.0 {
                    Block::Sandstone
                } else {
                    Block::Sand
                }
            }
            BiomeType::Mountain | BiomeType::Tundra => {
                // Mountains: granite and stone
                let variety = fbm(x * 0.5, z * 0.5, 0.01, 0.55, 2) as i32 % 2;
                if variety == 0 {
                    Block::Granite
                } else {
                    Block::Stone
                }
            }
            _ => {
                // Default: dirt under grass, stone deeper
                if y > 0.0 {
                    Block::Dirt
                } else {
                    Block::Stone
                }
            }
        }
    }

    /// Get ore block if one should spawn here
    pub fn get_ore_block(&self, x: f32, y: f32, z: f32) -> Option<super::block::Block> {
        use super::block::Block;

        let ore_check = noise2d(
            x * 2.3 + y * 0.5,
            z * 1.7 - y * 0.3,
        );

        if ore_check > 0.80 && y < 60.0 && y > 20.0 {
            Some(Block::CoalOre)
        } else if ore_check < -0.85 && y < 40.0 && y > 0.0 {
            Some(Block::IronOre)
        } else if ore_check > 0.88 && y < 10.0 && y > -20.0 {
            Some(Block::GoldOre)
        } else if ore_check < -0.90 && y < -30.0 && y > -80.0 {
            Some(Block::DiamondOre)
        } else {
            None
        }
    }

    /// Calculate tree placement data for a column (type, height, whether to spawn)
    fn calculate_tree_data(&self, wx: f32, wz: f32) -> TreeData {
        // Determine biome at this location
        let biome = self.get_biome_type(wx, wz, 30.0);
        
        // Check if this is a tree center (using noise)
        let tree_location = noise2d(wx * self.config.tree_noise_frequency + 200.0, wz * self.config.tree_noise_frequency - 200.0);
        let should_spawn = tree_location > self.config.tree_spawn_threshold;
        
        // Generate random value for tree type/height variation
        let tree_chance = noise2d(wx * 0.2 + 200.0, wz * 0.2 - 200.0);
        let tree_rng = (tree_chance + 1.0) * 0.5;
        
        // Determine tree type based on biome
        let tree_type = match biome {
            BiomeType::Tundra => TreeType::Spruce,
            BiomeType::Forest => if tree_rng > 0.4 { TreeType::Birch } else { TreeType::Oak },
            BiomeType::Mountain => if tree_rng > 0.6 { TreeType::Spruce } else { TreeType::Oak },
            BiomeType::Jungle => if tree_rng > 0.5 { TreeType::DarkOak } else { TreeType::Acacia },
            BiomeType::Desert => TreeType::Acacia,
            BiomeType::Plain => TreeType::Oak,
            _ => TreeType::Oak,
        };
        
        // Calculate height based on tree type with slight variation
        let tree_height = match tree_type {
            TreeType::Spruce => 9 + ((tree_rng * 10.0) as i32 % self.config.tree_height_variation),
            TreeType::Birch => 7 + ((tree_rng * 10.0) as i32 % (self.config.tree_height_variation - 1).max(1)),
            TreeType::Oak => 6 + ((tree_rng * 10.0) as i32 % self.config.tree_height_variation),
            TreeType::Acacia => 8 + ((tree_rng * 10.0) as i32 % (self.config.tree_height_variation + 1)),
            TreeType::DarkOak => 12 + ((tree_rng * 10.0) as i32 % (self.config.tree_height_variation + 2)),
        };
        
        TreeData { tree_type, tree_height, should_spawn }
    }

    /// Populate a chunk with terrain and features using a complete generation pipeline:
    /// 
    /// GENERATION PIPELINE:
    /// 1. Use 2D noise to determine biome (Forest, Mountain, Plains, etc.)
    /// 2. Use 3D density to generate solid terrain with natural gravity
    /// 3. Carve out caves during density calculation
    /// 4. Fill depressions with water (y <= 0)
    /// 5. Place trees on surface blocks matching biome type
    /// 6. Add clouds at height 255
    pub fn populate_chunk(&self, chunk: &mut super::chunk::Chunk, chunk_coord: &crate::utils::ChunkCoord) {
        use crate::utils::BlockCoord;
            
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let world_coord = chunk_coord.to_world_coord();
                let wx = world_coord.0 as f32 + x as f32;
                let wz = world_coord.2 as f32 + z as f32;

                // Calculate tree placement and properties once per column (for efficiency)
                let tree_data = self.calculate_tree_data(wx, wz);

                // STEP 2-6: Process each Y level in this column
                for y in 0..CHUNK_SIZE {
                    let world_y = chunk_coord.1 * CHUNK_SIZE + y;
                    let wy = world_y as f32;
                    
                    // STEP 6: Add clouds at height 255
                    if world_y == 255 {
                        let cloud_noise = noise2d(wx * 0.04, wz * 0.04);
                        if cloud_noise > 0.0 {
                            chunk.set_block(&BlockCoord(x as usize, y as usize, z as usize), Block::Cloud, false);
                            continue;
                        }
                    }
                    
                    // STEP 2: Use 3D density to calculate terrain (includes cave carving)
                    let density = self.calculate_density(wx, wy, wz);
                    let is_solid = density > 0.0;
                    let is_surface = is_solid && self.calculate_density(wx, wy + 1.0, wz) <= 0.0;
                    
                    // STEP 3-5: Determine block type
                    let block = if !is_solid {
                        // STEP 4: Fill with water if below sea level (y <= 0)
                        if wy <= 0.0 { Block::Water } else { Block::Empty }
                    } else {
                        // Solid block: determine type based on biome and depth
                        let biome = self.get_biome_type(wx, wz, wy);
                        
                        if is_surface {
                            self.get_surface_block_for_biome(wx, wz, wy, biome)
                        } else {
                            // Check for ores, otherwise use default subsurface type
                            self.get_ore_block(wx, wy, wz)
                                .unwrap_or_else(|| self.get_subsurface_block(wx, wz, wy, biome))
                        }
                    };

                    chunk.set_block(&BlockCoord(x as usize, y as usize, z as usize), block, false);

                    // STEP 5: Plant trees on surface grass/moss blocks
                    if is_surface && tree_data.should_spawn && matches!(block, Block::Grass | Block::Moss) {
                        let tree = Tree {
                            pos: (x as i32, z as i32),
                            tree_type: tree_data.tree_type,
                            trunk_height: tree_data.tree_height,
                        };
                        Self::plant_tree(&tree, chunk_coord, world_y as i32 + 1, chunk);
                    }
                    
                    // Place vegetation (plants) on surface blocks
                    if is_surface && matches!(block, Block::Grass | Block::Moss) && world_y > 0 {
                        let plant_noise = noise2d(wx * self.config.plant_frequency + 100.0, wz * self.config.plant_frequency - 100.0);
                        let biome = self.get_biome_type(wx, wz, wy);
                        
                        // Only place plants if not tree-center and noise is above threshold
                        if !tree_data.should_spawn && plant_noise > self.config.plant_density {
                            let plant_type = match biome {
                                BiomeType::Forest | BiomeType::Jungle => {
                                    if plant_noise > 0.8 { Block::GrassTall } else { Block::GrassShort }
                                }
                                BiomeType::Desert => {
                                    if plant_noise > 0.9 { Block::Cactus } else { Block::DeadBush }
                                }
                                BiomeType::Lake | BiomeType::Beach => {
                                    Block::SeaGrass
                                }
                                BiomeType::Plain => {
                                    if plant_noise > 0.85 { Block::RedFlower } else { Block::YellowFlower }
                                }
                                _ => {
                                    if plant_noise > 0.85 { Block::RedFlower } else { Block::YellowFlower }
                                }
                            };
                            
                            // Place plant on top of surface block
                            if world_y < 255 {
                                let plant_y = y + 1;
                                if plant_y < CHUNK_SIZE {
                                    chunk.set_block(&BlockCoord(x as usize, plant_y as usize, z as usize), plant_type, false);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Populate a chunk with terrain - ENHANCED VERSION
    /// 
    /// Features:
    /// 1. Dramatic terrain with tall mountains and deep valleys
    /// 2. Snow-capped peaks
    /// 3. Ice regions (cold biomes)
    /// 4. Flowers on meadows
    /// 5. Cave entrances visible from surface
    /// 6. Multiple tree types with varied shapes
    pub fn populate_chunk_simple(&self, chunk: &mut super::chunk::Chunk, chunk_coord: &crate::utils::ChunkCoord) {
        use crate::utils::BlockCoord;
        
        // Sea level at y=32 (gives room for underwater terrain and beaches)
        const SEA_LEVEL: isize = 32;
        const SNOW_LINE: isize = 100;  // Snow starts here
        const PEAK_LINE: isize = 130;  // Rocky peaks above this
        
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                // World coordinates
                let wx = chunk_coord.0 * CHUNK_SIZE + lx;
                let wz = chunk_coord.2 * CHUNK_SIZE + lz;
                let fwx = wx as f32;
                let fwz = wz as f32;

                // ============================================
                // STEP 1: Calculate terrain height (DRAMATIC!)
                // ============================================
                
                // Base terrain
                let base_height = 50.0;
                
                // Large continental features
                let continental = Self::simple_noise(fwx * 0.003, fwz * 0.003);
                let continental_height = continental * 50.0; // -50 to +50
                
                // Rolling hills
                let large_noise = Self::simple_noise(fwx * 0.008, fwz * 0.008);
                let large_hills = large_noise * 35.0;
                
                // Medium variation
                let medium_noise = Self::simple_noise(fwx * 0.025, fwz * 0.025);
                let medium_hills = medium_noise * 18.0;
                
                // Small detail
                let small_noise = Self::simple_noise(fwx * 0.1, fwz * 0.1);
                let small_detail = small_noise * 6.0;
                
                // DRAMATIC MOUNTAINS - multiple noise layers for tall peaks
                let mountain_noise1 = Self::simple_noise(fwx * 0.006, fwz * 0.006);
                let mountain_noise2 = Self::simple_noise(fwx * 0.012 + 500.0, fwz * 0.012 + 500.0);
                let mountain_combined = (mountain_noise1 + mountain_noise2 * 0.5) / 1.5;
                
                let mountains = if mountain_combined > 0.2 {
                    let m = (mountain_combined - 0.2) * 1.25; // 0 to 1
                    let sharp = m * m * m; // Cubic for sharp peaks
                    sharp * 150.0 // Up to 150 extra height for tall mountains!
                } else {
                    0.0
                };
                
                // Ridge noise for extra drama
                let ridge = Self::ridge_noise_simple(fwx * 0.015, fwz * 0.015);
                let ridges = if mountain_combined > 0.1 {
                    ridge * 40.0 * mountain_combined
                } else {
                    0.0
                };
                
                // Valleys/canyons (negative mountains)
                let valley_noise = Self::simple_noise(fwx * 0.01 + 1000.0, fwz * 0.01 + 1000.0);
                let valleys = if valley_noise < -0.4 && continental > -0.3 {
                    let v = (-valley_noise - 0.4) * 1.67;
                    -v * v * 30.0 // Carve down into terrain
                } else {
                    0.0
                };
                
                // Combine for final height
                let height_f = base_height + continental_height + large_hills + medium_hills 
                              + small_detail + mountains + ridges + valleys;
                let terrain_height = (height_f as isize).clamp(5, 220);
                
                // ============================================
                // BIOME DETERMINATION (temperature based on position + height)
                // ============================================
                let temp_noise = Self::simple_noise(fwx * 0.004 + 2000.0, fwz * 0.004 + 2000.0);
                let base_temp = temp_noise; // -1 to 1
                // Colder at higher altitudes and certain regions
                let altitude_cooling = (terrain_height as f32 - 50.0) / 150.0;
                let temperature = base_temp - altitude_cooling;
                
                let is_cold_biome = temperature < -0.3;
                let is_frozen = temperature < -0.5;
                
                // ============================================
                // TREE DETERMINATION
                // ============================================
                let tree_noise = Self::simple_noise(fwx * 0.3 + 100.0, fwz * 0.3 + 100.0);
                let forest_density = Self::simple_noise(fwx * 0.02 + 300.0, fwz * 0.02 + 300.0);
                
                // Trees more likely in forests (high density areas)
                let tree_threshold = if forest_density > 0.3 { 0.3 } // Forest: lots of trees
                                     else if forest_density > 0.0 { 0.5 } // Scattered
                                     else { 0.7 }; // Sparse
                
                let wants_tree = tree_noise > tree_threshold 
                                && terrain_height > SEA_LEVEL + 3 
                                && terrain_height < SNOW_LINE - 10;
                
                // Determine tree type based on biome/temperature
                let tree_type_noise = Self::simple_noise(fwx * 0.5 + 500.0, fwz * 0.5 + 500.0);
                let tree_type = if is_cold_biome || terrain_height > 80 {
                    0 // Spruce in cold/mountain areas
                } else if tree_type_noise > 0.3 {
                    1 // Oak
                } else if tree_type_noise > -0.2 {
                    2 // Birch
                } else {
                    3 // Acacia
                };

                // ============================================
                // STEP 2: Fill the column
                // ============================================
                for ly in 0..CHUNK_SIZE {
                    let world_y = chunk_coord.1 * CHUNK_SIZE + ly;
                    let fwy = world_y as f32;
                    
                    // Cave check - can extend to surface for entrances!
                    let cave_depth_limit = if Self::is_cave_entrance(fwx, fwz) {
                        terrain_height // Allow caves to reach surface = entrance!
                    } else {
                        terrain_height - 4 // Normal: caves stay underground
                    };
                    
                    let is_cave = world_y < cave_depth_limit 
                                  && world_y > 5 
                                  && Self::is_cave(fwx, fwy, fwz);
                    
                    let block = if world_y >= terrain_height {
                        // Above terrain
                        if world_y < SEA_LEVEL {
                            if is_frozen {
                                Block::Ice // Frozen ocean/lake
                            } else {
                                Block::Water
                            }
                        } else {
                            Block::Empty
                        }
                    } else if is_cave {
                        // Cave interior
                        if world_y < SEA_LEVEL {
                            Block::Water // Flooded cave
                        } else {
                            Block::Empty
                        }
                    } else {
                        // Solid terrain
                        let depth = terrain_height - world_y;
                        
                        if depth == 1 {
                            // Surface block
                            if terrain_height < SEA_LEVEL - 5 {
                                if is_frozen { Block::Ice } else { Block::Sand }
                            } else if terrain_height < SEA_LEVEL + 4 {
                                Block::Sand // Beach
                            } else if terrain_height > PEAK_LINE {
                                Block::Snow // Snow-capped peaks
                            } else if terrain_height > SNOW_LINE {
                                // Mix of snow and stone on upper mountains
                                let snow_mix = Self::simple_noise(fwx * 0.2, fwz * 0.2);
                                if snow_mix > -0.2 { Block::Snow } else { Block::Stone }
                            } else if is_frozen {
                                // Frozen ground
                                let frozen_mix = Self::simple_noise(fwx * 0.15, fwz * 0.15);
                                if frozen_mix > 0.3 { Block::Snow }
                                else if frozen_mix > -0.3 { Block::Ice }
                                else { Block::Moss }
                            } else if is_cold_biome {
                                // Cold but not frozen - mostly moss/snow patches
                                let cold_mix = Self::simple_noise(fwx * 0.12, fwz * 0.12);
                                if cold_mix > 0.5 { Block::Snow } else { Block::Moss }
                            } else {
                                Block::Grass
                            }
                        } else if depth <= 4 {
                            // Subsurface
                            if terrain_height < SEA_LEVEL + 4 {
                                Block::Sand
                            } else if terrain_height > SNOW_LINE || is_frozen {
                                Block::Stone
                            } else {
                                Block::Dirt
                            }
                        } else if world_y < 8 {
                            Block::Bedrock
                        } else if world_y < 25 {
                            let g = Self::simple_noise(fwx * 0.1, fwz * 0.1 + fwy * 0.1);
                            if g > 0.5 { Block::Granite } else { Block::Stone }
                        } else {
                            // Stone with ores
                            let ore = Self::simple_noise(fwx * 0.3 + fwy, fwz * 0.3);
                            if ore > 0.88 && world_y < 60 {
                                Block::CoalOre
                            } else if ore < -0.88 && world_y < 45 {
                                Block::IronOre
                            } else if ore > 0.95 && world_y < 25 {
                                Block::GoldOre
                            } else if ore < -0.95 && world_y < 15 {
                                Block::DiamondOre
                            } else {
                                Block::Stone
                            }
                        }
                    };

                    chunk.set_block(&BlockCoord(lx as usize, ly as usize, lz as usize), block, false);

                    // ============================================
                    // 3D CLOUDS at height 200-204 (thinner, wispier)
                    // ============================================
                    const CLOUD_BASE: isize = 200;
                    const CLOUD_TOP: isize = 205;
                    if world_y >= CLOUD_BASE && world_y <= CLOUD_TOP {
                        // 2D base noise determines where clouds exist - use larger scale for patchier coverage
                        let cloud_base_noise = Self::simple_noise(fwx * 0.012, fwz * 0.012);
                        let cloud_detail = Self::simple_noise(fwx * 0.04 + 100.0, fwz * 0.04 + 100.0) * 0.2;
                        
                        // Higher threshold = sparser clouds
                        if cloud_base_noise + cloud_detail > 0.25 {
                            // Cloud density decreases toward edges (both horizontal and vertical)
                            let cloud_strength = cloud_base_noise + cloud_detail - 0.25;
                            
                            // Vertical shape: clouds are thicker in the middle
                            let cloud_mid = (CLOUD_BASE + CLOUD_TOP) / 2;
                            let y_dist = (world_y - cloud_mid).abs() as f32;
                            let max_y_dist = ((CLOUD_TOP - CLOUD_BASE) / 2) as f32;
                            let y_factor = 1.0 - (y_dist / max_y_dist);
                            
                            // 3D noise for puffy cloud shape - more variation
                            let cloud_3d = Self::simple_noise(fwx * 0.1 + fwy * 0.15, fwz * 0.1 - fwy * 0.15);
                            let cloud_3d_detail = Self::simple_noise(fwx * 0.2 + 50.0, fwz * 0.2 + fwy * 0.25) * 0.3;
                            
                            // Combine: need base noise + good y position + 3D shape (higher threshold = wispier)
                            let threshold = 0.4 - (cloud_strength * 0.3) - (y_factor * 0.25);
                            if cloud_3d + cloud_3d_detail > threshold {
                                chunk.set_block(&BlockCoord(lx as usize, ly as usize, lz as usize), Block::Cloud, false);
                            }
                        }
                    }

                    // ============================================
                    // Place trees and decorations on surface
                    // ============================================
                    if world_y == terrain_height - 1 {
                        // Trees on grass or moss
                        if (block == Block::Grass || block == Block::Moss) && wants_tree {
                            Self::place_fancy_tree(chunk, chunk_coord, lx, ly + 1, lz, world_y + 1, tree_type);
                        }
                        // Flowers on grass (not where trees are)
                        else if block == Block::Grass && !wants_tree {
                            let flower_noise = Self::simple_noise(fwx * 0.8 + 200.0, fwz * 0.8 + 200.0);
                            if flower_noise > 0.6 {
                                let flower_type_noise = Self::simple_noise(fwx * 1.5, fwz * 1.5);
                                let flower = if flower_type_noise > 0.3 {
                                    Block::RedFlower
                                } else if flower_type_noise > -0.3 {
                                    Block::YellowFlower
                                } else {
                                    Block::GrassTall
                                };
                                let fy = ly + 1;
                                if fy >= 0 && fy < CHUNK_SIZE {
                                    chunk.set_block(&BlockCoord(lx as usize, fy as usize, lz as usize), flower, false);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Very simple noise function - just uses sine waves
    fn simple_noise(x: f32, z: f32) -> f32 {
        let n1 = (x * 1.0).sin() * (z * 1.0).cos();
        let n2 = (x * 0.5 + 10.0).sin() * (z * 0.7 + 20.0).sin();
        let n3 = (x * 1.3 - 5.0).cos() * (z * 1.1 + 15.0).cos();
        (n1 + n2 + n3) / 3.0
    }
    
    /// Ridge noise for sharp mountain features
    fn ridge_noise_simple(x: f32, z: f32) -> f32 {
        let n = Self::simple_noise(x, z);
        1.0 - n.abs() * 2.0 // Invert absolute value for ridges
    }
    
    /// Simple cave check using 3D sine waves
    fn is_cave(x: f32, y: f32, z: f32) -> bool {
        // Main worm caves
        let n1 = (x * 0.08).sin() * (y * 0.1).cos() * (z * 0.08).sin();
        let n2 = (x * 0.12 + 50.0).cos() * (y * 0.09).sin() * (z * 0.11 + 30.0).cos();
        let worm = (n1 + n2) / 2.0;
        
        // Larger cheese caves
        let c1 = (x * 0.04).sin() * (y * 0.05).cos() * (z * 0.04).sin();
        let c2 = (x * 0.06 + 100.0).cos() * (y * 0.07 + 50.0).sin() * (z * 0.05).cos();
        let cheese = (c1 + c2) / 2.0;
        
        // Cave if either worm tunnel or cheese cavern
        worm.abs() < 0.07 || (cheese > 0.3 && y < 50.0)
    }
    
    /// Check if this XZ position should have a cave entrance
    fn is_cave_entrance(x: f32, z: f32) -> bool {
        let entrance_noise = Self::simple_noise(x * 0.05 + 777.0, z * 0.05 + 777.0);
        entrance_noise > 0.7 // Rare cave entrances
    }
    
    /// Place a fancy tree with varied shape based on type
    /// type: 0=Spruce, 1=Oak, 2=Birch, 3=Acacia
    fn place_fancy_tree(chunk: &mut super::chunk::Chunk, chunk_coord: &crate::utils::ChunkCoord, 
                        lx: isize, ly: isize, lz: isize, world_y: isize, tree_type: i32) {
        
        // Random variation based on position
        let variation = ((world_y + lx * 7 + lz * 13) % 5) as i32;
        
        match tree_type {
            0 => Self::place_spruce_tree(chunk, chunk_coord, lx, ly, lz, variation),
            1 => Self::place_oak_tree(chunk, chunk_coord, lx, ly, lz, variation),
            2 => Self::place_birch_tree(chunk, chunk_coord, lx, ly, lz, variation),
            3 => Self::place_acacia_tree(chunk, chunk_coord, lx, ly, lz, variation),
            _ => Self::place_oak_tree(chunk, chunk_coord, lx, ly, lz, variation),
        }
    }
    
    /// Spruce tree - tall, conical, with slight lean
    fn place_spruce_tree(chunk: &mut super::chunk::Chunk, _chunk_coord: &crate::utils::ChunkCoord,
                         lx: isize, ly: isize, lz: isize, variation: i32) {
        use crate::utils::BlockCoord;
        
        // Trees: 10-14 blocks tall (limited by chunk height)
        let max_height = (CHUNK_SIZE - ly - 2).max(0) as i32; // Leave room for top
        let trunk_height = (10 + variation).min(max_height).max(5);
        
        // Slight lean for natural look
        let lean_dir = variation % 4;
        let (lean_x, lean_z): (isize, isize) = match lean_dir {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };
        let has_lean = variation > 1;
        
        let mut cx = lx;
        let mut cz = lz;
        
        // Trunk with possible lean
        for t in 0..trunk_height {
            let ty = ly + t as isize;
            if ty >= 0 && ty < CHUNK_SIZE && cx >= 0 && cx < CHUNK_SIZE && cz >= 0 && cz < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(cx as usize, ty as usize, cz as usize), Block::SpruceWood, true);
            }
            // Lean every 5 blocks
            if has_lean && t > 3 && t % 5 == 0 && t < trunk_height - 3 {
                let new_cx = cx + lean_x;
                let new_cz = cz + lean_z;
                if new_cx >= 0 && new_cx < CHUNK_SIZE && new_cz >= 0 && new_cz < CHUNK_SIZE {
                    cx = new_cx;
                    cz = new_cz;
                }
            }
        }
        
        // Conical leaves - wider for bigger trees
        let leaf_start = 3;
        for layer in 0..(trunk_height - leaf_start) {
            let ty = ly + leaf_start as isize + layer as isize;
            if ty < 0 || ty >= CHUNK_SIZE { continue; }
            
            let layer_from_top = trunk_height - leaf_start - layer - 1;
            let radius = (layer_from_top / 3).min(4).max(0);
            
            for dx in -(radius as isize)..=(radius as isize) {
                for dz in -(radius as isize)..=(radius as isize) {
                    let dist = (dx.abs() + dz.abs()) as i32;
                    if dist <= radius + 1 {
                        let tx = cx + dx;
                        let tz = cz + dz;
                        if tx >= 0 && tx < CHUNK_SIZE && tz >= 0 && tz < CHUNK_SIZE {
                            if !(dx == 0 && dz == 0) {
                                chunk.set_block(&BlockCoord(tx as usize, ty as usize, tz as usize), Block::SpruceLeaves, false);
                            }
                        }
                    }
                }
            }
        }
        // Pointed top
        let top_y = ly + trunk_height as isize;
        if top_y >= 0 && top_y < CHUNK_SIZE && cx >= 0 && cx < CHUNK_SIZE && cz >= 0 && cz < CHUNK_SIZE {
            chunk.set_block(&BlockCoord(cx as usize, top_y as usize, cz as usize), Block::SpruceLeaves, false);
        }
        let top_y2 = top_y + 1;
        if top_y2 >= 0 && top_y2 < CHUNK_SIZE && cx >= 0 && cx < CHUNK_SIZE && cz >= 0 && cz < CHUNK_SIZE {
            chunk.set_block(&BlockCoord(cx as usize, top_y2 as usize, cz as usize), Block::SpruceLeaves, false);
        }
    }
    
    /// Oak tree - large, curved/twisted trunk, big round canopy
    fn place_oak_tree(chunk: &mut super::chunk::Chunk, _chunk_coord: &crate::utils::ChunkCoord,
                      lx: isize, ly: isize, lz: isize, variation: i32) {
        use crate::utils::BlockCoord;
        
        // Trees: 7-11 blocks tall (limited by chunk height)
        let max_height = (CHUNK_SIZE - ly - 3).max(0) as i32; // Leave room for canopy
        let trunk_height = (7 + (variation % 3) * 2).min(max_height).max(5);
        
        // Trunk with multiple curves for natural twisted look
        let mut cx = lx;
        let mut cz = lz;
        let curve_dir1 = variation % 4;
        let curve_dir2 = (variation + 2) % 4;
        
        for t in 0..trunk_height {
            let ty = ly + t as isize;
            if ty >= 0 && ty < CHUNK_SIZE && cx >= 0 && cx < CHUNK_SIZE && cz >= 0 && cz < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(cx as usize, ty as usize, cz as usize), Block::Wood, true);
            }
            
            // First curve at 1/3 height
            if t == trunk_height / 3 {
                match curve_dir1 {
                    0 if cx + 1 < CHUNK_SIZE => cx += 1,
                    1 if cx > 0 => cx -= 1,
                    2 if cz + 1 < CHUNK_SIZE => cz += 1,
                    3 if cz > 0 => cz -= 1,
                    _ => {}
                }
            }
            // Second curve at 2/3 height (different direction)
            if t == trunk_height * 2 / 3 {
                match curve_dir2 {
                    0 if cx + 1 < CHUNK_SIZE => cx += 1,
                    1 if cx > 0 => cx -= 1,
                    2 if cz + 1 < CHUNK_SIZE => cz += 1,
                    3 if cz > 0 => cz -= 1,
                    _ => {}
                }
            }
        }
        
        // Multiple branches at different heights
        let branch_heights = [trunk_height / 2, trunk_height * 2 / 3, trunk_height - 2];
        for (i, &bh) in branch_heights.iter().enumerate() {
            let branch_y = ly + bh as isize;
            if branch_y >= 0 && branch_y < CHUNK_SIZE {
                let dir = (variation as usize + i) % 4;
                let (bx, bz): (isize, isize) = match dir {
                    0 => (1, 0),
                    1 => (-1, 0),
                    2 => (0, 1),
                    _ => (0, -1),
                };
                // Branch extends 1-2 blocks
                let tx = cx + bx;
                let tz = cz + bz;
                if tx >= 0 && tx < CHUNK_SIZE && tz >= 0 && tz < CHUNK_SIZE {
                    chunk.set_block(&BlockCoord(tx as usize, branch_y as usize, tz as usize), Block::Wood, true);
                    // Extended branch
                    let tx2 = tx + bx;
                    let tz2 = tz + bz;
                    let branch_y2 = branch_y + 1;
                    if tx2 >= 0 && tx2 < CHUNK_SIZE && tz2 >= 0 && tz2 < CHUNK_SIZE && branch_y2 >= 0 && branch_y2 < CHUNK_SIZE && i > 0 {
                        chunk.set_block(&BlockCoord(tx2 as usize, branch_y2 as usize, tz2 as usize), Block::Wood, true);
                    }
                }
            }
        }
        
        // Large round canopy
        let canopy_base = ly + (trunk_height - 4) as isize;
        for dy in 0isize..6 {
            let ty = canopy_base + dy;
            if ty < 0 || ty >= CHUNK_SIZE { continue; }
            
            // Bigger canopy: radius 3-4
            let radius = if dy == 0 || dy == 5 { 3 } else { 4 };
            for dx in -(radius as isize)..=(radius as isize) {
                for dz in -(radius as isize)..=(radius as isize) {
                    let dist_sq = dx * dx + dz * dz;
                    if dist_sq <= (radius * radius) as isize + 2 {
                        let tx = cx + dx;
                        let tz = cz + dz;
                        if tx >= 0 && tx < CHUNK_SIZE && tz >= 0 && tz < CHUNK_SIZE {
                            let skip = dist_sq > ((radius - 1) * (radius - 1)) as isize 
                                      && ((tx + tz + ty) % 5) == 0;
                            if !skip && !(dx == 0 && dz == 0 && dy < 3) {
                                chunk.set_block(&BlockCoord(tx as usize, ty as usize, tz as usize), Block::OakLeaves, false);
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Birch tree - tall, slender, elegant with gentle sway
    fn place_birch_tree(chunk: &mut super::chunk::Chunk, _chunk_coord: &crate::utils::ChunkCoord,
                        lx: isize, ly: isize, lz: isize, variation: i32) {
        use crate::utils::BlockCoord;
        
        // Trees: 8-12 blocks tall (limited by chunk height)
        let max_height = (CHUNK_SIZE - ly - 3).max(0) as i32; // Leave room for canopy
        let trunk_height = (8 + (variation % 3) * 2).min(max_height).max(5);
        
        // Gentle sway - birch can bend slightly
        let sway_dir = variation % 4;
        let (sway_x, sway_z): (isize, isize) = match sway_dir {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };
        
        let mut cx = lx;
        let mut cz = lz;
        
        // Slender trunk with gentle curve
        for t in 0..trunk_height {
            let ty = ly + t as isize;
            if ty >= 0 && ty < CHUNK_SIZE && cx >= 0 && cx < CHUNK_SIZE && cz >= 0 && cz < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(cx as usize, ty as usize, cz as usize), Block::BirchWood, true);
            }
            // Gentle sway in upper half
            if t == trunk_height * 2 / 3 && variation > 2 {
                let new_cx = cx + sway_x;
                let new_cz = cz + sway_z;
                if new_cx >= 0 && new_cx < CHUNK_SIZE && new_cz >= 0 && new_cz < CHUNK_SIZE {
                    cx = new_cx;
                    cz = new_cz;
                }
            }
        }
        
        // Taller, airier canopy
        let canopy_base = ly + (trunk_height - 5) as isize;
        for dy in 0isize..6 {
            let ty = canopy_base + dy;
            if ty < 0 || ty >= CHUNK_SIZE { continue; }
            
            let radius = if dy == 0 || dy == 5 { 2 } else { 3 };
            for dx in -(radius as isize)..=(radius as isize) {
                for dz in -(radius as isize)..=(radius as isize) {
                    let dist_sq = dx * dx + dz * dz;
                    if dist_sq <= (radius * radius) as isize + 1 {
                        let tx = cx + dx;
                        let tz = cz + dz;
                        if tx >= 0 && tx < CHUNK_SIZE && tz >= 0 && tz < CHUNK_SIZE {
                            // Very sparse for airy look
                            let skip = ((tx + tz + ty) % 3) == 0;
                            if !skip && !(dx == 0 && dz == 0 && dy < 4) {
                                chunk.set_block(&BlockCoord(tx as usize, ty as usize, tz as usize), Block::BirchLeaves, false);
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Acacia tree - dramatically angled trunk, wide flat canopy
    fn place_acacia_tree(chunk: &mut super::chunk::Chunk, _chunk_coord: &crate::utils::ChunkCoord,
                         lx: isize, ly: isize, lz: isize, variation: i32) {
        use crate::utils::BlockCoord;
        
        // Trees: 7-10 blocks tall (limited by chunk height)
        let max_height = (CHUNK_SIZE - ly - 3).max(0) as i32; // Leave room for canopy
        let trunk_height = (7 + (variation % 2) * 2).min(max_height).max(5);
        
        // Angled trunk
        let mut cx = lx;
        let mut cz = lz;
        let lean_dir = variation % 4;
        let (lean_x, lean_z): (isize, isize) = match lean_dir {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };
        
        for t in 0..trunk_height {
            let ty = ly + t as isize;
            if ty >= 0 && ty < CHUNK_SIZE && cx >= 0 && cx < CHUNK_SIZE && cz >= 0 && cz < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(cx as usize, ty as usize, cz as usize), Block::AcaciaWood, true);
            }
            
            // Lean every 2 blocks
            if t > 0 && t % 2 == 0 && t < trunk_height - 1 {
                let new_cx = cx + lean_x;
                let new_cz = cz + lean_z;
                if new_cx >= 0 && new_cx < CHUNK_SIZE && new_cz >= 0 && new_cz < CHUNK_SIZE {
                    cx = new_cx;
                    cz = new_cz;
                }
            }
        }
        
        // Forked branches at top - more dramatic forks for bigger tree
        let fork_y = ly + trunk_height as isize - 1;
        if fork_y >= 0 && fork_y < CHUNK_SIZE {
            // Main branch continues further
            let bx1 = cx + lean_x;
            let bz1 = cz + lean_z;
            if bx1 >= 0 && bx1 < CHUNK_SIZE && bz1 >= 0 && bz1 < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(bx1 as usize, fork_y as usize, bz1 as usize), Block::AcaciaWood, true);
                // Extend further
                let bx1e = bx1 + lean_x;
                let bz1e = bz1 + lean_z;
                if bx1e >= 0 && bx1e < CHUNK_SIZE && bz1e >= 0 && bz1e < CHUNK_SIZE {
                    let fy_up = fork_y + 1;
                    if fy_up >= 0 && fy_up < CHUNK_SIZE {
                        chunk.set_block(&BlockCoord(bx1e as usize, fy_up as usize, bz1e as usize), Block::AcaciaWood, true);
                    }
                }
            }
            // Perpendicular branch - also extends
            let bx2 = cx + lean_z;
            let bz2 = cz - lean_x;
            if bx2 >= 0 && bx2 < CHUNK_SIZE && bz2 >= 0 && bz2 < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(bx2 as usize, fork_y as usize, bz2 as usize), Block::AcaciaWood, true);
                let bx2e = bx2 + lean_z;
                let bz2e = bz2 - lean_x;
                if bx2e >= 0 && bx2e < CHUNK_SIZE && bz2e >= 0 && bz2e < CHUNK_SIZE {
                    chunk.set_block(&BlockCoord(bx2e as usize, fork_y as usize, bz2e as usize), Block::AcaciaWood, true);
                }
            }
            // Third branch - opposite
            let bx3 = cx - lean_x;
            let bz3 = cz - lean_z;
            if bx3 >= 0 && bx3 < CHUNK_SIZE && bz3 >= 0 && bz3 < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(bx3 as usize, fork_y as usize, bz3 as usize), Block::AcaciaWood, true);
            }
        }
        
        // Flat, wide canopy (bigger for taller tree)
        let canopy_y = ly + trunk_height as isize;
        for dy in 0isize..3 {
            let ty = canopy_y + dy;
            if ty < 0 || ty >= CHUNK_SIZE { continue; }
            
            let radius = if dy == 0 { 5 } else if dy == 1 { 4 } else { 3 };
            for dx in -(radius as isize)..=(radius as isize) {
                for dz in -(radius as isize)..=(radius as isize) {
                    let dist_sq = dx * dx + dz * dz;
                    if dist_sq <= (radius * radius) as isize + 2 {
                        let tx = cx + dx;
                        let tz = cz + dz;
                        if tx >= 0 && tx < CHUNK_SIZE && tz >= 0 && tz < CHUNK_SIZE {
                            // Add some gaps for natural look
                            let skip = dist_sq > ((radius - 1) * (radius - 1)) as isize && ((tx + tz) % 4) == 0;
                            if !skip {
                                chunk.set_block(&BlockCoord(tx as usize, ty as usize, tz as usize), Block::AcaciaLeaves, false);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Plant a tree of given type at specified location
    fn plant_tree(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, height: i32, chunk: &mut super::chunk::Chunk) {
        
        let (x, z) = tree.pos;
        match tree.tree_type {
            TreeType::Oak => Self::plant_oak(tree, chunk_coord, x, z, height, chunk),
            TreeType::Spruce => Self::plant_spruce(tree, chunk_coord, x, z, height, chunk),
            TreeType::Birch => Self::plant_birch(tree, chunk_coord, x, z, height, chunk),
            TreeType::Acacia => Self::plant_acacia(tree, chunk_coord, x, z, height, chunk),
            TreeType::DarkOak => Self::plant_darkoak(tree, chunk_coord, x, z, height, chunk),
        }
    }

    /// Helper to safely place a block within chunk bounds
    fn safe_set_block(chunk: &mut super::chunk::Chunk, chunk_coord: &crate::utils::ChunkCoord, 
                      wx: i32, wy: i32, wz: i32, block: Block) {
        use crate::utils::BlockCoord;
        const CHUNK_SIZE_I32: i32 = CHUNK_SIZE as i32;
        
        // Convert world coords to chunk-local coords
        let chunk_world_x = chunk_coord.0 as i32 * CHUNK_SIZE_I32;
        let chunk_world_y = chunk_coord.1 as i32 * CHUNK_SIZE_I32;
        let chunk_world_z = chunk_coord.2 as i32 * CHUNK_SIZE_I32;
        
        let lx = wx - chunk_world_x;
        let ly = wy - chunk_world_y;
        let lz = wz - chunk_world_z;
        
        if lx >= 0 && lx < CHUNK_SIZE_I32 && 
           ly >= 0 && ly < CHUNK_SIZE_I32 && 
           lz >= 0 && lz < CHUNK_SIZE_I32 {
            chunk.set_block(&BlockCoord(lx as usize, ly as usize, lz as usize), block, false);
        }
    }

    /// Plant an Oak tree: Natural oak with thick trunk, branches, and layered canopy
    fn plant_oak(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        let trunk_h = tree.trunk_height;
        let chunk_world_x = chunk_coord.0 as i32 * CHUNK_SIZE as i32;
        let chunk_world_z = chunk_coord.2 as i32 * CHUNK_SIZE as i32;
        let wx = chunk_world_x + x;
        let wz = chunk_world_z + z;
        
        // Main trunk
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            Self::safe_set_block(chunk, chunk_coord, wx, wy, wz, Block::Wood);
        }
        
        // Add branches at ~60% height - two branches going opposite directions
        let branch_height = world_y + (trunk_h * 3 / 5);
        let branch_dir = ((wx + wz) % 4) as i32; // Pseudo-random direction based on position
        
        // Branch 1
        let (bx1, bz1) = match branch_dir {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };
        Self::safe_set_block(chunk, chunk_coord, wx + bx1, branch_height, wz + bz1, Block::Wood);
        Self::safe_set_block(chunk, chunk_coord, wx + bx1 * 2, branch_height + 1, wz + bz1 * 2, Block::Wood);
        
        // Branch 2 (opposite direction)
        Self::safe_set_block(chunk, chunk_coord, wx - bx1, branch_height + 1, wz - bz1, Block::Wood);
        
        // Layered canopy - multiple layers with varying radii for natural look
        let canopy_base = world_y + trunk_h - 3;
        let canopy_layers = [
            (0, 3),  // Bottom layer: y offset, radius
            (1, 3),  // 
            (2, 2),  // Middle layers
            (3, 2),  //
            (4, 1),  // Top layer
        ];
        
        for (y_offset, radius) in canopy_layers.iter() {
            let wy = canopy_base + *y_offset;
            for lx in -(*radius as i32)..=(*radius as i32) {
                for lz in -(*radius as i32)..=(*radius as i32) {
                    let dist_sq = lx * lx + lz * lz;
                    let max_dist = radius * radius + 1;
                    
                    // Create rounded canopy with some gaps for natural look
                    if dist_sq <= max_dist as i32 {
                        // Add some irregularity - skip some outer leaves
                        let skip = ((wx + lx + wz + lz + wy) % 5) == 0 && dist_sq > (max_dist / 2) as i32;
                        if !skip {
                            Self::safe_set_block(chunk, chunk_coord, wx + lx, wy, wz + lz, Block::OakLeaves);
                        }
                    }
                }
            }
        }
        
        // Add some hanging leaves below branches
        for bx in -1..=1 {
            for bz in -1..=1 {
                if ((wx + bx + wz + bz) % 3) == 0 {
                    Self::safe_set_block(chunk, chunk_coord, wx + bx, canopy_base - 1, wz + bz, Block::OakLeaves);
                }
            }
        }
    }

    /// Plant a Spruce tree: Tall conical tree with tiered branches
    fn plant_spruce(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        let trunk_h = tree.trunk_height;
        let chunk_world_x = chunk_coord.0 as i32 * CHUNK_SIZE as i32;
        let chunk_world_z = chunk_coord.2 as i32 * CHUNK_SIZE as i32;
        let wx = chunk_world_x + x;
        let wz = chunk_world_z + z;
        
        // Tall straight trunk
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            Self::safe_set_block(chunk, chunk_coord, wx, wy, wz, Block::SpruceWood);
        }
        
        // Conical foliage with multiple tiers
        // Start narrow at top, widen toward bottom
        let foliage_start = trunk_h / 3; // Start foliage 1/3 up the trunk
        let foliage_height = trunk_h - foliage_start;
        
        for tier in 0..foliage_height {
            let wy = world_y + foliage_start + tier;
            let tier_from_top = foliage_height - tier - 1;
            
            // Radius increases toward bottom, with periodic narrowing for tiered look
            let base_radius = (tier_from_top / 2).min(3);
            let is_narrow_tier = tier % 2 == 1;
            let radius = if is_narrow_tier { (base_radius as i32 - 1).max(0) } else { base_radius as i32 };
            
            for lx in -radius..=radius {
                for lz in -radius..=radius {
                    let dist_sq = lx * lx + lz * lz;
                    
                    // Circular shape with diamond pattern for spruces
                    if dist_sq <= radius * radius + 1 {
                        // Skip center on branch tiers to show trunk
                        if !(lx == 0 && lz == 0) || tier == foliage_height - 1 {
                            // Add some droop to outer leaves
                            let droop = if dist_sq == radius * radius && !is_narrow_tier { -1 } else { 0 };
                            Self::safe_set_block(chunk, chunk_coord, wx + lx, wy + droop, wz + lz, Block::SpruceLeaves);
                        }
                    }
                }
            }
        }
        
        // Pointed top
        Self::safe_set_block(chunk, chunk_coord, wx, world_y + trunk_h, wz, Block::SpruceLeaves);
        Self::safe_set_block(chunk, chunk_coord, wx, world_y + trunk_h + 1, wz, Block::SpruceLeaves);
    }

    /// Plant a Birch tree: Elegant tree with slender trunk and airy canopy
    fn plant_birch(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        let trunk_h = tree.trunk_height;
        let chunk_world_x = chunk_coord.0 as i32 * CHUNK_SIZE as i32;
        let chunk_world_z = chunk_coord.2 as i32 * CHUNK_SIZE as i32;
        let wx = chunk_world_x + x;
        let wz = chunk_world_z + z;
        
        // Slender trunk - birch trees are thin
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            Self::safe_set_block(chunk, chunk_coord, wx, wy, wz, Block::BirchWood);
        }
        
        // Small branches near top
        let branch_y = world_y + trunk_h - 2;
        for dir in 0..4 {
            let (bx, bz) = match dir {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };
            if ((wx + wz + dir) % 2) == 0 {
                Self::safe_set_block(chunk, chunk_coord, wx + bx, branch_y, wz + bz, Block::BirchWood);
            }
        }
        
        // Light, airy canopy - birches have less dense foliage
        let canopy_base = world_y + trunk_h - 3;
        
        // Multiple small clusters instead of one solid mass
        for (cx, cz, cy_off) in [(0i32, 0i32, 2i32), (1, 0, 1), (-1, 0, 1), (0, 1, 1), (0, -1, 1)] {
            let cluster_y = canopy_base + cy_off;
            for ly in 0..3 {
                let wy = cluster_y + ly;
                let radius = if ly == 1 { 2 } else { 1 };
                
                for lx in -radius..=radius {
                    for lz in -radius..=radius {
                        let dist_sq = lx * lx + lz * lz;
                        if dist_sq <= radius * radius {
                            // Sparse leaves - skip some for airy look
                            if ((wx + cx + lx + wz + cz + lz + wy) % 3) != 0 {
                                Self::safe_set_block(chunk, chunk_coord, 
                                    wx + cx + lx, wy, wz + cz + lz, Block::BirchLeaves);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Plant an Acacia tree: Distinctive flat-topped tree with angled trunk
    fn plant_acacia(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        let trunk_h = tree.trunk_height;
        let chunk_world_x = chunk_coord.0 as i32 * CHUNK_SIZE as i32;
        let chunk_world_z = chunk_coord.2 as i32 * CHUNK_SIZE as i32;
        let wx = chunk_world_x + x;
        let wz = chunk_world_z + z;
        
        // Acacia has a distinctive angled trunk
        let lean_dir = ((wx + wz) % 4) as i32;
        let (lean_x, lean_z) = match lean_dir {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };
        
        // Angled trunk
        let mut curr_x = wx;
        let mut curr_z = wz;
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            Self::safe_set_block(chunk, chunk_coord, curr_x, wy, curr_z, Block::AcaciaWood);
            
            // Lean every 3 blocks
            if ty > 0 && ty % 3 == 0 && ty < trunk_h - 2 {
                curr_x += lean_x;
                curr_z += lean_z;
            }
        }
        
        // Forked top - two branches going different directions
        let fork_y = world_y + trunk_h - 1;
        
        // Main branch continues in lean direction
        Self::safe_set_block(chunk, chunk_coord, curr_x + lean_x, fork_y, curr_z + lean_z, Block::AcaciaWood);
        Self::safe_set_block(chunk, chunk_coord, curr_x + lean_x * 2, fork_y + 1, curr_z + lean_z * 2, Block::AcaciaWood);
        
        // Secondary branch goes perpendicular
        let (perp_x, perp_z) = (lean_z, -lean_x);
        Self::safe_set_block(chunk, chunk_coord, curr_x + perp_x, fork_y, curr_z + perp_z, Block::AcaciaWood);
        
        // Flat, wide canopy - characteristic of acacias
        let canopy_centers = [
            (curr_x + lean_x * 2, fork_y + 2, curr_z + lean_z * 2),
            (curr_x + perp_x, fork_y + 1, curr_z + perp_z),
            (curr_x, fork_y + 1, curr_z),
        ];
        
        for (cx, cy, cz) in canopy_centers {
            // Flat canopy - only 1-2 blocks tall but wide
            for ly in 0..2 {
                let wy = cy + ly;
                let radius = if ly == 0 { 3 } else { 2 };
                
                for lx in -radius..=radius {
                    for lz in -radius..=radius {
                        let dist_sq = lx * lx + lz * lz;
                        // Slightly irregular edge
                        let max_dist = radius * radius + ((cx + lx + cz + lz) % 2);
                        if dist_sq <= max_dist {
                            Self::safe_set_block(chunk, chunk_coord, cx + lx, wy, cz + lz, Block::AcaciaLeaves);
                        }
                    }
                }
            }
        }
    }

    /// Plant a Dark Oak tree: Massive tree with thick trunk and dense canopy
    fn plant_darkoak(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        let trunk_h = tree.trunk_height;
        let chunk_world_x = chunk_coord.0 as i32 * CHUNK_SIZE as i32;
        let chunk_world_z = chunk_coord.2 as i32 * CHUNK_SIZE as i32;
        let wx = chunk_world_x + x;
        let wz = chunk_world_z + z;
        
        // Thick 2x2 trunk
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            for tx in 0..2 {
                for tz in 0..2 {
                    Self::safe_set_block(chunk, chunk_coord, wx + tx, wy, wz + tz, Block::DarkOakWood);
                }
            }
        }
        
        // Large branches extending outward at various heights
        let branch_heights = [trunk_h * 2 / 3, trunk_h - 2, trunk_h - 4];
        for (i, &bh) in branch_heights.iter().enumerate() {
            let branch_y = world_y + bh;
            let dir = i as i32;
            let (bx, bz) = match dir % 4 {
                0 => (2, 1),
                1 => (-1, 2),
                2 => (1, -1),
                _ => (-1, -1),
            };
            
            // Thick branch
            Self::safe_set_block(chunk, chunk_coord, wx + bx, branch_y, wz + bz, Block::DarkOakWood);
            Self::safe_set_block(chunk, chunk_coord, wx + bx * 2, branch_y + 1, wz + bz * 2, Block::DarkOakWood);
        }
        
        // Massive, dense canopy
        let canopy_base = world_y + trunk_h - 4;
        
        // Multiple layers with large radius
        for ly in 0..6 {
            let wy = canopy_base + ly;
            // Radius varies: widest in middle
            let radius = match ly {
                0 => 3,
                1 | 2 => 4,
                3 => 4,
                4 => 3,
                _ => 2,
            };
            
            for lx in -radius..=radius {
                for lz in -radius..=radius {
                    let dist_sq = lx * lx + lz * lz;
                    if dist_sq <= radius * radius + 2 {
                        // Dense canopy with few gaps
                        let is_edge = dist_sq > (radius - 1) * (radius - 1);
                        let skip = is_edge && ((wx + lx + wz + lz + wy) % 7) == 0;
                        
                        if !skip {
                            // Offset to center on 2x2 trunk
                            Self::safe_set_block(chunk, chunk_coord, wx + lx + 1, wy, wz + lz + 1, Block::DarkOakLeaves);
                        }
                    }
                }
            }
        }
        
        // Roots at base
        for rx in -1..3 {
            for rz in -1..3 {
                if (rx == -1 || rx == 2 || rz == -1 || rz == 2) && ((rx + rz) % 2) == 0 {
                    Self::safe_set_block(chunk, chunk_coord, wx + rx, world_y - 1, wz + rz, Block::DarkOakWood);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_density_gradient() {
        let generator = VoxelDensityGenerator::new();
        
        // At sea level with neutral conditions, density should be positive (solid)
        let density_at_0 = generator.calculate_density(100.0, 0.0, 100.0);
        assert!(density_at_0 > -0.5, "Should be mostly solid near y=0");

        // High up in sky, density should be negative (air)
        let density_at_150 = generator.calculate_density(100.0, 150.0, 100.0);
        assert!(density_at_150 < 0.0, "Should be mostly air at y=150");
    }

    #[test]
    fn test_biome_detection() {
        let generator = VoxelDensityGenerator::new();
        
        // Various biome checks - just ensure they don't panic
        let _ = generator.get_biome_type(0.0, 0.0, 0.0);
        let _ = generator.get_biome_type(1000.0, 1000.0, 100.0);
        let _ = generator.get_biome_type(-1000.0, -1000.0, 50.0);
    }
}
