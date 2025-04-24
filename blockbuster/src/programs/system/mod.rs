use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};
use solana_sdk::{pubkey::Pubkey, pubkeys};

pubkeys!(system_program_id, "11111111111111111111111111111111");

pub struct SystemProgramParser;

pub struct SystemProgramAccount;

impl ParseResult for SystemProgramAccount {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::SystemProgramAccount(self)
    }
}

impl ProgramParser for SystemProgramParser {
    fn key(&self) -> Pubkey {
        system_program_id()
    }
    fn key_match(&self, key: &Pubkey) -> bool {
        key == &system_program_id()
    }
    fn handles_account_updates(&self) -> bool {
        true
    }

    fn handles_instructions(&self) -> bool {
        false
    }
    fn handle_account(
        &self,
        _account_data: &[u8],
    ) -> Result<Box<(dyn ParseResult + 'static)>, BlockbusterError> {
        Ok(Box::new(SystemProgramAccount))
    }
}
