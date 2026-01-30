use super::terrain::VoxelDensityGenerator;
use crate::utils::{ChunkCoord, BlockCoord, Mesh};
use super::block::Block;
use super::chunk_mesher::{ChunkBorders, compute_mesh, BORDER_SIZE};

pub const CHUNK_SIZE: isize = 32;
const N_BLOCKS_PER_CHUNK: usize = CHUNK_SIZE.pow(3) as usize;
const LOD_LEVELS: usize = CHUNK_SIZE.ilog2() as usize + 1; // e.g., 16 -> 5 levels (0-4)

#[derive(Clone)]
pub struct Chunk {
    blocks: [Block; N_BLOCKS_PER_CHUNK],
    /// Count of non-empty blocks for O(1) is_empty() check
    non_empty_count: u16,
}

impl Chunk {

    /// creates a new empty chunk
    pub fn new_empty() -> Self {
        Self {
            blocks: [Block::Empty; N_BLOCKS_PER_CHUNK],
            non_empty_count: 0,
        }
    }

    pub fn new_flat(coord: &ChunkCoord, block_type: Block) -> Self {
        let mut chunk = Self::new_empty();
        for x in 0..CHUNK_SIZE as usize {
            for y in 0..CHUNK_SIZE as usize {
                for z in 0..CHUNK_SIZE as usize {
                    if coord.1 == 0 {
                        chunk.set_block(&BlockCoord(x, y, z), block_type, true);
                    }
                }
            }
        }
        chunk
    }

    pub fn new_polulated(density_generator: &VoxelDensityGenerator, chunk_coord: &ChunkCoord) -> Self {

        let mut chunk = Self::new_empty();
        density_generator.populate_chunk_simple(&mut chunk, chunk_coord);
        chunk
    }

    pub fn with_blocks(blocks: [Block; N_BLOCKS_PER_CHUNK]) -> Self {
        let non_empty_count = blocks.iter().filter(|b| !b.is_empty()).count() as u16;
        Self {
            blocks,
            non_empty_count,
        }
    }


