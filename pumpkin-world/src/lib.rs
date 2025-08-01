use dimension::Dimension;
use generation::settings::GenerationSettings;
use pumpkin_util::math::vector2::Vector2;

pub mod biome;
pub mod block;
pub mod chunk;
pub mod cylindrical_chunk_iterator;
pub mod data;
pub mod dimension;
pub mod entity;
pub mod generation;
pub mod inventory;
pub mod item;
pub mod level;
pub mod lock;
pub mod tick;
pub mod world;
pub mod world_info;

pub type BlockId = u16;
pub type BlockStateId = u16;

#[macro_export]
macro_rules! global_path {
    ($path:expr) => {{
        use std::path::Path;
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(file!())
            .parent()
            .unwrap()
            .join($path)
    }};
}

// TODO: is there a way to do in-file benches?
pub use generation::{
    GlobalRandomConfig, noise_router::proto_noise_router::ProtoNoiseRouters,
    proto_chunk::ProtoChunk, settings::GENERATION_SETTINGS, settings::GeneratorSetting,
};
pub fn bench_create_and_populate_noise(
    base_router: &ProtoNoiseRouters,
    random_config: &GlobalRandomConfig,
    settings: &GenerationSettings,
) {
    let mut chunk = ProtoChunk::new(Vector2::new(0, 0), base_router, random_config, settings);
    chunk.populate_noise();
}

pub fn bench_create_and_populate_biome(
    base_router: &ProtoNoiseRouters,
    random_config: &GlobalRandomConfig,
    settings: &GenerationSettings,
) {
    let mut chunk = ProtoChunk::new(Vector2::new(0, 0), base_router, random_config, settings);
    chunk.populate_biomes(Dimension::Overworld);
}

pub fn bench_create_and_populate_noise_with_surface(
    base_router: &ProtoNoiseRouters,
    random_config: &GlobalRandomConfig,
    settings: &GenerationSettings,
) {
    let mut chunk = ProtoChunk::new(Vector2::new(0, 0), base_router, random_config, settings);
    chunk.populate_biomes(Dimension::Overworld);
    chunk.populate_noise();
    chunk.build_surface();
}
