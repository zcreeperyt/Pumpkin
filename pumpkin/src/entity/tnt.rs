use super::{Entity, EntityBase, living::LivingEntity};
use crate::server::Server;
use async_trait::async_trait;
use core::f32;
use pumpkin_data::{Block, damage::DamageType};
use pumpkin_protocol::{
    codec::var_int::VarInt,
    java::client::play::{MetaDataType, Metadata},
};
use pumpkin_util::math::vector3::Vector3;
use std::{
    f64::consts::TAU,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering::Relaxed},
    },
};

pub struct TNTEntity {
    entity: Entity,
    power: f32,
    fuse: AtomicU32,
}

impl TNTEntity {
    pub fn new(entity: Entity, power: f32, fuse: u32) -> Self {
        Self {
            entity,
            power,
            fuse: AtomicU32::new(fuse),
        }
    }
}

#[async_trait]
impl EntityBase for TNTEntity {
    async fn tick(&self, _caller: Arc<dyn EntityBase>, server: &Server) {
        let fuse = self.fuse.fetch_sub(1, Relaxed);
        if fuse == 0 {
            self.entity.remove().await;
            self.entity
                .world
                .read()
                .await
                .explode(server, self.entity.pos.load(), self.power)
                .await;
        }
    }

    async fn init_data_tracker(&self) {
        // TODO: Yes, this is the wrong function, but we need to send this after spawning the entity.
        let pos: f64 = rand::random::<f64>() * TAU;

        self.entity
            .set_velocity(Vector3::new(-pos.sin() * 0.02, 0.2, -pos.cos() * 0.02))
            .await;
        // We can merge multiple `Metadata`s into one meta packet.
        self.entity
            .send_meta_data(&[
                Metadata::new(
                    8,
                    MetaDataType::Integer,
                    VarInt(self.fuse.load(Relaxed) as i32),
                ),
                Metadata::new(
                    9,
                    MetaDataType::BlockState,
                    VarInt(i32::from(Block::TNT.default_state.id)),
                ),
            ])
            .await;
    }

    async fn damage_with_context(
        &self,
        _amount: f32,
        _damage_type: DamageType,
        _position: Option<Vector3<f64>>,
        _source: Option<&dyn EntityBase>,
        _cause: Option<&dyn EntityBase>,
    ) -> bool {
        false
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }
}
