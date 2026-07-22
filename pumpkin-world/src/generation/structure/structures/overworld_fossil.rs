//! Overworld Fossil — buried bone block structure (Minecraft 1.18+)
//!
//! Overworld fossils generate underground in desert, swamp, and mangrove swamp biomes.
//! They use the same bone block structure templates as nether fossils but with
//! different Y levels and block replacement (diamond/deepslate vs coal/stone).
//!
//! This is the structure-level wrapper that uses the FossilFeature system
//! from pumpkin-world/src/generation/feature/features/fossil.rs.

use std::sync::Arc;

use pumpkin_data::{Block, BlockState, Mirror, Rotation, tag};
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
                StructurePiecesCollector, StructurePosition,
            },
            template::{BlockStateResolver, StructureTemplate, get_template},
        },
    },
};

/// Overworld fossil structure templates (diamond-level fossil ores).
const OVERWORLD_FOSSIL_STRUCTURES: &[&str] = &[
    "fossil/fossil_1",
    "fossil/fossil_2",
    "fossil/fossil_3",
    "fossil/fossil_4",
    "fossil/fossil_5",
    "fossil/fossil_6",
    "fossil/fossil_7",
];

/// Overlay templates (coal ore overlay for regular, diamond for deepslate variant).
const OVERWORLD_FOSSIL_OVERLAYS: &[&str] = &[
    "fossil/fossil_1_coal",
    "fossil/fossil_2_coal",
    "fossil/fossil_3_coal",
    "fossil/fossil_4_coal",
    "fossil/fossil_5_coal",
    "fossil/fossil_6_coal",
    "fossil/fossil_7_coal",
];

/// Deepslate fossil structures (diamond ore overlay).
const DEEPSLATE_FOSSIL_STRUCTURES: &[&str] = &[
    "fossil/fossil_1_deepslate",
    "fossil/fossil_2_deepslate",
    "fossil/fossil_3_deepslate",
    "fossil/fossil_4_deepslate",
    "fossil/fossil_5_deepslate",
    "fossil/fossil_6_deepslate",
    "fossil/fossil_7_deepslate",
];

/// Overlay templates for deepslate fossils (diamond ore).
const DEEPSLATE_FOSSIL_OVERLAYS: &[&str] = &[
    "fossil/fossil_1_diamond",
    "fossil/fossil_2_diamond",
    "fossil/fossil_3_diamond",
    "fossil/fossil_4_diamond",
    "fossil/fossil_5_diamond",
    "fossil/fossil_6_diamond",
    "fossil/fossil_7_diamond",
];

pub struct OverworldFossilGenerator {
    pub is_deepslate: bool,
}

impl OverworldFossilGenerator {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            is_deepslate: false,
        }
    }

    #[must_use]
    pub const fn deepslate() -> Self {
        Self {
            is_deepslate: true,
        }
    }
}

impl StructureGenerator for OverworldFossilGenerator {
    fn get_structure_position(
        &self,
        mut context: StructureGeneratorContext<'_>,
    ) -> Option<StructurePosition> {
        let x = start_block_x(context.chunk_x) + context.random.next_bounded_i32(16);
        let z = start_block_z(context.chunk_z) + context.random.next_bounded_i32(16);

        let rotation = Rotation::from_index(context.random.next_bounded_i32(4) as u8);

        let (fossil_pool, overlay_pool) = if self.is_deepslate {
            (DEEPSLATE_FOSSIL_STRUCTURES, DEEPSLATE_FOSSIL_OVERLAYS)
        } else {
            (OVERWORLD_FOSSIL_STRUCTURES, OVERWORLD_FOSSIL_OVERLAYS)
        };

        let template_idx = context.random.next_bounded_i32(fossil_pool.len() as i32) as usize;
        let fossil_name = fossil_pool[template_idx];
        let overlay_name = overlay_pool[template_idx];

        let fossil_template = get_template(fossil_name)?;
        let overlay_template = get_template(overlay_name)?;

        let rotated_size = rotation.transform_size(fossil_template.size);

        let bounding_box = BlockBox::new(
            x - rotated_size.x / 2,
            context.min_y,
            z - rotated_size.z / 2,
            x + rotated_size.x / 2,
            256,
            z + rotated_size.z / 2,
        );

        let mut collector = StructurePiecesCollector::default();
        collector.add_piece(Box::new(OverworldFossilPiece {
            piece: StructurePiece::new(
                StructurePieceType::OverworldFossil,
                bounding_box,
                0,
            ),
            fossil_template,
            overlay_template,
            rotation,
            is_deepslate: self.is_deepslate,
        }));

        Some(StructurePosition {
            start_pos: BlockPos::new(x, 64, z),
            collector: Arc::new(collector.into()),
        })
    }
}

pub struct OverworldFossilPiece {
    piece: StructurePiece,
    fossil_template: Arc<StructureTemplate>,
    overlay_template: Arc<StructureTemplate>,
    rotation: Rotation,
    is_deepslate: bool,
}

impl OverworldFossilPiece {
    fn place_template(
        &self,
        chunk: &mut ProtoChunk,
        template: &StructureTemplate,
        origin: Vector3<i32>,
    ) {
        for block in &template.blocks {
            let local_pos = self.rotation.transform_pos(block.pos, template.size);
            let world_pos = origin + local_pos;

            let palette_entry = &template.palette[block.state as usize];
            if palette_entry.name == "minecraft:structure_void" {
                continue;
            }

            if let Some(state) = BlockStateResolver::resolve(palette_entry, self.rotation, Mirror::default()) {
                chunk.set_block_state(world_pos.x, world_pos.y, world_pos.z, state);
            }
        }
    }
}

impl StructurePieceBase for OverworldFossilPiece {
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
        _block_registry: &dyn crate::world::WorldPortalExt,
        _random: &mut RandomGenerator,
        _seed: i64,
        chunk_box: &BlockBox,
    ) {
        let origin = Vector3::new(
            self.piece.bounding_box.min.x,
            self.piece.bounding_box.min.y,
            self.piece.bounding_box.min.z,
        );

        // Place fossil structure
        self.place_template(chunk, &self.fossil_template, origin);

        // Place overlay (coal ore or diamond ore)
        self.place_template(chunk, &self.overlay_template, origin);
    }
}
