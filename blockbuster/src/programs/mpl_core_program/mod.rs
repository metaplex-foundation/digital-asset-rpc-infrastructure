use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};
use borsh::BorshDeserialize;
use mpl_core::{
    accounts::GroupV1,
    types::Key,
    IndexableAsset,
};
use solana_sdk::{pubkey::Pubkey, pubkeys};

pubkeys!(mpl_core_id, "CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

#[derive(Clone, Debug, PartialEq)]
pub enum MplCoreAccountData {
    Asset(IndexableAsset),
    Collection(IndexableAsset),
    Group {
        indexable_asset: IndexableAsset,
        group: GroupV1,
    },
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
            Key::GroupV1 => {
                let group = GroupV1::from_bytes(account_data)?;
                let indexable_asset = IndexableAsset::fetch(key, account_data)?;
                MplCoreAccountState {
                    key,
                    data: MplCoreAccountData::Group {
                        indexable_asset,
                        group,
                    },
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

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;
    use mpl_core::types::UpdateAuthority;

    #[test]
    fn handle_account_parses_group_v1() {
        let parser = MplCoreParser;
        let group = GroupV1 {
            key: Key::GroupV1,
            update_authority: [1u8; 32].into(),
            name: "MPL Core Group".to_string(),
            uri: "https://example.com/group.json".to_string(),
            collections: vec![[2u8; 32].into()],
            groups: vec![[3u8; 32].into()],
            parent_groups: vec![[4u8; 32].into(), [5u8; 32].into()],
            assets: vec![[6u8; 32].into()],
        };
        let bytes = group.try_to_vec().unwrap();

        let parsed = parser.handle_account(&bytes).unwrap();
        match parsed.result_type() {
            ProgramParseResult::MplCore(state) => {
                assert_eq!(state.key, Key::GroupV1);
                match &state.data {
                    MplCoreAccountData::Group {
                        indexable_asset,
                        group: parsed_group,
                    } => {
                        assert_eq!(parsed_group, &group);
                        assert_eq!(indexable_asset.owner, Some(group.update_authority));
                        assert_eq!(
                            indexable_asset.update_authority,
                            UpdateAuthority::Address(group.update_authority)
                        );
                        assert_eq!(indexable_asset.name, group.name);
                        assert_eq!(indexable_asset.uri, group.uri);
                    }
                    _ => panic!("Expected MplCore group parser result"),
                }
            }
            _ => panic!("Expected MplCore parser result"),
        }
    }

    #[test]
    fn handle_account_empty_data_is_empty_account() {
        let parser = MplCoreParser;
        let parsed = parser.handle_account(&[]).unwrap();

        match parsed.result_type() {
            ProgramParseResult::MplCore(state) => {
                assert_eq!(state.key, Key::Uninitialized);
                assert_eq!(state.data, MplCoreAccountData::EmptyAccount);
            }
            _ => panic!("Expected MplCore parser result"),
        }
    }

    #[test]
    fn handle_account_unsupported_key_returns_not_implemented() {
        let parser = MplCoreParser;
        let unsupported_data = vec![Key::PluginHeaderV1 as u8];

        assert!(matches!(
            parser.handle_account(&unsupported_data),
            Err(BlockbusterError::AccountTypeNotImplemented)
        ));
    }
}
