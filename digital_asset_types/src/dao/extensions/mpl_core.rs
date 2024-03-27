use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, mpl_core};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => mpl_core::Entity::belongs_to(asset::Entity)
                .from(mpl_core::Column::AssetId)
                .to(asset::Column::Id)
                .into(),
        }
    }
}

impl Related<asset::Entity> for mpl_core::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}
