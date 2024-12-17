use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, asset_authority, asset_data, asset_grouping};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
    AssetAuthority,
    AssetData,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => asset_grouping::Entity::belongs_to(asset::Entity)
                .from(asset_grouping::Column::AssetId)
                .to(asset::Column::Id)
                .into(),
            Self::AssetAuthority => asset_grouping::Entity::belongs_to(asset_authority::Entity)
                .from(asset_grouping::Column::AssetId)
                .to(asset_authority::Column::Id)
                .into(),
            Self::AssetData => asset_grouping::Entity::belongs_to(asset_data::Entity)
                .from(asset_grouping::Column::AssetId)
                .to(asset_data::Column::Id)
                .into(),
        }
    }
}

impl Related<asset::Entity> for asset_grouping::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}

impl Related<asset_authority::Entity> for asset_grouping::Entity {
    fn to() -> RelationDef {
        Relation::AssetAuthority.def()
    }
}

impl Related<asset_data::Entity> for asset_grouping::Entity {
    fn to() -> RelationDef {
        Relation::AssetData.def()
    }
}