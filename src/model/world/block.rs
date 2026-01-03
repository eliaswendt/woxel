#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Block {
    Empty = 0,
    Grass = 1,
    Dirt = 2,
    Stone = 3,
    Sand = 4,
    Gravel = 5,
    Cobblestone = 6,
    Bedrock = 7,
    OakLeaves = 8,
    Wood = 9,
    Water = 10,
    Cloud = 11,
    Snow = 12,
    Ice = 13,
    CoalOre = 14,
    IronOre = 15,
    GoldOre = 16,
    DiamondOre = 17,
    Granite = 18,
    Sandstone = 19,
    Clay = 20,
    SpruceLeaves = 21,
    SpruceWood = 22,
    BirchLeaves = 23,
    BirchWood = 24,
    Cactus = 25,
    DeadBush = 26,
    RedFlower = 27,
    YellowFlower = 28,
    Moss = 29,
    // New plant blocks
    Grass_Tall = 30,
    Grass_Short = 31,
    SeaGrass = 32,
    // New tree types
    AcaciaLeaves = 33,
    AcaciaWood = 34,
    DarkOakLeaves = 35,
    DarkOakWood = 36,
    // Water variants
    LakeWater = 37,
    // Cliff blocks
    Basalt = 38,
    BlackStone = 39,
}

impl Block {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Block::Empty,
            1 => Block::Grass,
            2 => Block::Dirt,
            3 => Block::Stone,
            4 => Block::Sand,
            5 => Block::Gravel,
            6 => Block::Cobblestone,
            7 => Block::Bedrock,
            8 => Block::OakLeaves,
            9 => Block::Wood,
            10 => Block::Water,
            11 => Block::Cloud,
            12 => Block::Snow,
            13 => Block::Ice,
            14 => Block::CoalOre,
            15 => Block::IronOre,
            16 => Block::GoldOre,
            17 => Block::DiamondOre,
            18 => Block::Granite,
            19 => Block::Sandstone,
            20 => Block::Clay,
            21 => Block::SpruceLeaves,
            22 => Block::SpruceWood,
            23 => Block::BirchLeaves,
            24 => Block::BirchWood,
            25 => Block::Cactus,
            26 => Block::DeadBush,
            27 => Block::RedFlower,
            28 => Block::YellowFlower,
            29 => Block::Moss,
            30 => Block::Grass_Tall,
            31 => Block::Grass_Short,
            32 => Block::SeaGrass,
            33 => Block::AcaciaLeaves,
            34 => Block::AcaciaWood,
            35 => Block::DarkOakLeaves,
            36 => Block::DarkOakWood,
            37 => Block::LakeWater,
            38 => Block::Basalt,
            39 => Block::BlackStone,
            _ => Block::Empty,
        }
    }
    
    pub fn to_u8(self) -> u8 {
        self as u8
    }
    
    pub fn is_empty(self) -> bool {
        self == Block::Empty
    }

    pub fn is_solid(self) -> bool {
        !matches!(self, Block::Empty | Block::Water | Block::Cloud)
    }
    
    pub fn color(self, face_dir: u8) -> [f32; 4] {
        match self {
            Block::Empty => [0.0, 0.0, 0.0, 1.0],
            Block::Grass => {
                match face_dir {
                    2 => [0.3, 0.8, 0.2, 1.0],    // +Y top: light green
                    _ => [0.6, 0.4, 0.2, 1.0],    // sides/bottom: brown
                }
            }
            Block::Dirt => [0.6, 0.4, 0.2, 1.0],
            Block::Stone => [0.5, 0.5, 0.5, 1.0],
            Block::Sand => [0.9, 0.85, 0.3, 1.0],
            Block::Gravel => [0.6, 0.55, 0.4, 1.0],
            Block::Cobblestone => [0.4, 0.4, 0.4, 1.0],
            Block::Bedrock => [0.2, 0.2, 0.2, 1.0],
            Block::OakLeaves => [0.2, 0.6, 0.2, 1.0],
            Block::Wood => [0.5, 0.3, 0.1, 1.0],
            Block::Water => [0.0, 0.1, 0.4, 1.0],
            Block::Cloud => [0.95, 0.95, 0.95, 0.7],
            Block::Snow => [0.95, 0.97, 1.0, 1.0],
            Block::Ice => [0.6, 0.8, 0.95, 0.7],
            Block::CoalOre => [0.3, 0.3, 0.3, 1.0],
            Block::IronOre => [0.7, 0.6, 0.5, 1.0],
            Block::GoldOre => [0.9, 0.8, 0.2, 1.0],
            Block::DiamondOre => [0.4, 0.7, 0.8, 1.0],
            Block::Granite => [0.65, 0.5, 0.45, 1.0],
            Block::Sandstone => [0.85, 0.75, 0.5, 1.0],
            Block::Clay => [0.65, 0.65, 0.7, 1.0],
            Block::SpruceLeaves => [0.15, 0.4, 0.2, 1.0],
            Block::SpruceWood => [0.35, 0.25, 0.15, 1.0],
            Block::BirchLeaves => [0.3, 0.7, 0.3, 1.0],
            Block::BirchWood => [0.85, 0.85, 0.75, 1.0],
            Block::Cactus => [0.25, 0.55, 0.25, 1.0],
            Block::DeadBush => [0.6, 0.5, 0.3, 1.0],
            Block::RedFlower => [0.9, 0.2, 0.2, 1.0],
            Block::YellowFlower => [0.95, 0.9, 0.3, 1.0],
            Block::Moss => [0.35, 0.6, 0.35, 1.0],
            Block::Grass_Tall => [0.25, 0.7, 0.25, 1.0],
            Block::Grass_Short => [0.3, 0.65, 0.3, 1.0],
            Block::SeaGrass => [0.2, 0.5, 0.4, 1.0],
            Block::AcaciaLeaves => [0.5, 0.65, 0.2, 1.0],
            Block::AcaciaWood => [0.6, 0.4, 0.2, 1.0],
            Block::DarkOakLeaves => [0.1, 0.35, 0.15, 1.0],
            Block::DarkOakWood => [0.3, 0.2, 0.1, 1.0],
            Block::LakeWater => [0.0, 0.15, 0.5, 1.0],
            Block::Basalt => [0.3, 0.3, 0.35, 1.0],
            Block::BlackStone => [0.25, 0.25, 0.28, 1.0],
        }
    }
}

// Convert face direction to normal vector
pub fn face_dir_to_normal(face_dir: u8) -> [f32; 3] {
    match face_dir {
        0 => [1.0, 0.0, 0.0],   // +X
        1 => [-1.0, 0.0, 0.0],  // -X
        2 => [0.0, 1.0, 0.0],   // +Y
        3 => [0.0, -1.0, 0.0],  // -Y
        4 => [0.0, 0.0, 1.0],   // +Z
        5 => [0.0, 0.0, -1.0],  // -Z
        _ => [0.0, 1.0, 0.0],
    }
}
