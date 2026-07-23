use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::random::xoroshiro128::Xoroshiro;
use pumpkin_util::random::{RandomImpl, get_seed};
use std::any::Any;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::{
    array::from_fn,
    sync::{Arc, atomic::AtomicBool},
};
use tokio::sync::Mutex;

use crate::block::viewer::{
    ViewerCountListener, ViewerCountTracker, ViewerCountTrackerExt, ViewerFuture,
};
use crate::world::World;
use pumpkin_world::inventory::InventoryFuture;
use pumpkin_world::inventory::{
    split_stack, {Clearable, Inventory},
};

use super::BlockEntity;

pub struct ShulkerBoxBlockEntity {
    pub position: BlockPos,
    pub items: [Arc<Mutex<ItemStack>>; Self::INVENTORY_SIZE],
    pub dirty: AtomicBool,

    // Viewer
    viewers: ViewerCountTracker,
}

impl BlockEntity for ShulkerBoxBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let shulker_box = Self {
            position,
            items: from_fn(|_| Arc::new(Mutex::new(ItemStack::EMPTY.clone()))),
            dirty: AtomicBool::new(false),
            viewers: ViewerCountTracker::new(),
        };

        shulker_box.read_data(nbt, &shulker_box.items);

        shulker_box
    }

    fn write_nbt<'a>(
        &'a self,
        nbt: &'a mut NbtCompound,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        self.write_inventory_nbt(nbt, true)
    }

    fn tick<'a>(&'a self, world: &'a Arc<World>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.viewers
                .update_viewer_count::<Self>(self, world, &self.position)
                .await;
        })
    }

    fn on_block_replaced<'a>(
        self: Arc<Self>,
        _world: Arc<World>,
        _position: BlockPos,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
    where
        Self: 'a,
    {
        Box::pin(async move {
            // Do nothing
        })
    }

    fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn Inventory>> {
        Some(self)
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ViewerCountListener for ShulkerBoxBlockEntity {
    fn on_container_open<'a>(
        &'a self,
        world: &'a Arc<World>,
        position: &'a BlockPos,
    ) -> ViewerFuture<'a, ()> {
        Box::pin(async move {
            Self::play_sound(world, position, Sound::BlockShulkerBoxOpen);
            // TODO: this.world.emitGameEvent(player, GameEvent.CONTAINER_OPEN, this.pos);
        })
    }

    fn on_container_close<'a>(
        &'a self,
        world: &'a Arc<World>,
        position: &'a BlockPos,
    ) -> ViewerFuture<'a, ()> {
        Box::pin(async move {
            Self::play_sound(world, position, Sound::BlockShulkerBoxClose);
            // TODO: this.world.emitGameEvent(player, GameEvent.CONTAINER_CLOSE, this.pos);
        })
    }

    fn on_viewer_count_update<'a>(
        &'a self,
        world: &'a Arc<World>,
        position: &'a BlockPos,
        _old: u16,
        new: u16,
    ) -> ViewerFuture<'a, ()> {
        Box::pin(async move {
            world
                .add_synced_block_event(*position, Self::OPEN_ANIMATION_EVENT_TYPE, new as u8)
                .await;
        })
    }
}

impl ShulkerBoxBlockEntity {
    pub const INVENTORY_SIZE: usize = 27;
    pub const OPEN_ANIMATION_EVENT_TYPE: u8 = 1;
    pub const ID: &'static str = "minecraft:shulker_box"; // TODO support multi IDs

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            items: from_fn(|_| Arc::new(Mutex::new(ItemStack::EMPTY.clone()))),
            dirty: AtomicBool::new(false),
            viewers: ViewerCountTracker::new(),
        }
    }

    fn play_sound(world: &Arc<World>, position: &BlockPos, sound: Sound) {
        let mut rng = Xoroshiro::from_seed(get_seed());

        world.play_sound_fine(
            sound,
            SoundCategory::Blocks,
            &position.to_centered_f64(),
            0.5,
            rng.next_f32() * 0.1 + 0.9,
        );
    }
}

