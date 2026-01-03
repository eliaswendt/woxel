use std::rc::Rc;

use web_sys::console::log_1;

use crate::{world::{Block, Chunk}, utils::{ChunkCoord, Mesh, MeshBuffer, WorldCoord}};

use crate::world::VoxelDensityGenerator;



fn select_lod(distance_to_player: usize) -> LOD {
    if distance_to_player < 200 {
        0  // Full resolution
    } else if distance_to_player < 40 {
        1  // 1/2 resolution
    } else if distance_to_player < 50 {
        2  // 1/4 resolution
    } else if distance_to_player < 60 {
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

type LOD = u8;

/// Active entry: (Chunk, (LOD, MeshBuffer))
type ActiveEntry = (Chunk, (LOD, MeshBuffer));


pub struct Scene {
    /// States: 
    /// 
    /// None = chunk not loaded (option exists to allow sparse storage)
    /// 
    /// Some((Chunk, None)) = chunk loaded/generated but not meshed
    /// 
    /// Some((Chunk, Some((LOD, MeshBuffer)))) = chunk loaded and meshed
    pub active: Vec<Option<Rc<ActiveEntry>>>,

    /// Number of chunks along each axis in the active chunk grid
    active_size: [usize; 3],
    previous_player_chunk_coord: ChunkCoord,
    sphere_offsets: Vec<((isize, isize, isize), usize)>,

    empty_entry: Rc<ActiveEntry>,
    density_generator: VoxelDensityGenerator,
}

impl Scene {
    pub fn new(active_size: [usize; 3], device: &wgpu::Device) -> Self {
        // ensure chunk_distance is a power of two for modulo indexing
        // assert!(chunk_distance.is_power_of_two(), "chunk_distance must be a power of two");
        
        let mut active = Vec::new();

        for _ in 0..active_size[0] * active_size[1] * active_size[2] {
            active.push(None);
        }

        Self {
            active_size: active_size,
            active: active,
            previous_player_chunk_coord: ChunkCoord(0, 0, 0),

            empty_entry: Rc::new((Chunk::new_empty(), (0, Mesh::empty().upload(device)))),
            sphere_offsets: generate_qube_offset_in_spherical_order(active_size),
            density_generator: VoxelDensityGenerator::new(),
        }
    }


    fn active_idx(&self, coord: &ChunkCoord) -> usize {
        coord.0.rem_euclid(self.active_size[0] as isize) as usize + 
        coord.1.rem_euclid(self.active_size[1] as isize) as usize * self.active_size[0] + 
        coord.2.rem_euclid(self.active_size[2] as isize) as usize * self.active_size[0] * self.active_size[1] 
    }

    fn get_active(&self, coord: &ChunkCoord) -> Option<&ActiveEntry> {
        self.active[self.active_idx(coord)].as_deref()
    }

    fn get_active_mut(&mut self, coord: &ChunkCoord) -> Option<&mut ActiveEntry> {
        let active_idx = self.active_idx(coord);
        if let Some(entry) = &mut self.active[active_idx] {
            Rc::get_mut(entry)
        } else {
            None
        }
    }

    fn unset_active(&mut self, coord: &ChunkCoord) {
        let active_idx = self.active_idx(coord);
        self.active[active_idx] = None;
    }


    pub fn get_block(&self, world_coord: &WorldCoord) -> Option<Block> {
        // Find which chunk contains this block
        let chunk_coord = world_coord.to_chunk_coord();

        if let Some(active_entry) = self.get_active(&chunk_coord) {
            let block_coord = world_coord.to_block_coord();
            Some(active_entry.0.get_block(&block_coord))
        } else {
            None
        }
    }
    
    pub fn set_block(&mut self, world_coord: &WorldCoord, block: Block, overwrite: bool, device: &wgpu::Device) -> bool {
        // Find which chunk contains this block
        let chunk_coord = world_coord.to_chunk_coord();

        if let Some((active_chunk, (active_lod, active_mesh_buffer))) = self.get_active_mut(&chunk_coord) {

            let block_coord = world_coord.to_block_coord();

            if active_chunk.set_block(&block_coord, block, overwrite) {
                
                // upload new mesh to GPU
                let mut new_mesh = active_chunk.get_mesh(*active_lod);
                new_mesh.offset_vertices_by(&chunk_coord);
                *active_mesh_buffer = new_mesh.upload(device);

                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn update(&mut self, player: &WorldCoord, device: &wgpu::Device, compute_budget: usize) {

        let mut used_compute_budget = 0;

        // Update sliding chunk window based on player position
        self.slide_active_chunk_window(player.to_chunk_coord());

        // copy offsets to allow mutable borrow of self in the loop
        let sphere_offsets = self.sphere_offsets.clone();
        
        // iterate in order of distance from player
        for ((offset_x, offset_y, offset_z), distance) in sphere_offsets {

            let required_lod = select_lod(distance);

            let chunk_coord = ChunkCoord(
                player.to_chunk_coord().0 + offset_x,
                player.to_chunk_coord().1 + offset_y,
                player.to_chunk_coord().2 + offset_z,
            ); 
            let active_idx = self.active_idx(&chunk_coord);


            // this manual check is needed because Rc::get_mut() will return None for shared references (like our empty chunk)
            if self.active[active_idx].is_some() && Rc::ptr_eq(self.active[active_idx].as_ref().unwrap(), &self.empty_entry) {
                // it's the empty chunk - no LOD update needed
                continue;
            }


            if let Some((active_chunk, (active_lod, active_mesh_buffer))) = self.get_active_mut(&chunk_coord){
                // chunk is present -> check if LOD needs to be updated
                if !active_chunk.is_empty() && *active_lod != required_lod {
                    // println!("Updating LOD for Chunk {:?} from {} to {}", chunk_coord, *active_lod, required_lod);

                    let mut new_mesh = active_chunk.get_mesh(required_lod);
                    new_mesh.offset_vertices_by(&chunk_coord);
                    used_compute_budget += 1;

                    (*active_lod, *active_mesh_buffer) = (required_lod, new_mesh.upload(device));
                }

            } else {
                // log_1(&format!("self.active at {:?} is None", chunk_coord).into());
                // chunk is missing -> generate and mesh it
                let mut new_chunk = Chunk::new_polulated(&self.density_generator, &chunk_coord);
                // let mut new_chunk = Chunk::new_flat(&chunk_coord, Block::Grass);

                // now check whether the new chunk is empty
                // if empty, use air chunk instance (safes memory and GPU resources)
                // else compute mesh and upload to gpu
                let active_idx = self.active_idx(&chunk_coord);

                self.active[active_idx] = if new_chunk.is_empty() {
                    // log_1(&format!("Re-Using air chunk at {:?}", chunk_coord).into());
                    // instead of generating a new empty chunk, reuse the precomputed empty chunk
                    Some(self.empty_entry.clone())
                } else {
                    // log_1(&format!("Loading Chunk {:?} at LOD {}", chunk_coord, required_lod).into());
                    used_compute_budget += 2;
                    let mut new_mesh = new_chunk.get_mesh(required_lod);
                    new_mesh.offset_vertices_by(&chunk_coord);

                    Some(Rc::new((new_chunk, (required_lod, new_mesh.upload(device)))))
                };
            }

            if used_compute_budget >= compute_budget {
                break;
            }
        }
    }


    /// Update loaded chunks based on player movement
    /// Uses modulo-based indexing to implement a sliding 3D array around the player
    /// Only loads "surface" layers of chunks in the direction of movement
    pub fn slide_active_chunk_window(&mut self, player_chunk_coord: ChunkCoord) {
        
        // Find in which direction(s) the player moved
        let deltas = [
            player_chunk_coord.0 - self.previous_player_chunk_coord.0,
            player_chunk_coord.1 - self.previous_player_chunk_coord.1,
            player_chunk_coord.2 - self.previous_player_chunk_coord.2,
        ];

        // process each axis independently
        for (axis, movement_delta) in deltas.iter().enumerate() {

            // store direction of movement (+1 or -1)
            let step = if *movement_delta == 0 { continue; } // no movement along this axis -> skip
            else if *movement_delta > 0 { 1 } // moved in positive direction
            else { -1 }; // moved in negative direction

            // log_1(&format!("Sliding chunks along axis {} by {}", axis, step).into());

            // process each step of movement separately
            for _ in 0..movement_delta.abs() {

                let half = self.active_size[axis] as isize / 2;
                let prev_base = self.previous_player_chunk_coord;

                // The plane to clear is at the edge in the direction of movement
                // When moving +X (step=1): clear at prev_base.0 + half (the positive edge)
                // When moving -X (step=-1): clear at prev_base.0 - half (the negative edge)
                let plane_offset = half * step;

                // iterate 2D plane perpendicular to the current axis
                for i in 0..self.active_size[(axis + 1) % 3] as isize {
                    for j in 0..self.active_size[(axis + 2) % 3] as isize {
                        
                        let chunk_coord = match axis {
                            0 => {
                                // move in x-axis: clear yz-plane at the correct edge
                                ChunkCoord(
                                    prev_base.0 + plane_offset,
                                    prev_base.1 + i - half,
                                    prev_base.2 + j - half,
                                )
                            }
                            1 => {
                                // Y-axis: clear xz-plane
                                ChunkCoord(
                                    prev_base.0 + i - half,
                                    prev_base.1 + plane_offset,
                                    prev_base.2 + j - half,
                                )
                            }
                            _ => {
                                // Z-axis: clear xy-plane
                                ChunkCoord(
                                    prev_base.0 + i - half,
                                    prev_base.1 + j - half,
                                    prev_base.2 + plane_offset,
                                )
                            }
                        };
                        
                        // clear chunk at this position
                        self.unset_active(&chunk_coord);

                    }
                }
            }
        }

        self.previous_player_chunk_coord = player_chunk_coord;
    }
}
