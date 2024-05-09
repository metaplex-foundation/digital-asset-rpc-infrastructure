use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, asset_authority, asset_grouping};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
    AssetAuthority,
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
