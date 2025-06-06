use sea_orm::{
    ColumnTrait, EntityTrait, EnumIter, Order, QueryFilter, QueryOrder, QuerySelect, Related,
    RelationDef, RelationTrait, Select,
};

use crate::dao::{
    asset,
    token_accounts::{self, Column},
    tokens, Cursor, Pagination,
};

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

pub trait TokenAccountSelectExt {
    fn sort_by(self, column: Column, direction: &Order) -> Self;

    fn page_by(
        self,
        pagination: &Pagination,
        limit: u64,
        sort_direction: &Order,
        col: Column,
    ) -> Self;
}

impl TokenAccountSelectExt for Select<token_accounts::Entity> {
    fn sort_by(self, col: Column, direction: &Order) -> Self {
        match col {
            Column::Pubkey => self.order_by(col, direction.clone()).to_owned(),
            _ => self
                .order_by(col, direction.clone())
                .order_by(Column::Pubkey, Order::Asc)
                .to_owned(),
        }
    }

    fn page_by(
        mut self,
        pagination: &Pagination,
        limit: u64,
        order: &Order,
        column: Column,
    ) -> Self {
        match pagination {
            Pagination::Keyset { before, after } => {
                if let Some(b) = before {
                    self = self.filter(column.lt(b.clone())).to_owned();
                }
                if let Some(a) = after {
                    self = self.filter(column.gt(a.clone())).to_owned();
                }
            }
            Pagination::Page { page } => {
                if *page > 0 {
                    self = self.offset((page - 1) * limit).to_owned();
                }
            }
            Pagination::Cursor(cursor) => {
                if *cursor != Cursor::default() {
                    if order == &Order::Asc {
                        self = self.filter(column.gt(cursor.id.clone())).to_owned();
                    } else {
                        self = self.filter(column.lt(cursor.id.clone())).to_owned();
                    }
                }
            }
        }
        self.limit(limit).to_owned()
    }
}
