//! MPL Agent Registry (`mpl-agent-identity`) account parsing for indexing.
//!
//! Layout matches `AgentIdentityV1` / `AgentIdentityV2` in `mpl-agent-identity`
//! (`Key` discriminator at byte 0, `asset` at bytes 8..40, optional mint at 40..72 for V2).

use solana_sdk::{pubkey::Pubkey, pubkeys};

use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};

pubkeys!(
    agent_registry_program_id,
    "1DREGFgysWYxLnRnKQnwrxnJQeSMk2HmGaC6whw2B2p"
);

/// Registry account discriminator (`mpl_agent_identity::state::Key`).
pub const KEY_AGENT_IDENTITY_V1: u8 = 1;
pub const KEY_AGENT_IDENTITY_V2: u8 = 2;

const AGENT_IDENTITY_V1_LEN: usize = 40;
const AGENT_IDENTITY_V2_LEN: usize = 104;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAgentIdentity {
    pub asset: Pubkey,
    pub agent_token_mint: Option<Pubkey>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRegistryAccount {
    pub inner: Option<ParsedAgentIdentity>,
}

impl AgentRegistryAccount {
    pub fn parse(account_data: &[u8]) -> Self {
        if account_data.is_empty() {
            return Self::default();
        }
        let key = account_data[0];
        match key {
            KEY_AGENT_IDENTITY_V1 if account_data.len() >= AGENT_IDENTITY_V1_LEN => {
                let Ok(asset) = Pubkey::try_from(&account_data[8..40]) else {
                    return Self::default();
                };
                Self {
                    inner: Some(ParsedAgentIdentity {
                        asset,
                        agent_token_mint: None,
                    }),
                }
            }
            KEY_AGENT_IDENTITY_V2 if account_data.len() >= AGENT_IDENTITY_V2_LEN => {
                let Ok(asset) = Pubkey::try_from(&account_data[8..40]) else {
                    return Self::default();
                };
                let Ok(mint_pk) = Pubkey::try_from(&account_data[40..72]) else {
                    return Self::default();
                };
                let agent_token_mint = (mint_pk != Pubkey::default()).then_some(mint_pk);
                Self {
                    inner: Some(ParsedAgentIdentity {
                        asset,
                        agent_token_mint,
                    }),
                }
            }
            _ => Self::default(),
        }
    }
}

impl ParseResult for AgentRegistryAccount {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::AgentRegistry(self)
    }
}

pub struct AgentRegistryParser;

impl ProgramParser for AgentRegistryParser {
    fn key(&self) -> Pubkey {
        agent_registry_program_id()
    }

    fn key_match(&self, key: &Pubkey) -> bool {
        key == &agent_registry_program_id()
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
    ) -> Result<Box<dyn ParseResult>, BlockbusterError> {
        Ok(Box::new(AgentRegistryAccount::parse(account_data)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v1_parses_asset() {
        let asset = Pubkey::new_unique();
        let mut data = vec![0u8; AGENT_IDENTITY_V1_LEN];
        data[0] = KEY_AGENT_IDENTITY_V1;
        data[8..40].copy_from_slice(asset.as_ref());

        let inner = AgentRegistryAccount::parse(&data).inner.expect("parsed");
        assert_eq!(inner.asset, asset);
        assert!(inner.agent_token_mint.is_none());
    }

    #[test]
    fn parses_v1_too_short_returns_empty() {
        let mut data = vec![0u8; 20];
        data[0] = KEY_AGENT_IDENTITY_V1;
        assert!(AgentRegistryAccount::parse(&data).inner.is_none());
    }

    #[test]
    fn parses_v2_with_token() {
        let asset = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let mut data = vec![0u8; AGENT_IDENTITY_V2_LEN];
        data[0] = KEY_AGENT_IDENTITY_V2;
        data[8..40].copy_from_slice(asset.as_ref());
        data[40..72].copy_from_slice(mint.as_ref());

        let parsed = AgentRegistryAccount::parse(&data);
        let inner = parsed.inner.expect("parsed");
        assert_eq!(inner.asset, asset);
        assert_eq!(inner.agent_token_mint, Some(mint));
    }

    #[test]
    fn parses_v2_without_token() {
        let asset = Pubkey::new_unique();
        let mut data = vec![0u8; AGENT_IDENTITY_V2_LEN];
        data[0] = KEY_AGENT_IDENTITY_V2;
        data[8..40].copy_from_slice(asset.as_ref());

        let inner = AgentRegistryAccount::parse(&data).inner.expect("parsed");
        assert_eq!(inner.asset, asset);
        assert!(inner.agent_token_mint.is_none());
    }
}
