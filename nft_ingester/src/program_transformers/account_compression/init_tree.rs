use crate::{
    error::IngesterError, program_transformers::account_compression::save_changelog_event,
    tasks::TaskData,
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::account_compression::{AccountCompressionInstruction, Instruction},
};
use sea_orm::{ConnectionTrait, TransactionTrait};
// TODO -> consider moving structs into these functions to avoid clone

pub async fn init_tree<'c, T>(
    parsing_result: &AccountCompressionInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    cl_audits: bool,
) -> Result<Option<TaskData>, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Instruction::Append { leaf: _ }, Some(cl)) =
        (&parsing_result.instruction, &parsing_result.tree_update)
    {
        let _seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await?;
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
