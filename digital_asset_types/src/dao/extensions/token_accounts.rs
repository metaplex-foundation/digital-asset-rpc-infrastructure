use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{asset, token_accounts, tokens};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Asset,
    Token,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => token_accounts::Entity::belongs_to(asset::Entity)
                .from(token_accounts::Column::Mint)
                .to(asset::Column::Id)
                .into(),
            Self::Token => token_accounts::Entity::belongs_to(tokens::Entity)
                .from(token_accounts::Column::Mint)
                .to(tokens::Column::Mint)
                .into(),
        }
    }
}

impl Related<asset::Entity> for token_accounts::Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}
impl Related<tokens::Entity> for token_accounts::Entity {
    fn to() -> RelationDef {
        Relation::Token.def()
    }
}
