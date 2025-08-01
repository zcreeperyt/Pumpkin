use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};

use async_trait::async_trait;
use pumpkin_util::math::position::BlockPos;

use crate::{block::entities::BlockEntity, inventory::Inventory, world::SimpleWorld};

#[derive(Debug)]
pub struct ViewerCountTracker {
    old: AtomicU16,
    current: AtomicU16,
}

impl Default for ViewerCountTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewerCountTracker {
    pub fn new() -> Self {
        Self {
            old: AtomicU16::new(0),
            current: AtomicU16::new(0),
        }
    }

    pub fn open_container(&self) {
        self.current.fetch_add(1, Ordering::Relaxed);
    }

    pub fn close_container(&self) {
        self.current.fetch_sub(1, Ordering::Relaxed);
    }

    pub async fn update_viewer_count<T>(
        &self,
        entity: &T,
        world: Arc<dyn SimpleWorld>,
        position: &BlockPos,
    ) where
        T: BlockEntity + Inventory + ViewerCountListener + 'static,
    {
        let current = self.current.load(Ordering::Relaxed);
        let old = self.old.swap(current, Ordering::Relaxed);
        if old != current {
            match (old, current) {
                (n, 0) if n > 0 => {
                    entity.on_container_close(&world, position).await;
                    // TODO: world.emitGameEvent(player, GameEvent.CONTAINER_CLOSE, pos);
                    // TODO: this.maxBlockInteractionRange = 0.0;
                }
                (0, n) if n > 0 => {
                    entity.on_container_open(&world, position).await;
                    // TODO: world.emitGameEvent(player, GameEvent.CONTAINER_OPEN, pos);
                    // TODO: scheduleBlockTick(world, pos, state);
                }
                _ => {} // Ignore
            }

            entity
                .on_viewer_count_update(&world, position, old, current)
                .await;
        }

        // TODO: Requires players
    }
}

#[async_trait]
pub trait ViewerCountListener {
    async fn on_container_open(&self, _world: &Arc<dyn SimpleWorld>, _position: &BlockPos) {}
    async fn on_container_close(&self, _world: &Arc<dyn SimpleWorld>, _position: &BlockPos) {}
    async fn on_viewer_count_update(
        &self,
        _world: &Arc<dyn SimpleWorld>,
        _position: &BlockPos,
        _old: u16,
        _new: u16,
    ) {
    }
}
