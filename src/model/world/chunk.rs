use super::terrain::VoxelDensityGenerator;
use crate::utils::{ChunkCoord, BlockCoord, Mesh, Vertex};
use super::block::{Block, face_dir_to_normal};


pub const CHUNK_SIZE: isize = 16;
const N_BLOCKS_PER_CHUNK: usize = CHUNK_SIZE.pow(3) as usize;
const LOD_LEVELS: usize = CHUNK_SIZE.ilog2() as usize + 1; // e.g., 16 -> 5 levels (0-4)
#[derive(Clone)]
pub struct Chunk {
    blocks: [Block; N_BLOCKS_PER_CHUNK],
    
    /// stores precomputed meshes for different LOD levels
    meshes: [Option<Mesh>; LOD_LEVELS],

    // tracks number of blocks that are Block::Empty (optimization for skipping empty chunks)
    n_empty_blocks: usize,
}

impl Chunk {

    /// creates a new empty chunk
    pub fn new_empty() -> Self {
        Self {
            blocks: [Block::Empty; N_BLOCKS_PER_CHUNK],
            meshes: Default::default(),
            n_empty_blocks: N_BLOCKS_PER_CHUNK,
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
        Self {
            blocks,
            meshes: Default::default(),
            n_empty_blocks: blocks.iter().filter(|b| b.is_empty()).count(),
        }
    }


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


    pub fn is_empty(&self) -> bool {
        self.n_empty_blocks == N_BLOCKS_PER_CHUNK
    }


    pub fn get_block(&self, coord: &BlockCoord) -> Block {
        self.blocks[coord.get_block_idx()]
    }
    

    pub fn set_block(&mut self, coord: &BlockCoord, new: Block, overwrite: bool) -> bool {
        
        let target = &mut self.blocks[coord.get_block_idx()];
        
        if target.is_empty() || overwrite {

            // keep track of empty blocks count
            if target.is_empty() && !new.is_empty() {
                self.n_empty_blocks -= 1;
            } else if !target.is_empty() && new.is_empty() {
                self.n_empty_blocks += 1;
            }

            *target = new;

            // invalidate meshes
            self.meshes = Default::default();

            true
        } else { false }
    }

    pub fn get_mesh(&mut self, lod: u8) -> Mesh {

        if self.meshes[lod as usize].is_none() {

            self.meshes[lod as usize] = if lod == 0 {
                // if lod 0, use original blocks
                Some(compute_mesh(&self.blocks))
            } else {
                let downsampled = self.compute_downsampled(lod);
                Some(compute_mesh(&downsampled.blocks))
            }
        };


        self.meshes[lod as usize].as_ref().unwrap().clone()
    }


    /// Compute a subsampled version of this chunk for the given LOD level
    /// Strategy: for each window_size^3 cell, pick the modal block (ignoring air so surface wins),
    /// then fill ALL blocks in that cell with the chosen block type.
    /// This allows greedy meshing to recognize merged surfaces across the downsampled region.
    pub fn compute_downsampled(&self, lod: u8) -> Chunk {
        
        assert_ne!(lod, 0, "LOD 0 is the original chunk");

        let mut downsampled_chunk = Chunk::new_empty();

        // return empty chunk if the original is empty
        if self.is_empty() {
            return downsampled_chunk;
        }

        let window_size = 1 << lod; // 2^lod

        // Downsampled chunk size
        let lod_size = CHUNK_SIZE / window_size;
            
        for z in 0..lod_size {
            for y in 0..lod_size {
                for x in 0..lod_size {
                    // Pick the modal block inside this window_size^3 cell (ignore air so surface wins over empty)
                    let mut block_counts = [0u32; 30]; // Updated for 30 block types
                    let mut any = false;

                    for oz in 0..window_size {
                        for oy in 0..window_size {
                            for ox in 0..window_size {
                                let bx = x * window_size + ox;
                                let by = y * window_size + oy;
                                let bz = z * window_size + oz;
                                let b = self.get_block(&BlockCoord(bx as usize, by as usize, bz as usize));
                                if b != Block::Empty {
                                    block_counts[b as usize] += 1;
                                    any = true;
                                }
                            }
                        }
                    }
                    let chosen = if any {
                        let mut best = Block::Empty;
                        let mut highest_count = 0u32;
                        for (b_idx, &c) in block_counts.iter().enumerate() {
                            if c > highest_count {
                                highest_count = c;
                                best = Block::from_u8(b_idx as u8);
                            }
                        }
                        best
                    } else {
                        Block::Empty
                    };
                    
                    // Fill all blocks in this window with the chosen type
                    for oz in 0..window_size {
                        for oy in 0..window_size {
                            for ox in 0..window_size {
                                let bx = x * window_size + ox;
                                let by = y * window_size + oy;
                                let bz = z * window_size + oz;
                                downsampled_chunk.set_block(&BlockCoord(bx as usize, by as usize, bz as usize), chosen, false);
                            }
                        }
                    }
                }
            }
        }
        downsampled_chunk
    }
    
}





// Greedy meshing with face culling - merges adjacent faces of same block type
pub fn compute_mesh(blocks: &[Block; N_BLOCKS_PER_CHUNK]) -> Mesh {

    let mut verts = Vec::new();
    let mut idxs = Vec::new();
    let mut index: u32 = 0;

    // Process each of the 6 face directions
    for dir in 0..6 {
        // Determine axis and direction for this sweep
        let (axis, back_face) = match dir {
            0 => (0, false), // +X
            1 => (0, true),  // -X
            2 => (1, false), // +Y
            3 => (1, true),  // -Y
            4 => (2, false), // +Z
            5 => (2, true),  // -Z
            _ => unreachable!(),
        };

        // Dimensions for the 2D sweep plane (cubic, so all equal to s)
        let (u_dim, v_dim, w_dim) = (CHUNK_SIZE as usize, CHUNK_SIZE as usize, CHUNK_SIZE as usize);

        // Sweep through each slice along the axis
        for w in 0..w_dim {
            // Create a mask for this slice (stores block or air for culled)
            let mut mask = vec![Block::Empty; (u_dim * v_dim) as usize];

            // Fill mask with visible faces
            for v in 0..v_dim {
                for u in 0..u_dim {
                    // Convert u,v,w back to x,y,z based on axis
                    let (x, y, z) = match axis {
                        0 => (w, u, v),
                        1 => (u, w, v),
                        2 => (u, v, w),
                        _ => unreachable!(),
                    };

                    let block = blocks[BlockCoord(x as usize, y as usize, z as usize).get_block_idx()];

                    // Render water and solid blocks, skip air
                    if block.is_empty() { continue; }

                    // Check if face should be visible (face culling)
                    let neighbor = if back_face {
                        // Looking backward along axis
                        if match axis {
                            0 => x == 0,
                            1 => y == 0,
                            2 => z == 0,
                            _ => unreachable!(),
                        } {
                            Block::Empty // Out of bounds = air
                        } else {
                            match axis {
                                0 => blocks[BlockCoord(x - 1, y, z).get_block_idx()],
                                1 => blocks[BlockCoord(x, y - 1, z).get_block_idx()],
                                2 => blocks[BlockCoord(x, y, z - 1).get_block_idx()],
                                _ => unreachable!(),
                            }
                        }
                    } else {
                        // Looking forward along axis
                        if match axis {
                            0 => x + 1 >= CHUNK_SIZE as usize,
                            1 => y + 1 >= CHUNK_SIZE as usize,
                            2 => z + 1 >= CHUNK_SIZE as usize,
                            _ => unreachable!(),
                        } {
                            Block::Empty // Out of bounds = air
                        } else {
                            match axis {
                                0 => blocks[BlockCoord(x + 1, y, z).get_block_idx()],
                                1 => blocks[BlockCoord(x, y + 1, z).get_block_idx()],
                                2 => blocks[BlockCoord(x, y, z + 1).get_block_idx()],
                                _ => unreachable!(),
                            }
                        }
                    };

                    // Face is visible if neighbor is air or different material (e.g., water next to land)
                    let should_render = neighbor == Block::Empty || 
                                        (block == Block::Water && neighbor != Block::Water) ||
                                        (block != Block::Water && neighbor == Block::Water);
                    if should_render {
                        mask[(u + v * u_dim) as usize] = block;
                    }
                }
            }

            // Greedy meshing: merge adjacent faces into rectangles
            for v in 0..v_dim {
                for u in 0..u_dim {
                    let mask_idx = (u + v * u_dim) as usize;
                    let block = mask[mask_idx];
                    if block == Block::Empty { continue; }

                    // Find width (u direction)
                    let mut width = 1;
                    while u + width < u_dim {
                        let check_idx = (u + width + v * u_dim) as usize;
                        if mask[check_idx] != block { break; }
                        width += 1;
                    }

                    // Find height (v direction)
                    let mut height = 1;
                    'height_loop: while v + height < v_dim {
                        for du in 0..width {
                            let check_idx = (u + du + (v + height) * u_dim) as usize;
                            if mask[check_idx] != block {
                                break 'height_loop;
                            }
                        }
                        height += 1;
                    }

                    // Clear merged area from mask
                    for dv in 0..height {
                        for du in 0..width {
                            let clear_idx = (u + du + (v + dv) * u_dim) as usize;
                            mask[clear_idx] = Block::Empty;
                        }
                    }

                    // Generate quad for this merged rectangle
                    let face_dir = dir as u8;
                    let color = block.color(face_dir);
                    let normal = face_dir_to_normal(face_dir);

                    // Generate quad vertices based on axis and dimensions
                    // For each axis, we need to map (u,v,w) and (width,height) correctly
                    let (p0, p1, p2, p3) = match axis {
                        0 => { // X-axis: u=Y, v=Z, w=X
                            let xf = if back_face { w as f32 } else { (w + 1) as f32 };
                            if back_face {
                                (
                                    [xf, u as f32, v as f32],
                                    [xf, (u + width) as f32, v as f32],
                                    [xf, (u + width) as f32, (v + height) as f32],
                                    [xf, u as f32, (v + height) as f32],
                                )
                            } else {
                                (
                                    [xf, u as f32, (v + height) as f32],
                                    [xf, (u + width) as f32, (v + height) as f32],
                                    [xf, (u + width) as f32, v as f32],
                                    [xf, u as f32, v as f32],
                                )
                            }
                        },
                        1 => { // Y-axis: u=X, v=Z, w=Y
                            let yf = if back_face { w as f32 } else { (w + 1) as f32 };
                            if back_face {
                                (
                                    [u as f32, yf, v as f32],
                                    [u as f32, yf, (v + height) as f32],
                                    [(u + width) as f32, yf, (v + height) as f32],
                                    [(u + width) as f32, yf, v as f32],
                                )
                            } else {
                                (
                                    [(u + width) as f32, yf, v as f32],
                                    [(u + width) as f32, yf, (v + height) as f32],
                                    [u as f32, yf, (v + height) as f32],
                                    [u as f32, yf, v as f32],
                                )
                            }
                        },
                        2 => { // Z-axis: u=X, v=Y, w=Z
                            let zf = if back_face { w as f32 } else { (w + 1) as f32 };
                            if back_face {
                                (
                                    [u as f32, v as f32, zf],
                                    [(u + width) as f32, v as f32, zf],
                                    [(u + width) as f32, (v + height) as f32, zf],
                                    [u as f32, (v + height) as f32, zf],
                                )
                            } else {
                                (
                                    [(u + width) as f32, v as f32, zf],
                                    [u as f32, v as f32, zf],
                                    [u as f32, (v + height) as f32, zf],
                                    [(u + width) as f32, (v + height) as f32, zf],
                                )
                            }
                        },
                        _ => unreachable!(),
                    };

                    // UV coordinates scaled by quad size
                    let uv_scale_u = width as f32;
                    let uv_scale_v = height as f32;

                    verts.push(Vertex { pos: p0, normal, color, uv: [0.0, 0.0] });
                    verts.push(Vertex { pos: p1, normal, color, uv: [0.0, uv_scale_v] });
                    verts.push(Vertex { pos: p2, normal, color, uv: [uv_scale_u, uv_scale_v] });
                    verts.push(Vertex { pos: p3, normal, color, uv: [uv_scale_u, 0.0] });

                    // Reverse winding order to match CCW front face
                    idxs.extend_from_slice(&[index, index + 2, index + 1, index, index + 3, index + 2]);
                    index += 4;
                }
            }
        }
    }

    Mesh { vertices: verts, indices: idxs }
}

