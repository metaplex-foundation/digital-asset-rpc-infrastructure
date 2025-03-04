use std::str::FromStr;

use crate::AccountInfo;
use blockbuster::programs::token_inscriptions::TokenInscriptionAccount;
use digital_asset_types::dao::asset_v1_account_attachments;
use digital_asset_types::dao::sea_orm_active_enums::V1AccountAttachments;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QueryTrait,
};
use solana_sdk::pubkey::Pubkey;

use crate::error::{ProgramTransformerError, ProgramTransformerResult};

pub async fn handle_token_inscription_program_update(
    account_info: &AccountInfo,
    parsing_result: &TokenInscriptionAccount,
    db: &DatabaseConnection,
) -> ProgramTransformerResult<()> {
    let account_key = account_info.pubkey.to_bytes().to_vec();

    let TokenInscriptionAccount { data } = parsing_result;

    let ser = serde_json::to_value(data)
        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?;

    let asset_id = Pubkey::from_str(&data.root)
        .map_err(|e| ProgramTransformerError::ParsingError(e.to_string()))?
        .to_bytes()
        .to_vec();

    let model = asset_v1_account_attachments::ActiveModel {
        id: ActiveValue::Set(account_key),
        asset_id: ActiveValue::Set(Some(asset_id)),
        data: ActiveValue::Set(Some(ser)),
        slot_updated: ActiveValue::Set(account_info.slot as i64),
        initialized: ActiveValue::Set(true),
        attachment_type: ActiveValue::Set(V1AccountAttachments::TokenInscription),
    };

    let mut query = asset_v1_account_attachments::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_v1_account_attachments::Column::Id])
                .update_columns([
                    asset_v1_account_attachments::Column::Data,
                    asset_v1_account_attachments::Column::SlotUpdated,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
        "{} WHERE excluded.slot_updated > asset_v1_account_attachments.slot_updated",
        query.sql
    );
    db.execute(query).await?;

    Ok(())
}
