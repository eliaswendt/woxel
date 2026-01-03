// terrain.rs - Complete terrain generation system
// Combines noise functions, density-based generation, biome detection, and surface generation
//
// ============================================================================
// COMPLETE TERRAIN GENERATION PIPELINE
// ============================================================================
// 
// The terrain is generated chunk-by-chunk using a 6-step pipeline:
//
// STEP 1: Biome Determination (2D Noise)
//   → Uses 2D perlin noise to determine biome type from temperature & humidity
//   → Biomes: Tundra, Mountain, Forest, Desert, Beach, Plain, Ocean
//   → Called by: get_biome_type()
//
// STEP 2: Density Calculation (3D Noise with Gravity)
//   → Uses 3D FBM noise to calculate terrain density at each (x,y,z) position
//   → Y-GRADIENT creates natural gravity: no floating terrain!
//   → Higher = more density, Lower = less density (air)
//   → Called by: calculate_density()
//
// STEP 3: Cave Carving (3D Noise Ranges)
//   → During density calculation, specific noise ranges force air (caves)
//   → Creates natural cave systems integrated with terrain
//   → Included in: calculate_density()
//
// STEP 4: Water/Terrain Filling (Y-Level Checks)
//   → If y <= 0 (below sea level): place water
//   → If y > 0 but no solid density: place air
//   → Creates natural water bodies at sea level
//   → Implemented in: populate_chunk()
//
// STEP 5: Tree Placement (2D Noise + Surface Detection)
//   → Use 2D noise to determine tree centers
//   → Only place trees on Grass/Moss surface blocks
//   → Tree type determined by biome
//   → Tree height randomized per position
//   → Implemented in: populate_chunk() and plant_tree()
//
// STEP 6: Clouds (Y == 255)
//   → 2D noise determines cloud coverage at height 255
//   → Only top of world
//   → Implemented in: populate_chunk()
//
// Result: Coherent, natural terrain with forests, mountains, caves, and water!
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
            continentalness_freq: 0.008,
            erosion_freq: 0.012,
            temperature_freq: 0.005,
            humidity_freq: 0.005,
            base_3d_freq: 0.028,
            cave_freq: 0.04,
            
            // Height parameters
            base_height: 45.0,
            continental_height_amplitude: 80.0,
            erosion_height_amplitude: 40.0,
            y_gradient_scale: 80.0,
            base_3d_noise_strength: 0.40,
            
            // Cave parameters
            cave_noise_min: -0.15,
            cave_noise_max: 0.2,
            
            // Tree parameters
            tree_noise_frequency: 0.4,
            tree_spawn_threshold: -0.02,
            tree_height_variation: 3,
            
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

        // 3. Temperature & Humidity for biome (used later in GetBiomeType)
        let temperature = fbm(x, z, self.config.temperature_freq, 0.55, 3);
        let humidity = fbm(x + 5000.0, z - 5000.0, self.config.humidity_freq, 0.55, 3);

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
                                    if plant_noise > 0.8 { Block::Grass_Tall } else { Block::Grass_Short }
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

    /// Populate a chunk with simple 2D terrain (sea level at y=0)
    /// 
    /// Simplified terrain generation using only 2D noise:
    /// - 2D noise for biome determination
    /// - 2D noise for terrain height (average 0, maximum 255)
    /// - Height-based block selection:
    ///   * y < 0: Water
    ///   * y >= 200: Snow (no grass)
    ///   * y >= 100: Stone (no grass)
    ///   * y < 100: Grass/biome-specific blocks
    /// - Trees placed only below y=150
    pub fn populate_chunk_simple(&self, chunk: &mut super::chunk::Chunk, chunk_coord: &crate::utils::ChunkCoord) {
        use crate::utils::BlockCoord;
        
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let world_coord = chunk_coord.to_world_coord();
                let wx = world_coord.0 as f32 + x as f32;
                let wz = world_coord.2 as f32 + z as f32;

                // STEP 1: Determine biome using 2D noise
                let biome = self.get_biome_type(wx, wz, 30.0);
                
                // STEP 2: Calculate terrain height using 2D noise
                // Use higher frequency (0.08) for more terrain variation and detail
                // More octaves (6) for realistic mountain/valley transitions
                let height_noise = fbm(wx * 0.08, wz * 0.08, 0.08, 0.55, 6);
                let terrain_height = ((height_noise + 1.0) * 0.5 * 255.0) as isize;

                // Calculate tree data once per column
                let tree_data = self.calculate_tree_data(wx, wz);

                // Fill entire column based on terrain height
                for y in 0..CHUNK_SIZE {
                    let world_y = chunk_coord.1 as isize * CHUNK_SIZE as isize + y as isize;
                    
                    let block = if world_y >= terrain_height {
                        // STEP 3: Above terrain = air
                        Block::Empty
                    } else if world_y < 0 {
                        // Below sea level = water
                        Block::Water
                    } else if world_y == terrain_height - 1 {
                        // Surface layer - height-based determination
                        if world_y >= 200 {
                            // Above y=200: Snow
                            Block::Snow
                        } else if world_y >= 100 {
                            // Above y=100: Bare stone (no grass)
                            Block::Stone
                        } else {
                            // Below y=100: Grass and biome-specific blocks
                            self.get_surface_block_for_biome(wx, wz, world_y as f32, biome)
                        }
                    } else {
                        // Subsurface blocks
                        if world_y >= 200 {
                            Block::Snow
                        } else if world_y >= 100 {
                            Block::Stone
                        } else {
                            self.get_subsurface_block(wx, wz, world_y as f32, biome)
                        }
                    };

                    chunk.set_block(&BlockCoord(x as usize, y as usize, z as usize), block, false);

                    // Place trees only below y=150 on grass/moss surface
                    if world_y < 150 && world_y == terrain_height - 1 && tree_data.should_spawn && 
                       matches!(block, Block::Grass | Block::Moss) {
                        let tree = Tree {
                            pos: (x as i32, z as i32),
                            tree_type: tree_data.tree_type,
                            trunk_height: tree_data.tree_height,
                        };
                        Self::plant_tree(&tree, chunk_coord, world_y as i32 + 1, chunk);
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

    /// Plant an Oak tree: compact tree with 1-block trunk and 2-layer foliage
    fn plant_oak(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        use crate::utils::BlockCoord;
        const CHUNK_SIZE: i32 = 16;
        const OAK_LEAF_RADIUS: i32 = 1;
        
        let trunk_h = tree.trunk_height;
        
        // Place trunk vertically
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local >= 0 && cy_local < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(x as usize, cy_local as usize, z as usize), Block::Wood, false);
            }
        }
        
        // Place foliage: 2 layers with compact 3x3 shape
        let leaves_base = world_y + trunk_h - 2;
        for ly in 0..2 {
            let wy = leaves_base + ly;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local < 0 || cy_local >= CHUNK_SIZE as i32 { continue; }
            
            for lx in -OAK_LEAF_RADIUS..=OAK_LEAF_RADIUS {
                for lz in -OAK_LEAF_RADIUS..=OAK_LEAF_RADIUS {
                    let nx = x + lx;
                    let nz = z + lz;
                    if nx < 0 || nz < 0 || nx >= CHUNK_SIZE as i32 || nz >= CHUNK_SIZE as i32 { continue; }
                    
                    // Place leaves in 3x3 area
                    chunk.set_block(&BlockCoord(nx as usize, cy_local as usize, nz as usize), Block::OakLeaves, false);
                }
            }
        }
    }

    /// Plant a Spruce tree: conical tree with 1-block trunk and 3-layer foliage
    fn plant_spruce(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        use crate::utils::BlockCoord;
        const CHUNK_SIZE: i32 = 16;
        
        let trunk_h = tree.trunk_height;
        
        // Place trunk vertically
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local >= 0 && cy_local < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(x as usize, cy_local as usize, z as usize), Block::SpruceWood, false);
            }
        }
        
        // Place foliage: 3 layers in conical shape (2x2, 2x2, 1x1)
        let leaves_base = world_y + trunk_h - 2;
        for ly in 0..3 {
            let wy = leaves_base + ly;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local < 0 || cy_local >= CHUNK_SIZE as i32 { continue; }
            
            // Radius shrinks for upper layers (cone shape)
            let radius = match ly {
                0 => 2,      // Bottom: wide
                1 => 1,      // Middle: medium
                _ => 1,      // Top: narrow
            };
            
            for lx in -radius..=radius {
                for lz in -radius..=radius {
                    let nx = x + lx;
                    let nz = z + lz;
                    if nx < 0 || nz < 0 || nx >= CHUNK_SIZE as i32 || nz >= CHUNK_SIZE as i32 { continue; }
                    
                    let dist_sq = lx * lx + lz * lz;
                    // Create circular foliage (not square)
                    if dist_sq <= (radius * radius + 1) {
                        chunk.set_block(&BlockCoord(nx as usize, cy_local as usize, nz as usize), Block::SpruceLeaves, false);
                    }
                }
            }
        }
    }

    /// Plant a Birch tree: tall thin tree with 1-block trunk and 2-layer foliage
    fn plant_birch(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        use crate::utils::BlockCoord;
        const CHUNK_SIZE: i32 = 16;
        const BIRCH_LEAF_RADIUS: i32 = 1;
        
        let trunk_h = tree.trunk_height;
        
        // Place trunk vertically
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local >= 0 && cy_local < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(x as usize, cy_local as usize, z as usize), Block::BirchWood, false);
            }
        }
        
        // Place foliage: 2 layers with compact spherical shape
        let leaves_base = world_y + trunk_h - 2;
        for ly in 0..2 {
            let wy = leaves_base + ly;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local < 0 || cy_local >= CHUNK_SIZE as i32 { continue; }
            
            for lx in -BIRCH_LEAF_RADIUS..=BIRCH_LEAF_RADIUS {
                for lz in -BIRCH_LEAF_RADIUS..=BIRCH_LEAF_RADIUS {
                    let nx = x + lx;
                    let nz = z + lz;
                    if nx < 0 || nz < 0 || nx >= CHUNK_SIZE as i32 || nz >= CHUNK_SIZE as i32 { continue; }
                    
                    // Place leaves in 3x3 area
                    chunk.set_block(&BlockCoord(nx as usize, cy_local as usize, nz as usize), Block::BirchLeaves, false);
                }
            }
        }
    }

    /// Plant an Acacia tree: dry climate tree with 1-block trunk and wide foliage
    fn plant_acacia(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        use crate::utils::BlockCoord;
        const CHUNK_SIZE: i32 = 16;
        
        let trunk_h = tree.trunk_height;
        
        // Place trunk vertically
        for ty in 0..trunk_h {
            let wy = world_y + ty;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local >= 0 && cy_local < CHUNK_SIZE {
                chunk.set_block(&BlockCoord(x as usize, cy_local as usize, z as usize), Block::AcaciaWood, false);
            }
        }
        
        // Acacia: wide, flat foliage - 2 layers with radius 2
        let leaves_base = world_y + trunk_h - 1;
        for ly in 0..2 {
            let wy = leaves_base + ly;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local < 0 || cy_local >= CHUNK_SIZE as i32 { continue; }
            
            let radius = 2;
            for lx in -radius..=radius {
                for lz in -radius..=radius {
                    let nx = x + lx;
                    let nz = z + lz;
                    if nx < 0 || nz < 0 || nx >= CHUNK_SIZE as i32 || nz >= CHUNK_SIZE as i32 { continue; }
                    
                    // Create circular foliage shape
                    let dist_sq = lx * lx + lz * lz;
                    if dist_sq <= 5 {
                        chunk.set_block(&BlockCoord(nx as usize, cy_local as usize, nz as usize), Block::AcaciaLeaves, false);
                    }
                }
            }
        }
    }

    /// Plant a Dark Oak tree: large tree with 2-block trunk and dense foliage
    fn plant_darkoak(tree: &Tree, chunk_coord: &crate::utils::ChunkCoord, x: i32, z: i32, world_y: i32, chunk: &mut super::chunk::Chunk) {
        use crate::utils::BlockCoord;
        const CHUNK_SIZE: i32 = 16;
        
        let trunk_h = tree.trunk_height;
        
        // Dark Oak: 2x2 trunk base
        for tx in 0..2 {
            for tz in 0..2 {
                for ty in 0..trunk_h {
                    let wy = world_y + ty;
                    let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
                    if cy_local >= 0 && cy_local < CHUNK_SIZE {
                        let nx = x + tx;
                        let nz = z + tz;
                        if nx < 0 || nz < 0 || nx >= CHUNK_SIZE as i32 || nz >= CHUNK_SIZE as i32 { continue; }
                        chunk.set_block(&BlockCoord(nx as usize, cy_local as usize, nz as usize), Block::DarkOakWood, false);
                    }
                }
            }
        }
        
        // Dark Oak: Dense foliage - 3 layers, large radius
        let leaves_base = world_y + trunk_h - 3;
        for ly in 0..3 {
            let wy = leaves_base + ly;
            let cy_local = wy - chunk_coord.1 as i32 * CHUNK_SIZE;
            if cy_local < 0 || cy_local >= CHUNK_SIZE as i32 { continue; }
            
            let radius = match ly {
                0 => 3,     // Bottom: very wide
                1 => 2,     // Middle: medium
                _ => 1,     // Top: narrow
            };
            
            for lx in -radius..=radius {
                for lz in -radius..=radius {
                    let nx = x + lx;
                    let nz = z + lz;
                    if nx < 0 || nz < 0 || nx >= CHUNK_SIZE as i32 || nz >= CHUNK_SIZE as i32 { continue; }
                    
                    chunk.set_block(&BlockCoord(nx as usize, cy_local as usize, nz as usize), Block::DarkOakLeaves, false);
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
