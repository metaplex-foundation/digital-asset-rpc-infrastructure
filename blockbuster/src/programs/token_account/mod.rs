use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};
use solana_sdk::{program_pack::Pack, pubkey::Pubkey, pubkeys};
use spl_token::state::{Account as TokenAccount, Mint};

pubkeys!(
    token_program_id,
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
);

pub struct TokenAccountParser;

pub enum TokenProgramAccount {
    Mint(Mint),
    TokenAccount(TokenAccount),
}

impl ParseResult for TokenProgramAccount {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::TokenProgramAccount(self)
    }
}

impl ProgramParser for TokenAccountParser {
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
        false
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

                TokenProgramAccount::TokenAccount(token_account)
            }
            82 => {
                let mint = Mint::unpack(account_data).map_err(|_| {
                    BlockbusterError::CustomDeserializationError(
                        "Token MINT Unpack Failed".to_string(),
                    )
                })?;

                TokenProgramAccount::Mint(mint)
            }
            _ => {
                return Err(BlockbusterError::InvalidDataLength);
            }
        };

        Ok(Box::new(account_type))
    }
}

pub trait IsNonFungible {
    fn is_non_fungible(&self) -> bool;
}

impl IsNonFungible for Mint {
    fn is_non_fungible(&self) -> bool {
        self.decimals == 0 && self.mint_authority.is_none() && self.supply == 1
    }
}
