use crate::IngesterError;
use blockbuster::programs::bubblegum::ChangeLogEvent;
use digital_asset_types::dao::{
    backfill_items, candy_guard_allow_list, candy_guard_bot_tax, candy_guard_lamports,
    candy_guard_live_date, candy_guard_mint_limit, candy_guard_nft_payment, candy_guard_spl_token,
    candy_guard_third_party_signer, cl_items,
};
use mpl_candy_guard::guards::{
    AllowList, BotTax, GuardSet, Lamports, MintLimit, NftPayment, SplToken, ThirdPartySigner,
};
use sea_orm::{entity::*, query::*, sea_query::OnConflict, DatabaseTransaction, DbBackend};

// TODO: clarify if needing to call to db to see if exists first,
// then add or update accordingly

pub async fn process_nft_payment_change(
    nft_payment: &NftPayment,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_nft_payment = candy_guard_nft_payment::ActiveModel {
        burn: todo!(),
        required_collection: todo!(),
        candy_guard_id: todo!(),
        ..Default::default()
    };

    let query = candy_guard_nft_payment::Entity::insert_one(candy_guard_nft_payment)
        .on_conflict(
            OnConflict::columns([candy_guard_nft_payment::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_mint_limit_change(
    mint_limit: &MintLimit,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_mint_limit = candy_guard_mint_limit::ActiveModel {
        limit: todo!(),
        candy_guard_id: todo!(),
        ..Default::default()
    };

    let query = candy_guard_mint_limit::Entity::insert_one(candy_guard_mint_limit)
        .on_conflict(
            OnConflict::columns([candy_guard_mint_limit::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_allow_list_change(
    allow_list: &AllowList,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_allow_list = candy_guard_allow_list::ActiveModel {
        merkle_root: todo!(),
        candy_guard_id: todo!(),
        ..Default::default()
    };

    let query = candy_guard_allow_list::Entity::insert_one(candy_guard_allow_list)
        .on_conflict(
            OnConflict::columns([candy_guard_allow_list::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_third_party_signer_change(
    third_party_signer: &ThirdPartySigner,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_third_party_signer = candy_guard_third_party_signer::ActiveModel {};

    let query = candy_guard_third_party_signer::Entity::insert_one(candy_guard_third_party_signer)
        .on_conflict(
            OnConflict::columns([candy_guard_third_party_signer::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_live_date_change(
    live_date: &LiveDate,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_live_date = candy_guard_live_date::ActiveModel {};

    let query = candy_guard_live_date::Entity::insert_one(candy_guard_live_date)
        .on_conflict(
            OnConflict::columns([candy_guard_live_date::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_spl_token_change(
    spl_token: &SplToken,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_spl_token = candy_guard_spl_token::ActiveModel {};

    let query = candy_guard_spl_token::Entity::insert_one(candy_guard_spl_token)
        .on_conflict(
            OnConflict::columns([candy_guard_spl_token::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_lamports_change(
    lamports: &Lamports,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_lamports = candy_guard_lamports::ActiveModel {};

    let query = candy_guard_lamports::Entity::insert_one(candy_guard_lamports)
        .on_conflict(
            OnConflict::columns([candy_guard_lamports::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_bot_tax_change(
    bot_tax: &BotTax,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard_bot_tax = candy_guard_bot_tax::ActiveModel {};

    let query = candy_guard_bot_tax::Entity::insert_one(candy_guard_bot_tax)
        .on_conflict(
            OnConflict::columns([candy_guard_bot_tax::Column::CandyMachineDataId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

// TODO: put appropriate field for each model, currently copied all from end settings

pub async fn process_guard_set_change(
    guard_set: &GuardSet,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    if let Some(whitelist) = guard_set.whitelist {
        process_whitelist_change(whitelist, 7, txn)?;
    }

    if let Some(gatekeeper) = guard_set.gatekeeper {
        process_gatekeeper_change(gatekeeper, 7, txn)?;
    }

    if let Some(end_settings) = guard_set.end_settings {
        process_end_settings_change(end_settings, 7, txn)?;
    }

    if let Some(bot_tax) = guard_set.bot_tax {
        process_bot_tax_change(&bot_tax, txn)?;
    }

    if let Some(lamports) = guard_set.lamports {
        process_lamports_change(&lamports, txn)?;
    }

    if let Some(spl_token) = guard_set.spl_token {
        process_spl_token_change(&spl_token, txn)?;
    }

    if let Some(live_date) = guard_set.live_date {
        process_live_date_change(&live_date, txn)?;
    }

    if let Some(third_party_signer) = guard_set.third_party_signer {
        process_third_party_signer_change(&third_party_signer, txn)?;
    }

    if let Some(allow_list) = guard_set.allow_list {
        process_allow_list_change(&allow_list, txn)?;
    }

    if let Some(mint_limit) = guard_set.mint_limit {
        process_mint_limit_change(&mint_limit, txn)?;
    }

    if let Some(nft_payment) = guard_set.nft_payment {
        process_nft_payment_change(&nft_payment, txn)?;
    }

    Ok(())
}
