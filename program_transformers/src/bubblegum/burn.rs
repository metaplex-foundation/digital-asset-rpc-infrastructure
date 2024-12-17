use {
    crate::{
        bubblegum::{
            db::{save_changelog_event, upsert_asset_with_seq},
            u32_to_u8_array,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
    },
    blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction},
    digital_asset_types::dao::asset,
    sea_orm::{
        entity::{ActiveValue, EntityTrait},
        query::QueryTrait,
        sea_query::query::OnConflict,
        ConnectionTrait, DbBackend, TransactionTrait,
    },
    solana_sdk::pubkey::Pubkey,
    tracing::debug,
};

pub async fn burn<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(cl) = &parsing_result.tree_update {
        // Begin a transaction.  If the transaction goes out of scope (i.e. one of the executions has
        // an error and this function returns it using the `?` operator), then the transaction is
        // automatically rolled back.
        let multi_txn = txn.begin().await?;

        let seq =
            save_changelog_event(cl, bundle.slot, bundle.txn_id, &multi_txn, instruction).await?;
        let leaf_index = cl.index;
        let (asset_id, _) = Pubkey::find_program_address(
            &[
                "asset".as_bytes(),
                cl.id.as_ref(),
                u32_to_u8_array(leaf_index).as_ref(),
            ],
            &mpl_bubblegum::ID,
        );
        debug!("Indexing burn for asset id: {:?}", asset_id);
        let id_bytes = asset_id.to_bytes();

        let asset_model = asset::ActiveModel {
            id: ActiveValue::Set(id_bytes.to_vec()),
            burnt: ActiveValue::Set(true),
            ..Default::default()
        };

        // Upsert asset table `burnt` column.  Note we don't check for decompression (asset.seq = 0)
        // because we know if the item was burnt it could not have been decompressed later.
        let query = asset::Entity::insert(asset_model)
            .on_conflict(
                OnConflict::columns([asset::Column::Id])
                    .update_columns([asset::Column::Burnt])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        multi_txn.execute(query).await?;

        upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

        multi_txn.commit().await?;

        return Ok(());
    }
    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
