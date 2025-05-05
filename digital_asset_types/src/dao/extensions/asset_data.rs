use crate::dao::sea_orm_active_enums::{ChainMutability, Mutability};
use crate::dao::{asset, asset_data};
use sea_orm::prelude::Json;
use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => asset_data::Entity::has_many(asset::Entity).into(),
        }
    }
}

impl Related<asset::Entity> for asset_data::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}

impl Default for ChainMutability {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Default for Mutability {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Default for asset_data::Model {
    fn default() -> Self {
        Self {
            id: vec![],
            chain_data_mutability: ChainMutability::default(),
            chain_data: Json::default(),
            metadata_url: String::default(),
            metadata_mutability: Mutability::default(),
            metadata: Json::default(),
            slot_updated: 0,
            reindex: None,
            raw_name: None,
            raw_symbol: None,
            base_info_seq: None,
        }
    }
}
