// voxel_density.rs - Professional 3D density-based terrain generation
// Similar to Minecraft's Caves & Cliffs terrain system

use crate::model::world::terrain::fbm;

#[derive(Clone, Copy, Debug)]
pub enum BiomeType {
    Ocean,
    Beach,
    Plain,
    Forest,
    Mountain,
    Tundra,
    Desert,
}

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
            tree_noise_frequency: 0.5,
            tree_spawn_threshold: -0.01,
            tree_height_variation: 3,
        }
    }
}

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

    /// Main function: Calculate 3D density at position (x, y, z)
    /// Returns a density value where:
    ///   > 0 = solid block
    ///   <= 0 = air/empty
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

        // 4. Calculate terrain height baseline
        let continental_height = continentalness * self.config.continental_height_amplitude;
        let erosion_height = erosion * self.config.erosion_height_amplitude;
        let base_height = continental_height + erosion_height + self.config.base_height;

        // 5. Y-gradient: density decreases as you go above base height
        let y_diff = y - base_height;
        let mut density = 0.5 - (y_diff / self.config.y_gradient_scale).clamp(-1.0, 1.0);

        // 6. Base 3D Noise: add surface distortion for overhangs and detail
        let base_3d = crate::terrain::fbm_3d(x, y, z, self.config.base_3d_freq, 0.55, 3);
        density += base_3d * self.config.base_3d_noise_strength;

        // 7. Cave carving: if cave noise is in narrow band, force air
        let cave_noise = crate::terrain::fbm_3d(x, y, z, self.config.cave_freq, 0.55, 3);
        if cave_noise > self.config.cave_noise_min && cave_noise < self.config.cave_noise_max {
            return -1.0; // Force air (caves)
        }

        density
    }

    /// Determine biome type based on temperature, humidity, and height
    pub fn get_biome_type(&self, x: f32, z: f32, y: f32) -> BiomeType {
        let temperature = fbm(x, z, self.config.temperature_freq, 0.55, 3);
        let humidity = fbm(x + 5000.0, z - 5000.0, self.config.humidity_freq, 0.55, 3);
        let continentalness = fbm(x, z, self.config.continentalness_freq, 0.55, 4);

        // High mountains (snow-covered peaks) - lower threshold
        if y > 80.0 && continentalness > 0.3 {
            if temperature < -0.6 {
                return BiomeType::Tundra;
            } else {
                return BiomeType::Mountain;
            }
        }

        // Moderate elevation
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

        // Wet regions - forest (more permissive: humidity > 0.0)
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
    ) -> crate::model::Block {
        use crate::model::Block;

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
        }
    }

    /// Get subsurface block based on depth and biome
    pub fn get_subsurface_block(&self, x: f32, z: f32, y: f32, biome: BiomeType) -> crate::model::Block {
        use crate::model::Block;

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
    pub fn get_ore_block(&self, x: f32, y: f32, z: f32) -> Option<crate::model::Block> {
        use crate::model::Block;

        let ore_check = crate::terrain::noise2d(
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
