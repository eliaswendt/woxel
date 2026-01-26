//! Greedy meshing implementation for voxel chunks.
//! 
//! This module handles the conversion of chunk block data into renderable meshes,
//! using greedy meshing to merge adjacent faces of the same block type.

use crate::utils::{BlockCoord, Mesh, Vertex};
use super::block::{Block, face_dir_to_normal};
use super::chunk::CHUNK_SIZE;

/// Size of a border slice (one face of the chunk)
pub const BORDER_SIZE: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Border slices from neighboring chunks for accurate face culling at chunk boundaries.
/// Each slice contains the blocks at the boundary face of the neighboring chunk.
/// - pos_x: x=0 slice from the +X neighbor (blocks that touch our x=CHUNK_SIZE-1)
/// - neg_x: x=CHUNK_SIZE-1 slice from the -X neighbor (blocks that touch our x=0)
/// - etc.
#[derive(Clone)]
pub struct ChunkBorders {
    pub pos_x: Option<[Block; BORDER_SIZE]>,
    pub neg_x: Option<[Block; BORDER_SIZE]>,
    pub pos_y: Option<[Block; BORDER_SIZE]>,
    pub neg_y: Option<[Block; BORDER_SIZE]>,
    pub pos_z: Option<[Block; BORDER_SIZE]>,
    pub neg_z: Option<[Block; BORDER_SIZE]>,
}

impl Default for ChunkBorders {
    fn default() -> Self {
        Self {
            pos_x: None,
            neg_x: None,
            pos_y: None,
            neg_y: None,
            pos_z: None,
            neg_z: None,
        }
    }
}

/// Get the neighbor block for face culling. Uses border slices for chunk boundaries.
#[inline]
fn get_neighbor(blocks: &[Block], x: isize, y: isize, z: isize, borders: &ChunkBorders) -> Block {
    let s = CHUNK_SIZE;
    
    // Check if outside chunk bounds and use border slices
    if x < 0 {
        return borders.neg_x
            .map(|b| b[(y as usize) + (z as usize) * s as usize])
            .unwrap_or(Block::Empty);
    }
    if x >= s {
        return borders.pos_x
            .map(|b| b[(y as usize) + (z as usize) * s as usize])
            .unwrap_or(Block::Empty);
    }
    if y < 0 {
        return borders.neg_y
            .map(|b| b[(x as usize) + (z as usize) * s as usize])
            .unwrap_or(Block::Empty);
    }
    if y >= s {
        return borders.pos_y
            .map(|b| b[(x as usize) + (z as usize) * s as usize])
            .unwrap_or(Block::Empty);
    }
    if z < 0 {
        return borders.neg_z
            .map(|b| b[(x as usize) + (y as usize) * s as usize])
            .unwrap_or(Block::Empty);
    }
    if z >= s {
        return borders.pos_z
            .map(|b| b[(x as usize) + (y as usize) * s as usize])
            .unwrap_or(Block::Empty);
    }
    
    // Inside chunk
    blocks[BlockCoord(x as usize, y as usize, z as usize).get_block_idx()]
}

/// Determine if a face should be rendered between a block and its neighbor.
#[inline]
fn should_render_face(block: Block, neighbor: Block) -> bool {
    // Always render against air
    if neighbor.is_empty() {
        return true;
    }
    // Render solid faces against transparent neighbors
    if !block.is_transparent() && neighbor.is_transparent() {
        return true;
    }
    // Render transparent faces against solid neighbors
    if block.is_transparent() && !neighbor.is_transparent() {
        return true;
    }
    // Render faces between different transparent types
    if block.is_transparent() && neighbor.is_transparent() && block != neighbor {
        return true;
    }
    false
}

/// Greedy meshing with face culling - merges adjacent faces of same block type.
/// 
/// # Arguments
/// * `blocks` - The block array for this chunk
/// * `lod` - Level of detail (currently only 0 is supported)
/// * `borders` - Border slices from neighboring chunks for accurate edge culling
pub fn compute_mesh(blocks: &[Block], lod: u8, borders: &ChunkBorders) -> Mesh {
    assert!(lod == 0, "Only LOD 0 meshing is currently implemented");

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

                    // Get neighbor coordinate
                    let (nx, ny, nz) = if back_face {
                        match axis {
                            0 => (x as isize - 1, y as isize, z as isize),
                            1 => (x as isize, y as isize - 1, z as isize),
                            2 => (x as isize, y as isize, z as isize - 1),
                            _ => unreachable!(),
                        }
                    } else {
                        match axis {
                            0 => (x as isize + 1, y as isize, z as isize),
                            1 => (x as isize, y as isize + 1, z as isize),
                            2 => (x as isize, y as isize, z as isize + 1),
                            _ => unreachable!(),
                        }
                    };

                    let neighbor = get_neighbor(blocks, nx, ny, nz, borders);

                    if should_render_face(block, neighbor) {
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
                    let roughness = block.roughness();
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

                    // UV coordinates: x = roughness, y = quad scale for potential tiling
                    let uv_scale = (width.max(height)) as f32;

                    verts.push(Vertex { pos: p0, normal, color, uv: [roughness, uv_scale] });
                    verts.push(Vertex { pos: p1, normal, color, uv: [roughness, uv_scale] });
                    verts.push(Vertex { pos: p2, normal, color, uv: [roughness, uv_scale] });
                    verts.push(Vertex { pos: p3, normal, color, uv: [roughness, uv_scale] });

                    // Reverse winding order to match CCW front face
                    idxs.extend_from_slice(&[index, index + 2, index + 1, index, index + 3, index + 2]);
                    index += 4;
                }
            }
        }
    }

    Mesh { vertices: verts, indices: idxs }
}
