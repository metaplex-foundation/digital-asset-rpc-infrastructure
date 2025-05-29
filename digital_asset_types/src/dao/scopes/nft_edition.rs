use crate::{
    dao::{
        asset_v1_account_attachments::{self, Column},
        sea_orm_active_enums::V1AccountAttachments,
        Cursor, Pagination,
    },
    rpc::response::{NftEdition, NftEditions},
};
use mpl_token_metadata::accounts::{Edition, MasterEdition};
use sea_orm::{
    sea_query::Expr, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder, QuerySelect, Select,
};
use serde::de::DeserializeOwned;
use solana_sdk::pubkey::Pubkey;

pub trait NftEditionSelectExt {
    fn sort_by(self, column: Column, direction: &Order) -> Self;

    fn page_by(
        self,
        pagination: &Pagination,
        limit: u64,
        sort_direction: &Order,
        col: Column,
    ) -> Self;
}

impl NftEditionSelectExt for Select<asset_v1_account_attachments::Entity> {
    fn sort_by(self, col: Column, direction: &Order) -> Self {
        match col {
            Column::Id => self.order_by(col, direction.clone()).to_owned(),
            _ => self
                .order_by(col, direction.clone())
                .order_by(Column::Id, Order::Desc)
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

fn get_edition_data_from_json<T: DeserializeOwned>(data: serde_json::Value) -> Result<T, DbErr> {
    serde_json::from_value(data).map_err(|e| DbErr::Custom(e.to_string()))
}

fn attachment_to_nft_edition(
    attachment: asset_v1_account_attachments::Model,
) -> Result<NftEdition, DbErr> {
    let data: Edition = attachment
        .data
        .clone()
        .ok_or(DbErr::RecordNotFound("Edition data not found".to_string()))
        .map(get_edition_data_from_json)??;

    Ok(NftEdition {
        mint_address: attachment
            .asset_id
            .clone()
            .map(|id| bs58::encode(id).into_string())
            .unwrap_or("".to_string()),
        edition_number: data.edition,
        edition_address: bs58::encode(attachment.id.clone()).into_string(),
    })
}

pub async fn get_nft_editions(
    conn: &impl ConnectionTrait,
    mint_address: Pubkey,
    pagination: &Pagination,
    limit: u64,
) -> Result<NftEditions, DbErr> {
    let master_edition_pubkey = MasterEdition::find_pda(&mint_address).0;

    // to fetch nft editions associated with a mint we need to fetch the master edition first
    let master_edition =
        asset_v1_account_attachments::Entity::find_by_id(master_edition_pubkey.to_bytes().to_vec())
            .one(conn)
            .await?
            .ok_or(DbErr::RecordNotFound(
                "Master Edition not found".to_string(),
            ))?;

    let master_edition_data: MasterEdition = master_edition
        .data
        .clone()
        .ok_or(DbErr::RecordNotFound(
            "Master Edition data not found".to_string(),
        ))
        .map(get_edition_data_from_json)??;

    let mut stmt = asset_v1_account_attachments::Entity::find();

    stmt = stmt.filter(
        asset_v1_account_attachments::Column::AttachmentType
            .eq(V1AccountAttachments::Edition)
            // The data field is a JSON field that contains the edition data.
            .and(asset_v1_account_attachments::Column::Data.is_not_null())
            // The parent field is a string field that contains the master edition pubkey ( mapping edition to master edition )
            .and(Expr::cust(&format!(
                "data->>'parent' = '{}'",
                master_edition_pubkey
            ))),
    );

    stmt = stmt.sort_by(Column::Id, &Order::Desc);
    stmt = stmt.page_by(pagination, limit, &Order::Desc, Column::Id);

    let nft_editions = stmt
        .all(conn)
        .await?
        .into_iter()
        .map(attachment_to_nft_edition)
        .collect::<Result<Vec<NftEdition>, _>>()?;

    let (page, before, after, cursor) = match pagination {
        Pagination::Keyset { before, after } => {
            let bef = before.clone().and_then(|x| String::from_utf8(x).ok());
            let aft = after.clone().and_then(|x| String::from_utf8(x).ok());
            (None, bef, aft, None)
        }
        Pagination::Page { page } => (Some(*page as u32), None, None, None),
        Pagination::Cursor(_) => {
            if let Some(last_asset) = nft_editions.last() {
                let cursor_str = last_asset.edition_address.clone();
                (None, None, None, Some(cursor_str))
            } else {
                (None, None, None, None)
            }
        }
    };

    Ok(NftEditions {
        total: nft_editions.len() as u32,
        master_edition_address: master_edition_pubkey.to_string(),
        supply: master_edition_data.supply,
        max_supply: master_edition_data.max_supply,
        editions: nft_editions,
        limit: limit as u32,
        page,
        before,
        after,
        cursor,
    })
}
