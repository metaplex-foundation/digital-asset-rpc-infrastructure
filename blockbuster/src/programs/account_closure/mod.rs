use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};
use solana_sdk::{pubkey::Pubkey, pubkeys};

use plerkle_serialization::AccountInfo;

pubkeys!(solana_program_id, "11111111111111111111111111111111");

pub struct ClosedAccountInfo {
    pub pubkey: Vec<u8>,
    pub owner: Vec<u8>,
}

#[allow(clippy::large_enum_variant)]
pub enum AccountClosureData {
    ClosedAccountInfo(ClosedAccountInfo),
    EmptyAccount,
}

impl ParseResult for AccountClosureData {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::AccountClosure(self)
    }
}

pub struct AccountClosureParser;

impl ProgramParser for AccountClosureParser {
    fn key(&self) -> Pubkey {
        solana_program_id()
    }
    fn key_match(&self, key: &Pubkey) -> bool {
        key == &solana_program_id()
    }

    fn handles_account_updates(&self) -> bool {
        true
    }

    fn handles_instructions(&self) -> bool {
        false
    }

    fn handle_account(
        &self,
        account_info: &AccountInfo,
    ) -> Result<Box<(dyn ParseResult + 'static)>, BlockbusterError> {
        let account_data: ClosedAccountInfo = match (account_info.pubkey(), account_info.owner()) {
            (Some(pubkey), Some(owner)) => ClosedAccountInfo {
                pubkey: pubkey.0.to_vec(),
                owner: owner.0.to_vec(),
            },
            _ => return Ok(Box::new(AccountClosureData::EmptyAccount)),
        };

        if account_info.lamports() == 0 {
            Ok(Box::new(AccountClosureData::ClosedAccountInfo(
                account_data,
            )))
        } else {
            Ok(Box::new(AccountClosureData::EmptyAccount))
        }
    }
}
