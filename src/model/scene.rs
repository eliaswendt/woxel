use crate::{model::{CHUNK_SIZE, world::{Block, Chunk, ChunkBorders}}, utils::{BlockCoord, ChunkCoord, MeshBuffer, WorldCoord}};

use super::world::VoxelDensityGenerator;

fn select_lod(distance_to_player: usize) -> u8 {
    // TODO: make these distances configurable
    return 0;

    if distance_to_player < 20 {
        0  // Full resolution
    } else if distance_to_player < 40 {
        1  // 1/2 resolution
    } else if distance_to_player < 60 {
        2  // 1/4 resolution
    } else if distance_to_player < 80 {
        3  // 1/8 resolution
    } else {
        4  // 1/16 resolution
    }
}


/// pre-compute sphere offsets for chunk loading order
fn generate_qube_offset_in_spherical_order(active_size: [usize; 3]) -> Vec<((isize, isize, isize), usize)> {

    let radius = [
        (active_size[0] / 2) as isize,
        (active_size[1] / 2) as isize,
        (active_size[2] / 2) as isize,
    ];

    let mut offsets = Vec::new();
    for x in -radius[0]..=radius[0] {
        for y in -radius[1]..=radius[1] {
            for z in -radius[2]..=radius[2] {
                let dist = (x.pow(2) + y.pow(2) + z.pow(2)).isqrt() as usize;
                offsets.push(((x, y, z), dist));
            }
        }
    }

    // sort by distance (closest first)
    offsets.sort_unstable_by_key(|(_, dist)| *dist);
    offsets
}

pub enum MeshGenerationState {
    Pending,
    Completed {
        lod: u8,
        buffer: MeshBuffer,
    },
}


pub enum ActiveEntry {

    /// loaded chunk that is non-empty
    Loaded {
        coord: ChunkCoord,
        chunk: Chunk,
        required_lod: u8,
        mesh: MeshGenerationState,
    },

    /// an empty chunk
    Empty {
        coord: ChunkCoord,
    },

    /// chunk not loaded yet
    Pending
}

impl ActiveEntry {
    pub fn is_loaded(&self) -> bool {
        matches!(self, ActiveEntry::Loaded { .. })
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, ActiveEntry::Empty { .. })
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, ActiveEntry::Pending)
    }

    pub fn unset_mesh(&mut self) {
        if let ActiveEntry::Loaded { mesh, .. } = self {
            *mesh = MeshGenerationState::Pending;
        }
    }

    fn needs_remeshing(&self) -> bool {
        if let ActiveEntry::Loaded { required_lod, mesh, .. } = self {
            match mesh {
                MeshGenerationState::Completed { lod, .. } => lod != required_lod,
                MeshGenerationState::Pending => true,
            }
        } else {
            false
        }
    }

    fn generate_and_upload_mesh(&mut self, device: &wgpu::Device, chunk_borders: &ChunkBorders) {
        if let ActiveEntry::Loaded { coord: entry_coord, chunk: active_chunk, required_lod, mesh } = self {
            log::info!("Generating mesh for chunk {:?} at LOD {}", entry_coord, required_lod);
            let mut new_mesh = active_chunk.compute_mesh(*required_lod, chunk_borders);
            new_mesh.offset_vertices_by(entry_coord);
            let new_mesh_buffer = new_mesh.upload(device);

            *mesh = MeshGenerationState::Completed { lod: *required_lod, buffer: new_mesh_buffer };
        }
    }
}




pub struct Scene {
    /// States: 
    /// 
    /// None = chunk not loaded (option exists to allow sparse storage)
    /// 
    /// Some((ChunkCoord, Chunk, None)) = chunk loaded/generated but not meshed (e.g. needs re-meshing)
    /// 
    /// Some((ChunkCoord, Chunk, Some((LOD, MeshBuffer)))) = chunk loaded and meshed
    pub active: Vec<ActiveEntry>,

