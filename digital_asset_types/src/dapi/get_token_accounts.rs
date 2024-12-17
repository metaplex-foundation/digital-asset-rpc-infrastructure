use sea_orm::{DatabaseConnection, DbErr};

use crate::{
    dao::PageOptions,
    rpc::{options::Options, response::TokenAccountList},
};

use super::common::{build_token_list_response, create_pagination};

pub async fn get_token_accounts(
    db: &DatabaseConnection,
    owner_address: Option<Vec<u8>>,
    mint_address: Option<Vec<u8>>,
    page_options: &PageOptions,
    options: &Options,
) -> Result<TokenAccountList, DbErr> {
    let pagination = create_pagination(page_options)?;

    let token_accounts = crate::dao::scopes::asset::get_token_accounts(
        db,
        owner_address,
        mint_address,
        &pagination,
        page_options.limit,
    )
    .await?;

    Ok(build_token_list_response(
        token_accounts,
        page_options.limit,
        &pagination,
        options,
    ))
}
