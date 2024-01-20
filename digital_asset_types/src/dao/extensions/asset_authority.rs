use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, asset_authority};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => asset_authority::Entity::belongs_to(asset::Entity)
                .from(asset_authority::Column::AssetId)
                .to(asset::Column::Id)
                .into(),
        }
    }
}

impl Related<asset::Entity> for asset_authority::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}
