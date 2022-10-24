pub use sea_orm_migration::prelude::*;

mod m20220101_000001_init;
mod m20221020_052135_add_asset_hashes;
mod m20221022_140350_add_creator_asset_unique_index;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_init::Migration),
            Box::new(m20221020_052135_add_asset_hashes::Migration),
            Box::new(m20221022_140350_add_creator_asset_unique_index::Migration),
        ]
    }
}
