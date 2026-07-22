//! Bastion Remnant — nether jigsaw structure (Minecraft 1.16)
//!
//! Bastion remnants are large player-created structures in the Nether made of
//! blackstone, basalt, and gold blocks. They generate in Crimson Forest and
//! Nether Wastes biomes.
//!
//! Structure type: Jigsaw (structure_type: Jigsaw)
//! Start pool: `minecraft:bastion/starts`
//! Size: 6
//! Start height: 33
//!
//! Pool groups:
//! - bastion/starts
//! - bastion/bridge/* (starting_pieces, bridge_pieces, connectors, legs, ramparts, walls)
//! - bastion/hoglin_stable/* (starting_pieces, large_stables, small_stables, connectors, ...)
//! - bastion/treasure/* (bases, brains, corners, entrances, extensions, ramparts, roofs, stairs, walls)
//! - bastion/units/* (center_pieces, edges, fillers, pathways, stages, wall_units, walls)
//! - bastion/blocks/gold (for gold block replacement in structures)
//!
//! Processors: bastion_generic_degradation, bottom_rampart, bridge, high_rampart,
//!             high_wall, housing, rampart_degradation, roof, side_wall_degradation,
//!             stable_degradation, treasure_rooms

use std::sync::Arc;

use pumpkin_util::{
    math::{block_box::BlockBox, position::BlockPos},
    random::RandomGenerator,
};

use crate::{
    ProtoChunk,
    generation::{
        structure::{
            piece::StructurePieceType,
            structures::{
                StructureGenerator, StructureGeneratorContext, StructurePiece, StructurePieceBase,
                StructurePiecesCollector, StructurePosition, WorldPortalExt,
            },
        },
    },
};

/// Generator for Bastion Remnant structures.
pub struct BastionRemnantGenerator;

impl StructureGenerator for BastionRemnantGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        let generator = super::jigsaw::JigsawGenerator::new(
            "minecraft:bastion/starts",
            6,
        );
        generator.get_structure_position(context)
    }
}

pub struct BastionRemnantPiece {
    pub piece: StructurePiece,
}

impl StructurePieceBase for BastionRemnantPiece {
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
        _chunk: &mut ProtoChunk,
        _block_registry: &dyn WorldPortalExt,
        _random: &mut RandomGenerator,
        _seed: i64,
        _chunk_box: &BlockBox,
    ) {
    }
}
