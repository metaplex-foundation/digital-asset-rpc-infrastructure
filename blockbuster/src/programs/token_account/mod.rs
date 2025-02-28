use crate::{
    error::BlockbusterError,
    instruction::InstructionBundle,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};
use solana_sdk::{program_pack::Pack, pubkey::Pubkey, pubkeys};
use spl_token::state::{Account as TokenAccount, Mint};

pubkeys!(
    token_program_id,
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
);

pub struct TokenProgramParser;

pub enum TokenProgramEntity {
    Mint(Mint),
    TokenAccount(TokenAccount),
    CloseIx(Pubkey),
}

impl ParseResult for TokenProgramEntity {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::TokenProgramEntity(self)
    }
}

impl ProgramParser for TokenProgramParser {
    fn key(&self) -> Pubkey {
        token_program_id()
    }
    fn key_match(&self, key: &Pubkey) -> bool {
        key == &token_program_id()
    }
    fn handles_account_updates(&self) -> bool {
        true
    }

    fn handles_instructions(&self) -> bool {
        true
    }
    fn handle_account(
        &self,
        account_data: &[u8],
    ) -> Result<Box<(dyn ParseResult + 'static)>, BlockbusterError> {
        let account_type = match account_data.len() {
            165 => {
                let token_account = TokenAccount::unpack(account_data).map_err(|_| {
                    BlockbusterError::CustomDeserializationError(
                        "Token Account Unpack Failed".to_string(),
                    )
                })?;

                TokenProgramEntity::TokenAccount(token_account)
            }
            82 => {
                let mint = Mint::unpack(account_data).map_err(|_| {
                    BlockbusterError::CustomDeserializationError(
                        "Token MINT Unpack Failed".to_string(),
                    )
                })?;

                TokenProgramEntity::Mint(mint)
            }
            _ => {
                return Err(BlockbusterError::InvalidDataLength);
            }
        };

        Ok(Box::new(account_type))
    }

    fn handle_instruction(
        &self,
        bundle: &crate::instruction::InstructionBundle,
    ) -> Result<Box<dyn ParseResult>, BlockbusterError> {
        let InstructionBundle {
            txn_id: _,
            program: _,
            keys,
            instruction,
            inner_ix: _,
            slot: _,
        } = bundle;

        if let Some(ix) = instruction {
            if !ix.data.is_empty() && ix.data[0] == 9 && !keys.is_empty() {
                return Ok(Box::new(TokenProgramEntity::CloseIx(keys[0])));
            } else {
                return Err(BlockbusterError::InstructionTypeNotImplemented);
            }
        }

        Err(BlockbusterError::InstructionParsingError)
    }
}
