//! Trail Ruins — brushable block structure (Minecraft 1.20)
//!
//! Trail ruins are buried structures made of mud bricks, packed mud, gravel,
//! and suspicious gravel blocks containing archaeology loot. They generate in
//! jungle, old growth taiga, snowy taiga, and badlands biomes.
//!
//! Structure type: Jigsaw
//! Start pool: `minecraft:trail_ruins/tower/tower_top`
//! Size: 7
//! Start jigsaw: `minecraft:bottom`
//!
//! Pool hierarchy:
//! - trail_ruins/tower/tower_top (tower top piece)
//! - trail_ruins/tower/additions (tower side additions)
//! - trail_ruins/buildings (main buildings)
//! - trail_ruins/buildings/grouped (grouped building variants)
//! - trail_ruins/roads (road pieces)
//! - trail_ruins/decor (decorative pieces)
//!
//! Processors:
//! - trail_ruins_houses_archaeology (3 processors)
//! - trail_ruins_roads_archaeology
//! - trail_ruins_tower_top_archaeology

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

/// Generator for Trail Ruins structures.
pub struct TrailRuinsGenerator;

impl StructureGenerator for TrailRuinsGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        let generator = super::jigsaw::JigsawGenerator::new(
            "minecraft:trail_ruins/tower/tower_top",
            7,
        )
        .with_start_jigsaw("minecraft:bottom");

        generator.get_structure_position(context)
    }
}

pub struct TrailRuinsPiece {
    pub piece: StructurePiece,
}

impl StructurePieceBase for TrailRuinsPiece {
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
        // Trail Ruins are placed by the jigsaw assembly engine.
        // Processors handle suspicious gravel replacement for archaeology.
    }
}
