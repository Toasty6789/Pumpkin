use decorator::TreeDecorator;
use foliage::FoliagePlacer;
use pumpkin_data::BlockState;
use pumpkin_data::{
    BlockDirection, BlockId,
    block_properties::{BlockProperties, OakLeavesLikeProperties},
    tag,
};
use pumpkin_util::{math::position::BlockPos, random::RandomGenerator};
use root::RootPlacer;
use std::collections::{HashSet, VecDeque};

use trunk::TrunkPlacer;

use crate::generation::proto_chunk::GenerationCache;
use crate::generation::{block_state_provider::BlockStateProvider, feature::size::FeatureSize};
use crate::world::WorldPortalExt;

pub mod decorator;
pub mod foliage;
pub mod root;
pub mod trunk;

pub struct TreeFeature {
    pub trunk_provider: BlockStateProvider,
    pub trunk_placer: TrunkPlacer,
    pub foliage_provider: BlockStateProvider,
    pub foliage_placer: FoliagePlacer,
    pub minimum_size: FeatureSize,
    pub ignore_vines: bool,
    pub decorators: Vec<TreeDecorator>,
    pub below_trunk_provider: BlockStateProvider,
    pub root_placer: Option<RootPlacer>,
}

pub struct TreeNode {
    center: BlockPos,
    foliage_radius: i32,
    giant_trunk: bool,
}

impl TreeFeature {
    #[expect(clippy::too_many_arguments)]
    pub fn generate<T: GenerationCache>(
        &self,
        block_registry: &dyn WorldPortalExt,
        chunk: &mut T,
        min_y: i8,
        height: u16,
        feature_name: pumpkin_data::placed_feature::PlacedFeature, // This placed feature
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let (log_positions, root_positions, foliage_positions) = self.generate_main(
            block_registry,
            chunk,
            min_y,
            height,
            feature_name,
            random,
            pos,
        );

        for decorator in &self.decorators {
            decorator.generate(
                chunk,
                block_registry,
                random,
                &root_positions,
                &log_positions,
                &foliage_positions,
            );
        }
        Self::update_leaf_distances(chunk, &log_positions, &foliage_positions);
        true
    }

    fn update_leaf_distances<T: GenerationCache>(
        chunk: &mut T,
        log_positions: &[BlockPos],
        foliage_positions: &[BlockPos],
    ) {
        let foliage = foliage_positions.iter().copied().collect::<HashSet<_>>();
        let mut visited = log_positions.iter().copied().collect::<HashSet<_>>();
        let mut queue = log_positions
            .iter()
            .copied()
            .map(|pos| (pos, 0u8))
            .collect::<VecDeque<_>>();

        while let Some((pos, distance)) = queue.pop_front() {
            if distance >= 6 {
                continue;
            }
            let leaf_distance = distance + 1;
            for direction in BlockDirection::all() {
                let neighbor = pos.offset(direction.to_offset());
                if !foliage.contains(&neighbor) || !visited.insert(neighbor) {
                    continue;
                }

                let state_id = GenerationCache::get_block_state(chunk, &neighbor.0);
                let block = state_id.to_block();
                if !block.id.has_tag(tag::Block::MINECRAFT_LEAVES) {
                    continue;
                }
                let mut properties = OakLeavesLikeProperties::from_state_id(state_id, block);
                properties.distance = leaf_distance;
                chunk.set_block_state(&neighbor.0, properties.to_state_id(block).to_state());
                queue.push_back((neighbor, leaf_distance));
            }
        }
    }

    pub fn can_replace_or_log(state: &BlockState, id: BlockId) -> bool {
        Self::can_replace(state, id) || id.has_tag(tag::Block::MINECRAFT_LOGS)
    }

    pub fn is_air_or_leaves(state: &BlockState, id: BlockId) -> bool {
        state.is_air() || id.has_tag(tag::Block::MINECRAFT_LEAVES)
    }

    pub fn can_replace(state: &BlockState, id: BlockId) -> bool {
        state.is_air() || id.has_tag(tag::Block::MINECRAFT_REPLACEABLE_BY_TREES)
    }

