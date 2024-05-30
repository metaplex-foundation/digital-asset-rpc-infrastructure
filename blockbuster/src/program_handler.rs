use crate::{
    error::BlockbusterError, instruction::InstructionBundle, programs::ProgramParseResult,
};
use solana_sdk::pubkey::Pubkey;

pub trait ParseResult: Sync + Send {
    fn result_type(&self) -> ProgramParseResult;

    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
}

pub struct NotUsed(());

impl NotUsed {
    pub fn new() -> Self {
        NotUsed(())
    }
}

impl Default for NotUsed {
    fn default() -> Self {
        Self::new()
    }
}

impl ParseResult for NotUsed {
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::Unknown
    }
}

pub trait ProgramParser: Sync + Send {
    fn key(&self) -> Pubkey;
    fn key_match(&self, key: &Pubkey) -> bool;
    fn handles_instructions(&self) -> bool;
    fn handles_account_updates(&self) -> bool;
    fn handle_account(
        &self,
        _account_data: &[u8],
    ) -> Result<Box<dyn ParseResult>, BlockbusterError>;
    fn handle_instruction(
        &self,
        _bundle: &InstructionBundle,
    ) -> Result<Box<dyn ParseResult>, BlockbusterError> {
        Ok(Box::new(NotUsed::new()))
    }
}
