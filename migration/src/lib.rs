pub use sea_orm_migration::prelude::*;

mod m20220101_000001_init;
mod m20221020_052135_add_asset_hashes;
mod m20221022_140350_add_creator_asset_unique_index;
mod m20221025_182127_remove_creator_error_unique_index;
mod m20221026_155220_add_bg_tasks;
mod m20221104_094327_add_backfiller_failed;
mod m20221114_173041_add_collection_info;
mod m20221115_165700_add_backfiller_locked;
mod m20221116_110500_add_backfiller_failed_and_locked_indeces;
mod m20230105_160722_drop_collection_info;
mod m20230106_051135_unique_groupings;
mod m20230131_140613_change_token_account_indexes;
mod m20230203_205959_improve_upsert_perf;
// mod m20230206_093340_raw_update_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_init::Migration),
            Box::new(m20221020_052135_add_asset_hashes::Migration),
            Box::new(m20221022_140350_add_creator_asset_unique_index::Migration),
            Box::new(m20221025_182127_remove_creator_error_unique_index::Migration),
            Box::new(m20221026_155220_add_bg_tasks::Migration),
            Box::new(m20221104_094327_add_backfiller_failed::Migration),
            Box::new(m20221114_173041_add_collection_info::Migration),
            Box::new(m20221115_165700_add_backfiller_locked::Migration),
            Box::new(m20221116_110500_add_backfiller_failed_and_locked_indeces::Migration),
            Box::new(m20230105_160722_drop_collection_info::Migration),
            Box::new(m20230106_051135_unique_groupings::Migration),
            Box::new(m20230131_140613_change_token_account_indexes::Migration),
            Box::new(m20230203_205959_improve_upsert_perf::Migration),
            // Box::new(m20230206_093340_raw_update_table::Migration),
        ]
    }
}