    /// Number of chunks along each axis in the active chunk grid
    active_size: [usize; 3],
    previous_player_coord: ChunkCoord,
    sphere_offsets: Vec<((isize, isize, isize), usize)>,

    density_generator: VoxelDensityGenerator,
}

impl Scene {
    pub fn new(active_size: [usize; 3]) -> Self {
        // ensure chunk_distance is a power of two for modulo indexing
        // assert!(chunk_distance.is_power_of_two(), "chunk_distance must be a power of two");
        
        let mut active = Vec::new();

        for _ in 0..active_size[0] * active_size[1] * active_size[2] {
            active.push(ActiveEntry::Pending);
        }

        Self {
            active_size: active_size,
            active: active,
            previous_player_coord: ChunkCoord(0, 0, 0),

            sphere_offsets: generate_qube_offset_in_spherical_order(active_size),
            density_generator: VoxelDensityGenerator::new(),
        }
    }


    fn get_active_idx(&self, coord: &ChunkCoord) -> usize {
        coord.0.rem_euclid(self.active_size[0] as isize) as usize + 
        coord.1.rem_euclid(self.active_size[1] as isize) as usize * self.active_size[0] + 
        coord.2.rem_euclid(self.active_size[2] as isize) as usize * self.active_size[0] * self.active_size[1] 
    }

    fn get_active_entry(&self, coord: &ChunkCoord) -> &ActiveEntry {
        &self.active[self.get_active_idx(coord)]
    }

    fn get_active_entry_mut(&mut self, coord: &ChunkCoord) -> &mut ActiveEntry {
        let active_idx = self.get_active_idx(coord);
        &mut self.active[active_idx]
    }


    pub fn remesh(&mut self) {
        for active_entry in &mut self.active {
            if let ActiveEntry::Loaded { mesh, .. } = active_entry {
                *mesh = MeshGenerationState::Pending;
            }
        }
    }


    /// Insert or replace a chunk at the given coordinate
    /// 
    /// If the chunk is empty, it uses the shared empty chunk reference
    /// 
    /// Clears own mesh and marks neighboring chunks for re-meshing
    fn insert_chunk(&mut self, coord: &ChunkCoord, chunk: Chunk, lod: u8) {
                
        *self.get_active_entry_mut(coord) = if chunk.is_empty() {
            // if chunk is empty -> re-use shared empty chunk
            ActiveEntry::Empty { coord: *coord }
        } else {
            
            // remove mesh of all neighboring chunks to force re-meshing
            for neighbor_coord in self.get_neighbor_chunk_coords(coord) {
                self.get_active_entry_mut(&neighbor_coord).unset_mesh();
            }

            // create new active entry
            ActiveEntry::Loaded { coord: *coord, chunk: chunk, required_lod: lod, mesh: MeshGenerationState::Pending }
        };
    }


    /// Collect border slices from neighboring chunks for accurate face culling
    fn get_chunk_borders(&self, coord: &ChunkCoord) -> ChunkBorders {
        let get_border = |entry: &ActiveEntry, face: usize| -> Option<_> {
            if let ActiveEntry::Loaded { chunk, .. } = entry {
                Some(chunk.get_border_slice(face))
            } else {
                None
            }
        };
        ChunkBorders {
            pos_x: get_border(self.get_active_entry(&ChunkCoord(coord.0 + 1, coord.1, coord.2)), 1), // their -X face
            neg_x: get_border(self.get_active_entry(&ChunkCoord(coord.0 - 1, coord.1, coord.2)), 0), // their +X face
            pos_y: get_border(self.get_active_entry(&ChunkCoord(coord.0, coord.1 + 1, coord.2)), 3), // their -Y face
            neg_y: get_border(self.get_active_entry(&ChunkCoord(coord.0, coord.1 - 1, coord.2)), 2), // their +Y face
            pos_z: get_border(self.get_active_entry(&ChunkCoord(coord.0, coord.1, coord.2 + 1)), 5), // their -Z face
            neg_z: get_border(self.get_active_entry(&ChunkCoord(coord.0, coord.1, coord.2 - 1)), 4), // their +Z face
        }
    }
    