    /// Create a chunk that displays its chunk coordinates using blocks (for debugging)
    pub fn new_coord(coord: ChunkCoord) -> Self {
        let mut chunk = Self::new_empty();

        if coord.1 != 0 {
            return chunk; // only create number in chunk y=0
        }

        // Create stone outline at y=0 on the perimeter
        for x in 0..16 {
            for z in 0..16 {
                if x == 0 || x == 15 || z == 0 || z == 15 {
                    chunk.set_block(&BlockCoord(x, 0, z), Block::Stone, true);
                }
            }
        }

        // Digit patterns (5x7 grid, true = block)
        let patterns: [[bool; 35]; 10] = [
            // 0
            [true,true,true,true,true, true,false,false,false,true, true,false,false,false,true, true,false,false,false,true, true,false,false,false,true, true,true,true,true,true, false,false,false,false,false],
            // 1
            [false,false,true,false,false, false,true,true,false,false, false,false,true,false,false, false,false,true,false,false, false,false,true,false,false, false,true,true,true,false, false,false,false,false,false],
            // 2
            [true,true,true,true,false, false,false,false,true,false, false,true,true,true,false, true,false,false,false,false, true,false,false,false,false, true,true,true,true,true, false,false,false,false,false],
            // 3
            [true,true,true,true,false, false,false,false,true,false, false,false,true,true,false, false,false,false,true,false, false,false,false,true,false, true,true,true,true,false, false,false,false,false,false],
            // 4
            [true,false,false,true,false, true,false,false,true,false, true,true,true,true,true, false,false,false,true,false, false,false,false,true,false, false,false,false,true,false, false,false,false,false,false],
            // 5
            [true,true,true,true,true, true,false,false,false,false, true,true,true,true,false, false,false,false,true,false, false,false,false,true,false, true,true,true,true,false, false,false,false,false,false],
            // 6
            [true,true,true,true,false, true,false,false,false,false, true,true,true,true,false, true,false,false,false,true, true,false,false,false,true, true,true,true,true,false, false,false,false,false,false],
            // 7
            [true,true,true,true,true, false,false,false,true,false, false,false,true,false,false, false,true,false,false,false, true,false,false,false,false, true,false,false,false,false, false,false,false,false,false],
            // 8
            [true,true,true,true,false, true,false,false,false,true, true,true,true,true,false, true,false,false,false,true, true,false,false,false,true, true,true,true,true,false, false,false,false,false,false],
            // 9
            [true,true,true,true,false, true,false,false,false,true, true,false,false,false,true, true,true,true,true,true, false,false,false,true,false, true,true,true,true,false, false,false,false,false,false],
        ];

        // Extract coordinates and convert to digit arrays (up to 3 digits, right-aligned)
        let coord_x = coord.0.abs() as u32;
        let coord_z = coord.2.abs() as u32;

        // Convert to base-10 digits
        let x_digits = [
            (coord_x / 100) % 10,
            (coord_x / 10) % 10,
            coord_x % 10,
        ];

        let z_digits = [
            (coord_z / 100) % 10,
            (coord_z / 10) % 10,
            coord_z % 10,
        ];

        println!("Creating number chunk for X: {:?}, Z: {:?}", x_digits, z_digits);

        // Row 1: Display coord.0 (X coordinate) starting at z=2
        let row1_z = 2;
        for (digit_idx, &digit) in x_digits.iter().enumerate() {
            let pattern = &patterns[digit as usize];
            let x_offset = 2 + (digit_idx * 4); // Start at x=2, space digits by 4

            for (idx, &is_set) in pattern.iter().enumerate() {
                if is_set {
                    let px = (idx % 5) + x_offset;
                    let pattern_height = idx / 5; // 0-6
                    
                    if px < 16 && row1_z < 16 && pattern_height < 6 {
                        chunk.set_block(&BlockCoord(px, row1_z, pattern_height), Block::Sand, true);
                    }
                }
            }
        }

        // Row 2: Display coord.2 (Z coordinate) starting at z=9
        let row2_z = 9;
        for (digit_idx, &digit) in z_digits.iter().enumerate() {
            let pattern = &patterns[digit as usize];
            let x_offset = 2 + (digit_idx * 4); // Start at x=2, space digits by 4

            for (idx, &is_set) in pattern.iter().enumerate() {
                if is_set {
                    let px = (idx % 5) + x_offset;
                    let pattern_height = idx / 5; // 0-6
                    
                    if px < 16 && row2_z < 16 && pattern_height < 6 {
                        chunk.set_block(&BlockCoord(px, row2_z, pattern_height), Block::Sand, true);
                    }
                }
            }
        }

        chunk
    }


    #[inline]
    pub fn is_empty(&self) -> bool {
        self.non_empty_count == 0
    }

    /// Get read-only access to the blocks array for meshing.
    pub fn get_blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn get_block(&self, coord: &BlockCoord) -> Block {
        self.blocks[coord.get_block_idx()]
    }
    
    #[inline]
    pub fn set_block(&mut self, coord: &BlockCoord, new: Block, overwrite: bool) -> bool {
        let idx = coord.get_block_idx();
        let old = self.blocks[idx];
        
        if old.is_empty() || overwrite {
            // Update counter: track transitions between empty and non-empty
            let old_empty = old.is_empty();
            let new_empty = new.is_empty();
            
            if old_empty && !new_empty {
                self.non_empty_count += 1; // Adding a block
            } else if !old_empty && new_empty {
                self.non_empty_count -= 1; // Removing a block
            }
            
            self.blocks[idx] = new;
            true
        } else { 
            false 
        }
    }

