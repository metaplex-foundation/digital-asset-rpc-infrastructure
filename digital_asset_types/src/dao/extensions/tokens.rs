use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, token_accounts, tokens};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    TokenAccounts,
    Asset,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::TokenAccounts => tokens::Entity::belongs_to(token_accounts::Entity)
                .from(tokens::Column::Mint)
                .to(token_accounts::Column::Mint)
                .into(),
            Self::Asset => tokens::Entity::belongs_to(asset::Entity)
                .from(tokens::Column::Mint)
                .to(asset::Column::Id)
                .into(),
        }
    }
}

impl Related<token_accounts::Entity> for tokens::Entity {
    fn to() -> RelationDef {
        Relation::TokenAccounts.def()
    }
}

impl Related<asset::Entity> for tokens::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}
