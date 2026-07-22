//! Ancient City — deep dark jigsaw structure (Minecraft 1.19)
//!
//! The Ancient City is a sprawling underground city made of deepslate, reinforced
//! deepslate, and sculk blocks. It generates in the deep dark biome at Y=-27.
//!
//! Structure type: Jigsaw
//! Start pool: `minecraft:ancient_city/city_center`
//! Start jigsaw: `minecraft:city_anchor`
//! Size: 7
//! Terrain adaptation: BeardBox
//!
//! Pool hierarchy:
//! - ancient_city/city_center (3 entries, total weight 3)
//! - ancient_city/structures (20 entries, weight 46)
//! - ancient_city/walls (16 entries, weight 27)
//! - ancient_city/city/entrance (6 entries, weight 6)
//! - ancient_city/city_center/walls (10 entries, weight 10)
//! - ancient_city/walls/no_corners (8 entries, weight 8)
//! - ancient_city/sculk (2 entries, weight 7)
//!
//! Processors:
//! - ancient_city_generic_degradation (3 processors)
//! - ancient_city_start_degradation (2 processors)
//! - ancient_city_walls_degradation (3 processors)
//!
//! The jigsaw assembly is handled by JigsawGenerator in the parent module.
//! This module defines the AncientCityPiece for identification and serialization.

use std::sync::Arc;

use pumpkin_util::{
    math::{block_box::BlockBox, position::BlockPos},
    random::{RandomGenerator, RandomImpl},
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

/// Generator for Ancient City structures.
///
/// This is a jigsaw-based structure fully handled by the JigsawGenerator
/// routing in the parent mod.rs. This module exists for piece identification.
pub struct AncientCityGenerator;

impl StructureGenerator for AncientCityGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        // Ancient City is handled by JigsawGenerator in mod.rs
        // This generator is a fallback for direct usage.
        // The start pool and anchor are configured in the Structure data.
        let chunk_center_x =
            crate::generation::positions::chunk_pos::get_center_x(context.chunk_x);
        let chunk_center_z =
            crate::generation::positions::chunk_pos::get_center_z(context.chunk_z);

        // Use the standard jigsaw placement
        let generator = super::jigsaw::JigsawGenerator::new(
            "minecraft:ancient_city/city_center",
            7,
        )
        .with_start_jigsaw("minecraft:city_anchor");

        generator.get_structure_position(context)
    }
}

/// Piece representing an Ancient City structure.
pub struct AncientCityPiece {
    pub piece: StructurePiece,
}

impl StructurePieceBase for AncientCityPiece {
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
        // Ancient City pieces are placed by PoolElementStructurePiece via
        // the jigsaw assembly engine. This piece is kept for identification.
    }
}
