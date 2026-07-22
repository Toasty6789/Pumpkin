//! Village — overworld jigsaw village (all biome variants) (Minecraft 1.14+)
//!
//! Villages are generated using the jigsaw system with biome-specific template
//! pools. Each village type has its own set of buildings, streets, and town
//! centers. Zombie (abandoned) variants use the same pools with zombie processor lists.
//!
//! Village biome variants:
//! - plains:   `minecraft:village/plains/town_centers`
//! - desert:   `minecraft:village/desert/town_centers`
//! - savanna:  `minecraft:village/savanna/town_centers`
//! - snowy:    `minecraft:village/snowy/town_centers`
//! - taiga:    `minecraft:village/taiga/town_centers`
//!
//! Each variant shares the pool hierarchy pattern:
//! - {biome}/town_centers
//! - {biome}/houses
//! - {biome}/streets
//! - {biome}/terminators
//! - {biome}/decor (optional)
//! - {biome}/villagers (entity spawns)
//! - common/ (animals, iron_golem, well_bottoms, cats, sheep, butcher_animals)
//!
//! Zombie variant pools under {biome}/zombie/ use zombie processors
//! (zombie_plains, zombie_desert, zombie_savanna, zombie_snowy, zombie_taiga)
//! that replace villagers with zombie villagers.

use std::sync::Arc;
use pumpkin_data::structures::StructureKeys;
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

/// Returns the start pool for a village biome variant.
#[must_use]
pub fn village_start_pool(key: StructureKeys) -> &'static str {
    match key {
        StructureKeys::VillagePlains => "minecraft:village/plains/town_centers",
        StructureKeys::VillageDesert => "minecraft:village/desert/town_centers",
        StructureKeys::VillageSavanna => "minecraft:village/savanna/town_centers",
        StructureKeys::VillageSnowy => "minecraft:village/snowy/town_centers",
        StructureKeys::VillageTaiga => "minecraft:village/taiga/town_centers",
        _ => "minecraft:village/plains/town_centers",
    }
}

/// Places a village using the JigsawGenerator for the given biome key.
#[must_use]
pub fn generate_village(
    context: StructureGeneratorContext<'_>,
    key: StructureKeys,
) -> Option<StructurePosition> {
    let pool = village_start_pool(key);
    let size = pumpkin_data::structures::Structure::get(&key)
        .size
        .unwrap_or(6);

    let generator = super::jigsaw::JigsawGenerator::new(pool, size);
    generator.get_structure_position(context)
}

/// Generator for Plains Village structures.
pub struct VillagePlainsGenerator;
impl StructureGenerator for VillagePlainsGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        generate_village(context, StructureKeys::VillagePlains)
    }
}

/// Generator for Desert Village structures.
pub struct VillageDesertGenerator;
impl StructureGenerator for VillageDesertGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        generate_village(context, StructureKeys::VillageDesert)
    }
}

/// Generator for Savanna Village structures.
pub struct VillageSavannaGenerator;
impl StructureGenerator for VillageSavannaGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        generate_village(context, StructureKeys::VillageSavanna)
    }
}

/// Generator for Snowy Village structures.
pub struct VillageSnowyGenerator;
impl StructureGenerator for VillageSnowyGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        generate_village(context, StructureKeys::VillageSnowy)
    }
}

/// Generator for Taiga Village structures.
pub struct VillageTaigaGenerator;
impl StructureGenerator for VillageTaigaGenerator {
    fn get_structure_position(
        &self,
        context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        generate_village(context, StructureKeys::VillageTaiga)
    }
}

/// Piece representing a Village structure piece.
pub struct VillagePiece {
    pub piece: StructurePiece,
    pub biome: StructureKeys,
}

impl StructurePieceBase for VillagePiece {
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
        // Village pieces are placed by PoolElementStructurePiece via jigsaw engine.
        // Street processors handle terrain matching.
    }
}
