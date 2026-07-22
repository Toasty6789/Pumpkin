//! Desert Well — simple structure for desert biomes (Minecraft)
//!
//! A simple desert well structure made of sandstone and sandstone slabs with
//! water at the bottom. This is a structure-level wrapper that delegates to
//! the DesertWellFeature for block placement.

use std::sync::Arc;

use pumpkin_data::Block;
use pumpkin_util::{
    HeightMap,
    math::{block_box::BlockBox, position::BlockPos, vector3::Vector3},
    random::{RandomGenerator, RandomImpl},
};

use crate::{
    ProtoChunk,
    generation::{
        positions::chunk_pos::{start_block_x, start_block_z},
        structure::{
            piece::StructurePieceType,
            structures::{
                StructureGenerator, StructureGeneratorContext, StructurePiece, StructurePieceBase,
                StructurePiecesCollector, StructurePosition, WorldPortalExt,
            },
        },
    },
};

fn try_generate_well_in_chunk(
    chunk: &mut ProtoChunk,
    x: i32,
    z: i32,
    chunk_box: &BlockBox,
) -> bool {
    let mut y = chunk.get_top_y(&HeightMap::WorldSurfaceWg, x, z);

    // Check if the surface block is sand
    let base_pos = Vector3::new(x, y, z);
    let surface_block = chunk.get_block_state(&base_pos).to_block_id();
    if surface_block != Block::SAND.id {
        return false;
    }

    // Check 5x5 area for air above
    for dx in -2..=2 {
        for dz in -2..=2 {
            if !chunk.is_air(&Vector3::new(x + dx, y, z + dz))
                || !chunk.is_air(&Vector3::new(x + dx, y + 1, z + dz))
            {
                return false;
            }
        }
    }

    let sand = Block::SANDSTONE.default_state;
    let slab = Block::SANDSTONE_SLAB.default_state;
    let water = Block::WATER.default_state;

    // Build the well structure
    // Floor
    for dx in -2..=2 {
        for dz in -2..=2 {
            let pos = Vector3::new(x + dx, y - 1, z + dz);
            if chunk_box.contains_pos(&pos) {
                chunk.set_block_state(pos.x, pos.y, pos.z, sand);
            }
        }
    }

    // Walls - two layers tall
    for dy in 0..=1 {
        for dx in -2..=2 {
            for dz in -2..=2 {
                if (dx == -2 || dx == 2 || dz == -2 || dz == 2) && !(dx == -2 && dz == -2)
                    && !(dx == -2 && dz == 2) && !(dx == 2 && dz == -2)
                    && !(dx == 2 && dz == 2)
                {
                    let pos = Vector3::new(x + dx, y + dy, z + dz);
                    if chunk_box.contains_pos(&pos) {
                        chunk.set_block_state(pos.x, pos.y, pos.z, sand);
                    }
                }
            }
        }
    }

    // Water at bottom
    let water_pos = Vector3::new(x, y, z);
    if chunk_box.contains_pos(&water_pos) {
        chunk.set_block_state(water_pos.x, water_pos.y, water_pos.z, water);
    }

    // Slab on top of walls
    for dx in -2..=2 {
        for dz in -2..=2 {
            if (dx == -2 || dx == 2 || dz == -2 || dz == 2) && !(dx == -2 && dz == -2)
                && !(dx == -2 && dz == 2) && !(dx == 2 && dz == -2)
                && !(dx == 2 && dz == 2)
            {
                let pos = Vector3::new(x + dx, y + 2, z + dz);
                if chunk_box.contains_pos(&pos) {
                    chunk.set_block_state(pos.x, pos.y, pos.z, slab);
                }
            }
        }
    }

    true
}

pub struct DesertWellGenerator;

impl StructureGenerator for DesertWellGenerator {
    fn get_structure_position(
        &self,
        mut context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        let x = start_block_x(context.chunk_x) + context.random.next_bounded_i32(16);
        let z = start_block_z(context.chunk_z) + context.random.next_bounded_i32(16);

        let bounding_box = BlockBox::new(x - 2, 0, z - 2, x + 2, 255, z + 2);

        let mut collector = StructurePiecesCollector::default();
        collector.add_piece(Box::new(DesertWellPiece {
            piece: StructurePiece::new(
                StructurePieceType::DesertWell,
                bounding_box,
                0,
            ),
        }));

        Some(StructurePosition {
            start_pos: BlockPos::new(x, 64, z),
            collector: Arc::new(collector.into()),
        })
    }
}

pub struct DesertWellPiece {
    piece: StructurePiece,
}

impl StructurePieceBase for DesertWellPiece {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn get_structure_piece(&self) -> &StructurePiece {
        &self.piece
    }
    fn get_structure_piece_mut(&mut self) -> &mut StructurePiece {
        &mut self.piece
    }
    fn place(
        &mut self,
        chunk: &mut ProtoChunk,
        _block_registry: &dyn WorldPortalExt,
        _random: &mut RandomGenerator,
        _seed: i64,
        chunk_box: &BlockBox,
    ) {
        let origin = self.piece.bounding_box.min;
        try_generate_well_in_chunk(chunk, origin.x, origin.z, chunk_box);
    }
}
