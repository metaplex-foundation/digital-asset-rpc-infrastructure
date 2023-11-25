use crate::error::IngesterError;
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
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    Ok(())
}
