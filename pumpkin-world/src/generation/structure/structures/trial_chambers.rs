//! Trial Chambers — trial spawner structure (Minecraft 1.21)
//!
//! Trial chambers are large underground structures made of tuff bricks, copper,
//! and trial spawners. They have various chamber types and spawners for different
//! mobs. They generate deep underground (around Y=-40).
//!
//! Structure type: Jigsaw
//! Start pool: `minecraft:trial_chambers/corridor`
//! Size: 7
//! Start jigsaw: `minecraft:entrance`
//!
//! Pool hierarchy:
//! - trial_chambers/corridor (first corridor piece)
//! - trial_chambers/corridor/slices (corridor slice pieces)
//! - trial_chambers/corridor/atrium (atrium rooms off corridors)
//! - trial_chambers/hallway (hallway connections)
//! - trial_chambers/intersection (intersection pieces)
//! - trial_chambers/chamber/* (chamber, addon, assembly, end, eruption, pedestal, slanted)
//! - trial_chambers/spawner/* (breeze, melee, ranged, slow_ranged, small_melee)
//! - trial_chambers/decor (decorative elements)
//! - trial_chambers/dispensers (dispenser/supply pieces)
//! - trial_chambers/chests (chest pieces)
//! - trial_chambers/reward (reward vault pieces)
//!
//! Processor: trial_chambers_copper_bulb_degradation
//!
//! Trial chambers use an expansion hack to ensure proper room height.

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

/// Generator for Trial Chambers structures.
pub struct TrialChambersGenerator;

impl StructureGenerator for TrialChambersGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        // Trial chambers use an expansion hack for space generation
        let mut generator = super::jigsaw::JigsawGenerator::new(
            "minecraft:trial_chambers/corridor",
            7,
        )
        .with_start_jigsaw("minecraft:entrance")
        .with_expansion_hack(true);

        generator.get_structure_position(context)
    }
}

pub struct TrialChambersPiece {
    pub piece: StructurePiece,
}

impl StructurePieceBase for TrialChambersPiece {
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
        // Placed by jigsaw engine with copper bulb degradation processor
    }
}
