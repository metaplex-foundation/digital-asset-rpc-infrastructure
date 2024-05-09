use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};
use borsh::BorshDeserialize;
use mpl_core::{types::Key, IndexableAsset};
use solana_sdk::{pubkey::Pubkey, pubkeys};

pubkeys!(mpl_core_id, "CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

#[derive(Clone, Debug, PartialEq)]
pub enum MplCoreAccountData {
    Asset(IndexableAsset),
    Collection(IndexableAsset),
    HashedAsset,
    EmptyAccount,
}

pub struct MplCoreAccountState {
    pub key: Key,
    pub data: MplCoreAccountData,
}

impl ParseResult for MplCoreAccountState {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::MplCore(self)
    }
}

pub struct MplCoreParser;

impl ProgramParser for MplCoreParser {
    fn key(&self) -> Pubkey {
        mpl_core_id()
    }
    fn key_match(&self, key: &Pubkey) -> bool {
        key == &mpl_core_id()
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
        if account_data.is_empty() {
            return Ok(Box::new(MplCoreAccountState {
                key: Key::Uninitialized,
                data: MplCoreAccountData::EmptyAccount,
            }));
        }
        let key = Key::try_from_slice(&account_data[0..1])?;
        let mpl_core_account_state = match key {
            Key::AssetV1 => {
                let indexable_asset = IndexableAsset::fetch(key, account_data)?;
                MplCoreAccountState {
                    key,
                    data: MplCoreAccountData::Asset(indexable_asset),
                }
            }
            Key::CollectionV1 => {
                let indexable_asset = IndexableAsset::fetch(key, account_data)?;
                MplCoreAccountState {
                    key,
                    data: MplCoreAccountData::Collection(indexable_asset),
                }
            }
            Key::Uninitialized => MplCoreAccountState {
                key: Key::Uninitialized,
                data: MplCoreAccountData::EmptyAccount,
            },
            _ => {
                return Err(BlockbusterError::AccountTypeNotImplemented);
            }
        };

        Ok(Box::new(mpl_core_account_state))
    }
}