    /// Mark all 6 neighbors of a chunk as dirty (needing re-mesh)
    fn get_neighbor_chunk_coords(&mut self, coord: &ChunkCoord) -> Vec<ChunkCoord> {

        vec![
            ChunkCoord(coord.0 + 1, coord.1, coord.2),
            ChunkCoord(coord.0 - 1, coord.1, coord.2),
            ChunkCoord(coord.0, coord.1 + 1, coord.2),
            ChunkCoord(coord.0, coord.1 - 1, coord.2),
            ChunkCoord(coord.0, coord.1, coord.2 + 1),
            ChunkCoord(coord.0, coord.1, coord.2 - 1),
        ]
    }


    pub fn get_block(&self, world_coord: &WorldCoord) -> Option<Block> {
        // Find which chunk contains this block
        let chunk_coord = world_coord.to_chunk_coord();

        if let ActiveEntry::Loaded { chunk, .. } = self.get_active_entry(&chunk_coord) {
            let block_coord = world_coord.to_block_coord();
            Some(chunk.get_block(&block_coord))
        } else {
            None
        }
    }
    

    pub fn set_block(&mut self, world_coord: &WorldCoord, block: Block, device: &wgpu::Device, overwrite: bool) -> bool {
        // Find which chunk contains this block
        let chunk_coord = world_coord.to_chunk_coord();
        let block_coord = world_coord.to_block_coord();

        
        if self.get_active_entry(&chunk_coord).is_empty() {
            *self.get_active_entry_mut(&chunk_coord) = ActiveEntry::Loaded {
                coord: chunk_coord,
                chunk: Chunk::new_empty(),
                required_lod: 0,
                mesh: MeshGenerationState::Pending,
            };
        }

        
        let block_was_set = if let ActiveEntry::Loaded { chunk, .. } = self.get_active_entry_mut(&chunk_coord) && chunk.set_block(&block_coord, block, overwrite) {
            true
        } else {
            false
        };
        
        if block_was_set {
            let chunk_borders = self.get_chunk_borders(&chunk_coord);
            self.get_active_entry_mut(&chunk_coord).generate_and_upload_mesh(device, &chunk_borders);

            // Block was set successfully, if block is at border, only remesh corresponding neighbor
            // find out which neighbors need remeshing

            // index of highest block coordinate along each axis
            const HIGHER_BORDER: usize = (CHUNK_SIZE - 1) as usize;

            match block_coord {
                BlockCoord(0, _, _) => {
                    let neighbor_coord = ChunkCoord(chunk_coord.0 - 1, chunk_coord.1, chunk_coord.2);
                    let chunk_borders = self.get_chunk_borders(&neighbor_coord);
                    self.get_active_entry_mut(&neighbor_coord).generate_and_upload_mesh(device, &chunk_borders);
                },
                BlockCoord(HIGHER_BORDER, _, _) => {
                    let neighbor_coord = ChunkCoord(chunk_coord.0 + 1, chunk_coord.1, chunk_coord.2);
                    let chunk_borders = self.get_chunk_borders(&neighbor_coord);
                    self.get_active_entry_mut(&neighbor_coord).generate_and_upload_mesh(device, &chunk_borders);
                },
                BlockCoord(_, 0, _) => {
                    let neighbor_coord = ChunkCoord(chunk_coord.0, chunk_coord.1 - 1, chunk_coord.2);
                    let chunk_borders = self.get_chunk_borders(&neighbor_coord);
                    self.get_active_entry_mut(&neighbor_coord).generate_and_upload_mesh(device, &chunk_borders);
                },
                BlockCoord(_, HIGHER_BORDER, _) => {
                    let neighbor_coord = ChunkCoord(chunk_coord.0, chunk_coord.1 + 1, chunk_coord.2);
                    let chunk_borders = self.get_chunk_borders(&neighbor_coord);
                    self.get_active_entry_mut(&neighbor_coord).generate_and_upload_mesh(device, &chunk_borders);
                },
                BlockCoord(_, _, 0) => {
                    let neighbor_coord = ChunkCoord(chunk_coord.0, chunk_coord.1, chunk_coord.2 - 1);
                    let chunk_borders = self.get_chunk_borders(&neighbor_coord);
                    self.get_active_entry_mut(&neighbor_coord).generate_and_upload_mesh(device, &chunk_borders);
                },
                BlockCoord(_, _, HIGHER_BORDER) => {
                    let neighbor_coord = ChunkCoord(chunk_coord.0, chunk_coord.1, chunk_coord.2 + 1);
                    let chunk_borders = self.get_chunk_borders(&neighbor_coord);
                    self.get_active_entry_mut(&neighbor_coord).generate_and_upload_mesh(device, &chunk_borders);
                },
                _ => {}
            }

        }

        block_was_set
    }


