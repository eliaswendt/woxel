use crate::model::{CHUNK_SIZE, Chunk};
use wgpu::util::DeviceExt;
use bytemuck::{NoUninit};

#[repr(C)]
#[derive(Debug, Clone, Copy, NoUninit)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

pub struct MeshBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn empty() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() && self.indices.is_empty()
    }

    // scale and offset vertices 
    pub fn offset_vertices_by(&mut self, coord: &ChunkCoord) {

        let coord = coord.to_world_coord();
        for v in self.vertices.iter_mut() {
            v.pos[0] = coord.0 as f32 + v.pos[0];
            v.pos[1] = coord.1 as f32 + v.pos[1];
            v.pos[2] = coord.2 as f32 + v.pos[2];
        }
    }

    pub fn upload(&self, device: &wgpu::Device) -> MeshBuffer {

        let vertices = bytemuck::cast_slice(&self.vertices);
        let indices = bytemuck::cast_slice(&self.indices);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Vertex Buffer"),
            contents: vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Index Buffer"),
            contents: indices,
            usage: wgpu::BufferUsages::INDEX,
        });

        MeshBuffer {
            vertex_buffer,
            index_buffer,
            index_count: self.indices.len() as u32,
        }
    }
}



/// Create outline mesh for block targeting (unit cube at origin)
pub fn create_outline_mesh() -> Mesh {
    let verts = vec![
        Vertex { pos: [0.0, 0.0, 0.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [0.0, 0.0] },
        Vertex { pos: [1.0, 0.0, 0.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [1.0, 0.0] },
        Vertex { pos: [1.0, 1.0, 0.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [1.0, 1.0] },
        Vertex { pos: [0.0, 1.0, 0.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [0.0, 1.0] },
        Vertex { pos: [0.0, 0.0, 1.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [0.0, 0.0] },
        Vertex { pos: [1.0, 0.0, 1.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [1.0, 0.0] },
        Vertex { pos: [1.0, 1.0, 1.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [1.0, 1.0] },
        Vertex { pos: [0.0, 1.0, 1.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 0.3, 1.0], uv: [0.0, 1.0] },
    ];
    let indices = vec![
        0, 1, 1, 2, 2, 3, 3, 0, // bottom
        4, 5, 5, 6, 6, 7, 7, 4, // top
        0, 4, 1, 5, 2, 6, 3, 7, // sides
    ];
    
    Mesh { vertices: verts, indices: indices }
}

/// Create chunk border mesh (edges of a chunk in world space)
pub fn create_chunk_border_mesh(chunk_size: i32) -> Mesh {
    let s = chunk_size as f32;
    let color = [0.2, 0.8, 0.2, 1.0]; // Green for chunk borders
    
    let verts = vec![
        // Bottom face corners
        Vertex { pos: [0.0, 0.0, 0.0], normal: [0.0, 1.0, 0.0], color, uv: [0.0, 0.0] },
        Vertex { pos: [s, 0.0, 0.0], normal: [0.0, 1.0, 0.0], color, uv: [1.0, 0.0] },
        Vertex { pos: [s, 0.0, s], normal: [0.0, 1.0, 0.0], color, uv: [1.0, 1.0] },
        Vertex { pos: [0.0, 0.0, s], normal: [0.0, 1.0, 0.0], color, uv: [0.0, 1.0] },
        // Top face corners
        Vertex { pos: [0.0, s, 0.0], normal: [0.0, 1.0, 0.0], color, uv: [0.0, 0.0] },
        Vertex { pos: [s, s, 0.0], normal: [0.0, 1.0, 0.0], color, uv: [1.0, 0.0] },
        Vertex { pos: [s, s, s], normal: [0.0, 1.0, 0.0], color, uv: [1.0, 1.0] },
        Vertex { pos: [0.0, s, s], normal: [0.0, 1.0, 0.0], color, uv: [0.0, 1.0] },
    ];
    
    let indices = vec![
        // Bottom face edges
        0, 1, 1, 2, 2, 3, 3, 0,
        // Top face edges
        4, 5, 5, 6, 6, 7, 7, 4,
        // Vertical edges
        0, 4, 1, 5, 2, 6, 3, 7,
    ];
    
    Mesh { vertices: verts, indices }
}

/// coordinates of a block in world space
#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
pub struct WorldCoord(pub isize, pub isize, pub isize);

impl WorldCoord {
    pub fn squared_distance(&self, other: &WorldCoord) -> isize {
        (self.0 - other.0).pow(2) +
        (self.1 - other.1).pow(2) + 
        (self.2 - other.2).pow(2)
    }

    /// Convert to chunk index
    pub fn to_chunk_coord(&self) -> ChunkCoord {
        ChunkCoord(
            self.0 / CHUNK_SIZE as isize,
            self.1 / CHUNK_SIZE as isize,
            self.2 / CHUNK_SIZE as isize,
        )
    }

    /// Convert to chunk-local coordinates
    pub fn to_block_coord(&self) -> BlockCoord {
        BlockCoord(
            (self.0.rem_euclid(CHUNK_SIZE as isize)) as usize,
            (self.1.rem_euclid(CHUNK_SIZE as isize)) as usize,
            (self.2.rem_euclid(CHUNK_SIZE as isize)) as usize,
        )
    }
}


// coordinates of a chunk in chunk space
#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
pub struct ChunkCoord(pub isize, pub isize, pub isize);

impl ChunkCoord {
    /// Convert to chunk world key (the corner position in world block coordinates)
    pub fn to_world_coord(&self) -> WorldCoord {
        WorldCoord(
            self.0 * CHUNK_SIZE as isize,
            self.1 * CHUNK_SIZE as isize,
            self.2 * CHUNK_SIZE as isize,
        )
    }
}

/// Chunk-local block coordinates (0-15 for each component)
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BlockCoord(pub usize, pub usize, pub usize);

impl BlockCoord {

    pub fn get_block_idx(&self) -> usize {
        self.0 + self.1 * CHUNK_SIZE as usize + self.2 * CHUNK_SIZE as usize * CHUNK_SIZE as usize
    }
}