    /// Extract a border slice for a given face direction.
    /// Returns the blocks at the boundary that would touch a neighboring chunk.
    /// - face 0 (+X): blocks at x=CHUNK_SIZE-1 (needed by +X neighbor's neg_x)
    /// - face 1 (-X): blocks at x=0 (needed by -X neighbor's pos_x)
    /// - face 2 (+Y): blocks at y=CHUNK_SIZE-1
    /// - face 3 (-Y): blocks at y=0
    /// - face 4 (+Z): blocks at z=CHUNK_SIZE-1
    /// - face 5 (-Z): blocks at z=0
    pub fn get_border_slice(&self, face: usize) -> [Block; BORDER_SIZE] {
        let mut slice = [Block::Empty; BORDER_SIZE];
        let s = CHUNK_SIZE as usize;
        
        match face {
            0 => { // +X face: x = CHUNK_SIZE-1, iterate y,z
                for y in 0..s {
                    for z in 0..s {
                        slice[y + z * s] = self.blocks[BlockCoord(s - 1, y, z).get_block_idx()];
                    }
                }
            }
            1 => { // -X face: x = 0, iterate y,z
                for y in 0..s {
                    for z in 0..s {
                        slice[y + z * s] = self.blocks[BlockCoord(0, y, z).get_block_idx()];
                    }
                }
            }
            2 => { // +Y face: y = CHUNK_SIZE-1, iterate x,z
                for x in 0..s {
                    for z in 0..s {
                        slice[x + z * s] = self.blocks[BlockCoord(x, s - 1, z).get_block_idx()];
                    }
                }
            }
            3 => { // -Y face: y = 0, iterate x,z
                for x in 0..s {
                    for z in 0..s {
                        slice[x + z * s] = self.blocks[BlockCoord(x, 0, z).get_block_idx()];
                    }
                }
            }
            4 => { // +Z face: z = CHUNK_SIZE-1, iterate x,y
                for x in 0..s {
                    for y in 0..s {
                        slice[x + y * s] = self.blocks[BlockCoord(x, y, s - 1).get_block_idx()];
                    }
                }
            }
            5 => { // -Z face: z = 0, iterate x,y
                for x in 0..s {
                    for y in 0..s {
                        slice[x + y * s] = self.blocks[BlockCoord(x, y, 0).get_block_idx()];
                    }
                }
            }
            _ => panic!("Invalid face direction"),
        }
        
        slice
    }



    // /// Compute a subsampled version of this chunk for the given LOD level
    // /// Strategy: for each window_size^3 cell, pick the modal block (ignoring air so surface wins),
    // /// then fill ALL blocks in that cell with the chosen block type.
    // /// This allows greedy meshing to recognize merged surfaces across the downsampled region.
    // pub fn compute_downsampled(&self, lod: u8) -> Chunk {
        
    //     if lod == 0 {
    //         return self.clone(); // LOD 0 is original chunk
    //     }

    //     let mut downsampled_chunk = Chunk::new_empty();

    //     let window_size = 1 << lod; // 2^lod

    //     // Downsampled chunk size
    //     let lod_size = CHUNK_SIZE / window_size;
            
    //     for z in 0..lod_size {
    //         for y in 0..lod_size {
    //             for x in 0..lod_size {
    //                 // Pick the modal block inside this window_size^3 cell (ignore air so surface wins over empty)
    //                 let mut block_counts = [0u32; 40]; // Updated for 40 block types (0-39)
    //                 let mut any = false;

    //                 for oz in 0..window_size {
    //                     for oy in 0..window_size {
    //                         for ox in 0..window_size {
    //                             let bx = x * window_size + ox;
    //                             let by = y * window_size + oy;
    //                             let bz = z * window_size + oz;
    //                             let b = self.get_block(&BlockCoord(bx as usize, by as usize, bz as usize));
    //                             if b != Block::Empty {
    //                                 block_counts[b as usize] += 1;
    //                                 any = true;
    //                             }
    //                         }
    //                     }
    //                 }
    //                 let chosen = if any {
    //                     let mut best = Block::Empty;
    //                     let mut highest_count = 0u32;
    //                     for (b_idx, &c) in block_counts.iter().enumerate() {
    //                         if c > highest_count {
    //                             highest_count = c;
    //                             best = Block::from_u8(b_idx as u8);
    //                         }
    //                     }
    //                     best
    //                 } else {
    //                     Block::Empty
    //                 };
                    
    //                 // Skip filling if chosen block is empty (chunk already initialized empty)
    //                 if chosen == Block::Empty {
    //                     continue;
    //                 }

    //                 // Fill all blocks in this window with the chosen type
    //                 for oz in 0..window_size {
    //                     for oy in 0..window_size {
    //                         for ox in 0..window_size {
    //                             let bx = x * window_size + ox;
    //                             let by = y * window_size + oy;
    //                             let bz = z * window_size + oz;
    //                             downsampled_chunk.set_block(&BlockCoord(bx as usize, by as usize, bz as usize), chosen, false);
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //     downsampled_chunk
    // }
    
}