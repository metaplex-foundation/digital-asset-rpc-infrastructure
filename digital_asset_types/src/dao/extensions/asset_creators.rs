use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, asset_creators};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => asset_creators::Entity::belongs_to(asset::Entity)
                .from(asset_creators::Column::AssetId)
                .to(asset::Column::Id)
                .into(),
        }
    }
}

impl Related<asset::Entity> for asset_creators::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}