    // gets called in each frame
    pub fn update(&mut self, player_position: &WorldCoord, device: &wgpu::Device, max_n_chunk_generations: usize, max_n_mesh_generations: usize) -> (usize, usize) {

        let position = player_position.to_chunk_coord();

        // Update sliding chunk window based on player position
        // this is almost free, as it only updates indices and does not generate anything
        self.slide_active(position);
        
        let n_chunk_generations = self.generate_chunks(&position, max_n_chunk_generations);
        let n_mesh_generations = self.generate_meshes(&position, device, max_n_mesh_generations);

        (n_chunk_generations, n_mesh_generations)
    }


    /// Update loaded chunks based on player movement
    /// Uses modulo-based indexing to implement a sliding 3D array around the player
    /// Only loads "surface" layers of chunks in the direction of movement
    fn slide_active(&mut self, new_player_coord: ChunkCoord) {
        
        // Find in which direction(s) the player moved
        let deltas = [
            new_player_coord.0 - self.previous_player_coord.0,
            new_player_coord.1 - self.previous_player_coord.1,
            new_player_coord.2 - self.previous_player_coord.2,
        ];

        // process each axis movement independently
        for (axis, movement_delta) in deltas.iter().enumerate() {

            // store direction of movement (+1 or -1)
            let step = if *movement_delta == 0 { continue; } // no movement along this axis -> skip
            else if *movement_delta > 0 { 1 } // moved in positive direction
            else { -1 }; // moved in negative direction

            log::debug!("Sliding chunks along axis {} by {}", axis, step);

            // track working position that gets updated each step (for multi-chunk teleports)
            let mut working_base = self.previous_player_coord;

            // process each step of movement separately
            for _ in 0..movement_delta.abs() {

                let half = self.active_size[axis] as isize / 2;

                // The plane to clear is at the edge OPPOSITE to the direction of movement
                // (these are the chunks being left behind as the player moves)
                // When moving +X (step=1): clear at working_base.0 - half (the negative edge)
                // When moving -X (step=-1): clear at working_base.0 + half (the positive edge)
                let plane_offset = -half * step;

                // iterate 2D plane perpendicular to the current axis
                for i in 0..self.active_size[(axis + 1) % 3] as isize {
                    for j in 0..self.active_size[(axis + 2) % 3] as isize {
                        
                        let chunk_coord = match axis {
                            0 => {
                                // move in x-axis: clear yz-plane at the correct edge
                                ChunkCoord(
                                    working_base.0 + plane_offset,
                                    working_base.1 + i - half,
                                    working_base.2 + j - half,
                                )
                            }
                            1 => {
                                // Y-axis: clear xz-plane
                                ChunkCoord(
                                    working_base.0 + j - half,
                                    working_base.1 + plane_offset,
                                    working_base.2 + i - half,
                                )
                            }
                            _ => {
                                // Z-axis: clear xy-plane
                                ChunkCoord(
                                    working_base.0 + i - half,
                                    working_base.1 + j - half,
                                    working_base.2 + plane_offset,
                                )
                            }
                        };
                        
                        // clear entry at this position
                        *self.get_active_entry_mut(&chunk_coord) = ActiveEntry::Pending;

                    }
                }

                // Update working base for next iteration (handles multi-chunk teleports)
                match axis {
                    0 => working_base.0 += step,
                    1 => working_base.1 += step,
                    _ => working_base.2 += step,
                }
            }
        }

        self.previous_player_coord = new_player_coord;
    }


