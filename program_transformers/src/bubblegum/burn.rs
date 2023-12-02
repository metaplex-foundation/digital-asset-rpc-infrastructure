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
    cl_audits: bool,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(cl) = &parsing_result.tree_update {
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await?;
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

        // Upsert asset table `burnt` column.
        let query = asset::Entity::insert(asset_model)
            .on_conflict(
                OnConflict::columns([asset::Column::Id])
                    .update_columns([
                        asset::Column::Burnt,
                        //TODO maybe handle slot updated.
                    ])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        txn.execute(query).await?;

        upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

        return Ok(());
    }
    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}