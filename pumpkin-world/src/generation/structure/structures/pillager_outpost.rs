//! Pillager Outpost — overworld jigsaw structure (Minecraft 1.14)
//!
//! Pillager outposts are tower structures guarded by pillagers. They generate
//! in all overworld biomes where pillagers can spawn.
//!
//! Structure type: Jigsaw
//! Start pool: `minecraft:pillager_outpost/base_plates`
//! Size: 7
//!
//! Pool hierarchy:
//! - pillager_outpost/base_plates (base plate for the outpost)
//! - pillager_outpost/towers (the main watchtower)
//! - pillager_outpost/feature_plates (plates for attaching cage features)
//! - pillager_outpost/features (cages, tents, targets, logs)
//!
//! Processor: outpost_rot (rubble/rotation)

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

/// Generator for Pillager Outpost structures.
pub struct PillagerOutpostGenerator;

impl StructureGenerator for PillagerOutpostGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        let generator = super::jigsaw::JigsawGenerator::new(
            "minecraft:pillager_outpost/base_plates",
            7,
        );
        generator.get_structure_position(context)
    }
}

pub struct PillagerOutpostPiece {
    pub piece: StructurePiece,
}

impl StructurePieceBase for PillagerOutpostPiece {
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
