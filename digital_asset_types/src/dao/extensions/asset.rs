use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{
    asset, asset_authority, asset_creators, asset_data, asset_grouping,
    asset_v1_account_attachments,
    sea_orm_active_enums::{OwnerType, RoyaltyTargetType},
    token_accounts,
};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    AssetData,
    AssetV1AccountAttachments,
    AssetAuthority,
    AssetCreators,
    AssetGrouping,
    TokenAccounts,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::AssetData => asset::Entity::belongs_to(asset_data::Entity)
                .from(asset::Column::AssetData)
                .to(asset_data::Column::Id)
                .into(),
            Self::TokenAccounts => asset::Entity::has_many(token_accounts::Entity)
                .from(asset::Column::Id)
                .to(token_accounts::Column::Mint)
                .into(),
            Self::AssetV1AccountAttachments => {
                asset::Entity::has_many(asset_v1_account_attachments::Entity).into()
            }
            Self::AssetAuthority => asset::Entity::has_many(asset_authority::Entity).into(),
            Self::AssetCreators => asset::Entity::has_many(asset_creators::Entity).into(),
            Self::AssetGrouping => asset::Entity::has_many(asset_grouping::Entity).into(),
        }
    }
}

impl Related<asset_data::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetData.def()
    }
}

impl Related<asset_v1_account_attachments::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetV1AccountAttachments.def()
    }
}

impl Related<asset_authority::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetAuthority.def()
    }
}

impl Related<asset_creators::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetCreators.def()
    }
}

impl Related<asset_grouping::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetGrouping.def()
    }
}

impl Related<token_accounts::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::TokenAccounts.def()
    }
}

impl Default for RoyaltyTargetType {
    fn default() -> Self {
        Self::Creators
    }
}

impl Default for asset::Model {
    fn default() -> Self {
        Self {
            id: vec![],
            alt_id: None,
            specification_version: None,
            specification_asset_class: None,
            owner: None,
            owner_type: OwnerType::Unknown,
            delegate: None,
            frozen: Default::default(),
            supply: Default::default(),
            supply_mint: None,
            compressed: Default::default(),
            compressible: Default::default(),
            seq: None,
            tree_id: None,
            leaf: None,
            nonce: None,
            royalty_target_type: RoyaltyTargetType::Unknown,
            royalty_target: None,
            royalty_amount: Default::default(),
            asset_data: None,
            created_at: None,
            burnt: Default::default(),
            slot_updated: None,
            slot_updated_metadata_account: None,
            slot_updated_mint_account: None,
            slot_updated_token_account: None,
            slot_updated_cnft_transaction: None,
            data_hash: None,
            creator_hash: None,
            owner_delegate_seq: None,
            leaf_seq: None,
            base_info_seq: None,
            mint_extensions: None,
            mpl_core_plugins: None,
            mpl_core_unknown_plugins: None,
            mpl_core_collection_current_size: None,
            mpl_core_collection_num_minted: None,
            mpl_core_plugins_json_version: None,
            mpl_core_external_plugins: None,
            mpl_core_unknown_external_plugins: None,
            collection_hash: None,
            asset_data_hash: None,
            bubblegum_flags: None,
            non_transferable: None,
        }
    }
}