    fn generate_chunks(&mut self, position: &ChunkCoord, chunk_generation_budget: usize) -> usize {
        let mut used_chunk_generation_budget = 0;

        // iterate in order of distance from player
        // Use index-based iteration to avoid cloning the entire Vec each frame
        for i in 0..self.sphere_offsets.len() {
            if used_chunk_generation_budget >= chunk_generation_budget {
                break;
            }

            let ((offset_x, offset_y, offset_z), distance) = self.sphere_offsets[i];

            let required_lod = select_lod(distance);
            let chunk_coord = ChunkCoord(
                position.0 + offset_x,
                position.1 + offset_y,
                position.2 + offset_z,
            ); 
            
            if self.get_active_entry(&chunk_coord).is_pending() {

                if chunk_coord.1 <= -2 || chunk_coord.1 >= 14 {
                    // Skip chunks that are guaranteed to be empty (above terrain + some margin)
                    // Terrain max height is ~200, so chunks starting at y=224 (chunk 14) are air-only
                    // Exception: clouds at y=255 (chunk 15), but those are handled separately if needed
                    *self.get_active_entry_mut(&chunk_coord) = ActiveEntry::Empty {
                        coord: chunk_coord,
                    };
                } else {
                    // generate new chunk
                    let new_chunk = Chunk::new_polulated(&self.density_generator, &chunk_coord);

                    used_chunk_generation_budget += 1;
                    self.insert_chunk(&chunk_coord, new_chunk, required_lod);
                }
            }
        }

        used_chunk_generation_budget
    }


    fn generate_meshes(&mut self, position: &ChunkCoord, device: &wgpu::Device, mesh_generation_budget: usize) -> usize {

        let mut used_mesh_generation_budget = 0;

        // iterate in order of distance from player
        // Use index-based iteration to avoid cloning the entire Vec each frame
        for i in 0..self.sphere_offsets.len() {
            if used_mesh_generation_budget >= mesh_generation_budget {
                break;
            }

            let ((offset_x, offset_y, offset_z), _) = self.sphere_offsets[i];
            
            let chunk_coord = ChunkCoord(
                position.0 + offset_x,
                position.1 + offset_y,
                position.2 + offset_z,
            ); 


            if self.get_active_entry(&chunk_coord).needs_remeshing() {
                // collect borders here before borrowing self mutably
                let chunk_borders = self.get_chunk_borders(&chunk_coord);

                self.get_active_entry_mut(&chunk_coord).generate_and_upload_mesh(device, &chunk_borders);
                used_mesh_generation_budget += 1;
            }
        }

        used_mesh_generation_budget
    }


    /// sum number of vertices and faces in all active mesh buffers
    pub fn get_n_vertices_and_faces(&self) -> (u32, u32) {
        let mut total_vertices = 0;
        let mut total_indices = 0;
        
        for active_entry in &self.active {
            if let ActiveEntry::Loaded { mesh: MeshGenerationState::Completed { buffer, .. }, .. } = active_entry {
                total_vertices += buffer.vertex_count;
                total_indices += buffer.index_count;
            }
        }
        
        // Each triangle has 3 indices, so faces = indices / 3
        let total_faces = total_indices / 3;
        (total_vertices, total_faces)
    }
}
