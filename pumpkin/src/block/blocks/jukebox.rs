use std::sync::Arc;

use crate::block::registry::BlockActionResult;
use crate::block::{BlockBehaviour, BrokenArgs, UseWithItemArgs};
use crate::world::World;
use async_trait::async_trait;
use pumpkin_data::data_component_impl::JukeboxPlayableImpl;
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{
    Block,
    block_properties::{BlockProperties, JukeboxLikeProperties},
};
use pumpkin_macros::pumpkin_block;
use pumpkin_registry::SYNCED_REGISTRIES;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

#[pumpkin_block("minecraft:jukebox")]
pub struct JukeboxBlock;

impl JukeboxBlock {
    async fn has_record(&self, block: &Block, location: &BlockPos, world: &World) -> bool {
        let state_id = world.get_block_state(location).await.id;
        JukeboxLikeProperties::from_state_id(state_id, block).has_record
    }

    async fn set_record(
        &self,
        has_record: bool,
        block: &Block,
        location: &BlockPos,
        world: &Arc<World>,
    ) {
        let new_state = JukeboxLikeProperties { has_record };
        world
            .set_block_state(location, new_state.to_state_id(block), BlockFlags::empty())
            .await;
    }

    async fn stop_music(&self, block: &Block, position: &BlockPos, world: &Arc<World>) {
        self.set_record(false, block, position, world).await;
        world
            .sync_world_event(WorldEvent::JukeboxStopsPlaying, *position, 0)
            .await;
    }
}

#[async_trait]
impl BlockBehaviour for JukeboxBlock {
    async fn use_with_item(&self, args: UseWithItemArgs<'_>) -> BlockActionResult {
        let world = &args.player.living_entity.entity.world.read().await;

        // if the jukebox already has a record, stop playing
        if self.has_record(args.block, args.position, world).await {
            self.stop_music(args.block, args.position, world).await;
            return BlockActionResult::Success;
        }

        let jukebox_playable = args
            .item_stack
            .lock()
            .await
            .get_data_component::<JukeboxPlayableImpl>()
            .map(|i| i.song);

        let Some(jukebox_playable) = jukebox_playable else {
            return BlockActionResult::Pass;
        };

        let Some(song) = jukebox_playable.split(':').nth(1) else {
            return BlockActionResult::Pass;
        };

        let Some(jukebox_song) = SYNCED_REGISTRIES.jukebox_song.get_index_of(song) else {
            log::error!("Jukebox playable song not registered!");
            return BlockActionResult::Pass;
        };

        //TODO: Update block nbt

        self.set_record(true, args.block, args.position, world)
            .await;
        world
            .sync_world_event(
                WorldEvent::JukeboxStartsPlaying,
                *args.position,
                jukebox_song as i32,
            )
            .await;

        BlockActionResult::Success
    }

    async fn broken(&self, args: BrokenArgs<'_>) {
        // For now just stop the music at this position
        args.world
            .sync_world_event(WorldEvent::JukeboxStopsPlaying, *args.position, 0)
            .await;
    }
}
