use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, asset_v1_account_attachments};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => asset_v1_account_attachments::Entity::belongs_to(asset::Entity)
                .from(asset_v1_account_attachments::Column::AssetId)
                .to(asset::Column::Id)
                .into(),
        }
    }
}

impl Related<asset::Entity> for asset_v1_account_attachments::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}
