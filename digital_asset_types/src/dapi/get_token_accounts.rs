use sea_orm::{DatabaseConnection, DbErr};

use crate::{
    dao::PageOptions,
    rpc::{options::Options, response::TokenAccountList},
};

use super::common::build_token_list_response;

#[tracing::instrument(
    name = "db::getTokenAccounts",
    skip_all,
    fields(
        owner = ?owner_address.as_ref().map(|o| bs58::encode(o).into_string()),
        mint  = ?mint_address.as_ref().map(|m| bs58::encode(m).into_string())
    )
)]
pub async fn get_token_accounts(
    db: &DatabaseConnection,
    owner_address: Option<Vec<u8>>,
    mint_address: Option<Vec<u8>>,
    page_options: &PageOptions,
    options: &Options,
) -> Result<TokenAccountList, DbErr> {
    let pagination = page_options.try_into()?;
    let token_accounts = crate::dao::scopes::token::get_token_accounts(
        db,
        owner_address,
        mint_address,
        &pagination,
        page_options.limit,
        options,
    )
    .await?;
    Ok(build_token_list_response(
        token_accounts,
        page_options.limit,
        &pagination,
    ))
}