impl Inventory for ShulkerBoxBlockEntity {
    fn size(&self) -> usize {
        self.items.len()
    }

    fn is_empty(&self) -> InventoryFuture<'_, bool> {
        Box::pin(async move {
            for slot in &self.items {
                if !slot.lock().await.is_empty() {
                    return false;
                }
            }

            true
        })
    }

    fn get_stack(&self, slot: usize) -> InventoryFuture<'_, Arc<Mutex<ItemStack>>> {
        Box::pin(async move { self.items[slot].clone() })
    }

    fn remove_stack(&self, slot: usize) -> InventoryFuture<'_, ItemStack> {
        Box::pin(async move {
            let mut removed = ItemStack::EMPTY.clone();
            let mut guard = self.items[slot].lock().await;
            std::mem::swap(&mut removed, &mut *guard);
            self.mark_dirty();
            removed
        })
    }

    fn remove_stack_specific(&self, slot: usize, amount: u8) -> InventoryFuture<'_, ItemStack> {
        Box::pin(async move {
            let res = split_stack(&self.items, slot, amount).await;
            self.mark_dirty();
            res
        })
    }

    fn set_stack(&self, slot: usize, stack: ItemStack) -> InventoryFuture<'_, ()> {
        Box::pin(async move {
            *self.items[slot].lock().await = stack;
            self.mark_dirty();
        })
    }

    fn on_open(&self) -> InventoryFuture<'_, ()> {
        Box::pin(async move {
            self.viewers.open_container();
        })
    }

    fn on_close(&self) -> InventoryFuture<'_, ()> {
        Box::pin(async move {
            self.viewers.close_container();
        })
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clearable for ShulkerBoxBlockEntity {
    fn clear(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            for slot in &self.items {
                *slot.lock().await = ItemStack::EMPTY.clone();
            }
            self.mark_dirty();
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ShulkerBoxBlockEntity;
    use crate::block::entities::BlockEntity;
    use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
    use pumpkin_util::math::position::BlockPos;

    fn make_nbt_items() -> NbtCompound {
        let items_tag = NbtTag::List(vec![NbtTag::Compound({
            let mut item = NbtCompound::new();
            item.put_string("id", "minecraft:dirt".to_string());
            item.put_int("count", 1);
            item.put_byte("Slot", 0i8);
            item
        })]);
        let mut block_entity_data = NbtCompound::new();
        block_entity_data.put("Items", items_tag);
        block_entity_data
    }

    #[test]
    fn items_survive_from_nbt_through_write_nbt() {
        let pos = BlockPos::new(0, 0, 0);
        let data = make_nbt_items();
        let dirt_id = pumpkin_data::item::Item::DIRT.id;

        // Exercise the public entry point that reads BlockEntity data into an
        // already-constructed ShulkerBoxBlockEntity.
        let entity = ShulkerBoxBlockEntity::from_nbt(&data, pos);
        assert_eq!(entity.items.len(), 27, "shulker box has 27 slots");

        let slot0 = entity.items[0].try_lock().expect("slot 0 not poisoned");
        assert_eq!(slot0.get_item().id, dirt_id);
        assert_eq!(slot0.item_count, 1);
        drop(slot0);

        // Write back to NBT and re-read. If write_nbt preserves items for
        // shulkers (which do NOT use the chest loot-table system), we should
        // get dirt back when we reconstruct via from_nbt.
        let mut written = NbtCompound::new();
        futures::executor::block_on(entity.write_nbt(&mut written));

        // Reconstruct from the written NBT
        let entity2 = ShulkerBoxBlockEntity::from_nbt(&written, pos);
        let slot0b = entity2.items[0].try_lock().expect("slot 0 not poisoned");
        assert_eq!(slot0b.get_item().id, dirt_id);
        assert_eq!(slot0b.item_count, 1);
    }
}
