use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{token_accounts, tokens};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    TokenAccounts,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::TokenAccounts => tokens::Entity::has_many(token_accounts::Entity).into(),
        }
    }
}

impl Related<token_accounts::Entity> for tokens::Entity {
    fn to() -> RelationDef {
        Relation::TokenAccounts.def()
    }
}
