use crate::{error::IngesterError, tasks::TaskData};
use blockbuster::{
    instruction::InstructionBundle, programs::account_compression::AccountCompressionInstruction,
};
use sea_orm::query::*;

// TODO -> consider moving structs into these functions to avoid clone

pub async fn close_tree<'c, T>(
    _parsing_result: &AccountCompressionInstruction,
    _bundle: &InstructionBundle<'c>,
    _txn: &'c T,
    _cl_audits: bool,
) -> Result<Option<TaskData>, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    Ok(None)
    // if let (Instruction::Append { leaf: _ }, Some(le), Some(cl)) = (
    //     &parsing_result.instruction,
    //     &parsing_result.leaf_update,
    //     &parsing_result.tree_update,
    // ) {
    //     let _seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await?;

    //     return match le {
    //         _ => Err(IngesterError::NotImplemented),
    //     };
    // }
    // Err(IngesterError::ParsingError(
    //     "Ix not parsed correctly".to_string(),
    // ))
}
