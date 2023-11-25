use crate::{error::IngesterError, program_transformers::account_compression::insert_change_log};
use blockbuster::{
    instruction::InstructionBundle, programs::account_compression::AccountCompressionInstruction,
};
use sea_orm::{ConnectionTrait, TransactionTrait};
// TODO -> consider moving structs into these functions to avoid clone

pub async fn init_tree<'c, T>(
    parsing_result: &AccountCompressionInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    cl_audits: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(cl) = &parsing_result.tree_update {
        return insert_change_log(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await;
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
