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
mod m20230224_093722_performance_improvements;
mod m20230310_162227_add_indexes_to_bg;
mod m20230317_121944_remove_indexes_for_perf;
mod m20230510_183736_add_indices_to_assets;
mod m20230516_185005_add_reindex_to_assets;
mod m20230526_120101_add_owner_delegate_sequence_number;
mod m20230601_120101_add_pnft_enum_val;
mod m20230615_120101_remove_asset_null_constraints;
mod m20230620_120101_add_was_decompressed;
mod m20230623_120101_add_leaf_sequence_number;
mod m20230712_120101_remove_asset_creators_null_constraints;
mod m20230720_120101_add_asset_grouping_verified;
mod m20230720_130101_remove_asset_grouping_null_constraints;
mod m20230724_120101_add_group_info_seq;
mod m20230726_013107_remove_not_null_constraint_from_group_value;
mod m20230918_182123_add_raw_name_symbol;
mod m20230919_072154_cl_audits;
mod m20231019_120101_add_seq_numbers_bgum_update_metadata;
mod m20231206_120101_remove_was_decompressed;
mod m20240104_203133_add_cl_audits_v2;
mod m20240104_203328_remove_cl_audits;
mod m20240116_130744_add_update_metadata_ix;
mod m20240117_120101_alter_creator_indices;
mod m20240124_173104_add_tree_seq_index_to_cl_audits_v2;
mod m20240124_181900_add_slot_updated_column_per_update_type;
mod m20240219_115532_add_extensions_column;
mod m20240313_120101_add_mpl_core_plugins_columns;
mod m20240319_120101_add_mpl_core_enum_vals;
mod m20240320_120101_add_mpl_core_info_items;
mod m20240520_120101_add_mpl_core_external_plugins_columns;
mod m20240718_161232_change_supply_columns_to_numeric;
mod m20241119_060310_add_token_inscription_enum_variant;
mod m20250213_053451_add_idx_token_accounts_owner;
mod m20250313_095336_drop_idx_token_accounts_owner_and_idx_ta_mint;
mod m20250313_095449_add_idx_ta_owner_amount_and_idx_ta_mint_amount;
mod m20250321_120101_add_bgum_leaf_schema_v2_items;
mod m20250327_120101_add_bubblegum_v2_ixs_to_enum;
mod m20250422_102715_add_slot_metas_table;
mod m20250430_065207_idx_asset_grouping_verified_with_not_null_value;
mod m20250501_110559_add_indexes_to_asset_grouping;
mod m20250502_111210_add_indexes_to_token_accounts;
mod m20250526_123057_idx_asset_supply_burnt_owner_type;

pub mod model;

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
            Box::new(m20230224_093722_performance_improvements::Migration),
            Box::new(m20230310_162227_add_indexes_to_bg::Migration),
            Box::new(m20230317_121944_remove_indexes_for_perf::Migration),
            Box::new(m20230510_183736_add_indices_to_assets::Migration),
            Box::new(m20230516_185005_add_reindex_to_assets::Migration),
            Box::new(m20230526_120101_add_owner_delegate_sequence_number::Migration),
            Box::new(m20230601_120101_add_pnft_enum_val::Migration),
            Box::new(m20230615_120101_remove_asset_null_constraints::Migration),
            Box::new(m20230620_120101_add_was_decompressed::Migration),
            Box::new(m20230623_120101_add_leaf_sequence_number::Migration),
            Box::new(m20230712_120101_remove_asset_creators_null_constraints::Migration),
            Box::new(m20230720_120101_add_asset_grouping_verified::Migration),
            Box::new(m20230720_130101_remove_asset_grouping_null_constraints::Migration),
            Box::new(m20230724_120101_add_group_info_seq::Migration),
            Box::new(m20230726_013107_remove_not_null_constraint_from_group_value::Migration),
            Box::new(m20230918_182123_add_raw_name_symbol::Migration),
            Box::new(m20230919_072154_cl_audits::Migration),
            Box::new(m20231019_120101_add_seq_numbers_bgum_update_metadata::Migration),
            Box::new(m20231206_120101_remove_was_decompressed::Migration),
            Box::new(m20240104_203133_add_cl_audits_v2::Migration),
            Box::new(m20240104_203328_remove_cl_audits::Migration),
            Box::new(m20240116_130744_add_update_metadata_ix::Migration),
            Box::new(m20240117_120101_alter_creator_indices::Migration),
            Box::new(m20240124_173104_add_tree_seq_index_to_cl_audits_v2::Migration),
            Box::new(m20240124_181900_add_slot_updated_column_per_update_type::Migration),
            Box::new(m20240219_115532_add_extensions_column::Migration),
            Box::new(m20240313_120101_add_mpl_core_plugins_columns::Migration),
            Box::new(m20240319_120101_add_mpl_core_enum_vals::Migration),
            Box::new(m20240320_120101_add_mpl_core_info_items::Migration),
            Box::new(m20240520_120101_add_mpl_core_external_plugins_columns::Migration),
            Box::new(m20240718_161232_change_supply_columns_to_numeric::Migration),
            Box::new(m20241119_060310_add_token_inscription_enum_variant::Migration),
            Box::new(m20250213_053451_add_idx_token_accounts_owner::Migration),
            Box::new(m20250313_095336_drop_idx_token_accounts_owner_and_idx_ta_mint::Migration),
            Box::new(m20250313_095449_add_idx_ta_owner_amount_and_idx_ta_mint_amount::Migration),
            Box::new(m20250321_120101_add_bgum_leaf_schema_v2_items::Migration),
            Box::new(m20250327_120101_add_bubblegum_v2_ixs_to_enum::Migration),
            Box::new(m20250422_102715_add_slot_metas_table::Migration),
            Box::new(m20250430_065207_idx_asset_grouping_verified_with_not_null_value::Migration),
            Box::new(m20250501_110559_add_indexes_to_asset_grouping::Migration),
            Box::new(m20250502_111210_add_indexes_to_token_accounts::Migration),
            Box::new(m20250526_123057_idx_asset_supply_burnt_owner_type::Migration),
        ]
    }
}
