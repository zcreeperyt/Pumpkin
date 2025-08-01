use crate::block::{BlockBehaviour, OnPlaceArgs};
use async_trait::async_trait;
use pumpkin_data::block_properties::{BlockProperties, FurnaceLikeProperties};
use pumpkin_macros::pumpkin_block;
use pumpkin_world::BlockStateId;

#[pumpkin_block("minecraft:blast_furnace")]
pub struct BlastFurnaceBlock;

#[async_trait]
impl BlockBehaviour for BlastFurnaceBlock {
    async fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = FurnaceLikeProperties::default(args.block);
        props.facing = args
            .player
            .living_entity
            .entity
            .get_horizontal_facing()
            .opposite();
        props.to_state_id(args.block)
    }
}