    #[expect(clippy::too_many_arguments)]
    fn generate_main<T: GenerationCache>(
        &self,
        block_registry: &dyn WorldPortalExt,
        chunk: &mut T,
        _min_y: i8,
        _height: u16,
        _feature_name: pumpkin_data::placed_feature::PlacedFeature, // This placed feature
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> (Vec<BlockPos>, Vec<BlockPos>, Vec<BlockPos>) {
        let height = self.trunk_placer.get_height(random);

        let trunk_start = self
            .root_placer
            .as_ref()
            .map_or(pos, |placer| placer.trunk_offset(pos, random));

        let clipped_height = self.minimum_size.min_clipped_height;
        let top = self.get_top(height, chunk, trunk_start);
        if top < height && top < clipped_height.map_or(u32::MAX, |h| h as u32) {
            return (vec![], vec![], vec![]);
        }

        let root_positions = if let Some(placer) = &self.root_placer {
            match placer.generate(chunk, block_registry, random, pos, trunk_start) {
                Some(positions) => positions,
                None => return (vec![], vec![], vec![]),
            }
        } else {
            Vec::new()
        };

        let trunk_state = self.trunk_provider.get(random, pos, chunk, block_registry);

        let (nodes, logs) = self.trunk_placer.generate(
            block_registry,
            top,
            trunk_start,
            chunk,
            random,
            &self.below_trunk_provider,
            trunk_state,
        );

        let foliage_height = self
            .foliage_placer
            .r#type
            .get_random_height(random, height as i32);
        let base_height = height as i32 - foliage_height;
        let foliage_radius = self.foliage_placer.get_random_radius(random, base_height);
        let foliage_state = self
            .foliage_provider
            .get(random, pos, chunk, block_registry);
        let mut foliage_positions = Vec::new();
        for node in nodes {
            foliage_positions.extend(self.foliage_placer.generate(
                chunk,
                random,
                &node,
                foliage_height,
                foliage_radius,
                foliage_state,
            ));
        }
        (logs, root_positions, foliage_positions)
    }

    fn get_top<T: GenerationCache>(&self, height: u32, chunk: &T, init_pos: BlockPos) -> u32 {
        for y in 0..=height + 1 {
            let j = self.minimum_size.r#type.get_radius(height, y as i32);
            for x in -j..=j {
                for z in -j..=j {
                    let pos = BlockPos(init_pos.0.add_raw(x, y as i32, z));
                    let rstate = GenerationCache::get_block_state(chunk, &pos.0);
                    let block = rstate.to_block_id();
                    if Self::can_replace_or_log(rstate.to_state(), block)
                        && (self.ignore_vines || block != BlockId::VINE)
                    {
                        continue;
                    }
                    return y.saturating_sub(2);
                }
            }
        }
        height
    }
}

#[cfg(test)]
mod tests {
    use super::TreeFeature;
    use crate::generation::{
        generator::{WorldGenerator, flat::FlatGenerator},
        proto_chunk::{GenerationCache, ProtoChunk},
    };
    use pumpkin_data::{
        Block,
        block_properties::{BlockProperties, OakLeavesLikeProperties},
        dimension::Dimension,
    };
    use pumpkin_util::{math::position::BlockPos, world_seed::Seed};

    #[test]
    fn generated_leaves_receive_distance_from_connected_logs() {
        let generator = WorldGenerator::Flat(FlatGenerator::new(
            Seed(0),
            Dimension::OVERWORLD,
            Vec::new(),
            String::new(),
        ));
        let mut chunk = ProtoChunk::new(0, 0, &generator);
        let log = BlockPos::new(8, 64, 8);
        let near_leaf = BlockPos::new(9, 64, 8);
        let next_leaf = BlockPos::new(10, 64, 8);
        let disconnected_leaf = BlockPos::new(12, 64, 8);
        let leaves = OakLeavesLikeProperties {
            distance: 7,
            persistent: false,
            waterlogged: true,
        }
        .to_state_id(&Block::OAK_LEAVES)
        .to_state();

        GenerationCache::set_block_state(&mut chunk, &log.0, Block::OAK_LOG.default_state);
        for pos in [near_leaf, next_leaf, disconnected_leaf] {
            GenerationCache::set_block_state(&mut chunk, &pos.0, leaves);
        }

        TreeFeature::update_leaf_distances(
            &mut chunk,
            &[log],
            &[near_leaf, next_leaf, disconnected_leaf],
        );

        let properties_at = |pos: BlockPos| {
            OakLeavesLikeProperties::from_state_id(
                GenerationCache::get_block_state(&chunk, &pos.0),
                &Block::OAK_LEAVES,
            )
        };
        assert_eq!(properties_at(near_leaf).distance, 1);
        assert_eq!(properties_at(next_leaf).distance, 2);
        assert_eq!(properties_at(disconnected_leaf).distance, 7);
        assert!(properties_at(near_leaf).waterlogged);
    }
}
