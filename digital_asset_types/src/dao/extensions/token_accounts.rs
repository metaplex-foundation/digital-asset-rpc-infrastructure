use sea_orm::{EntityTrait, EnumIter, Related, RelationDef, RelationTrait};

use crate::dao::{token_accounts, tokens};

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Tokens,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Tokens => token_accounts::Entity::belongs_to(tokens::Entity)
                .from(token_accounts::Column::Mint)
                .to(tokens::Column::Mint)
                .into(),
        }
    }
}

impl Related<tokens::Entity> for token_accounts::Entity {
    fn to() -> RelationDef {
        Relation::Tokens.def()
    }
}
