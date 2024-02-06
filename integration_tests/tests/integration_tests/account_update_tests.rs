use borsh::BorshSerialize;
use function_name::named;

use das_api::api::{self, ApiContract};

use plerkle_serialization::{
    root_as_account_info, serializer::serialize_account,
    solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2,
};
use serial_test::serial;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{program_option::COption, program_pack::Pack};
use spl_token::state::{Account as TokenAccount, AccountState, Mint};

use super::common::*;

#[derive(Debug, Clone)]
// TODO: Add amount
struct TokenAccountUpdate {
    owner: Pubkey,
    delegate: COption<Pubkey>,
    state: AccountState,
}

#[derive(Debug, Clone)]
struct MintAccountUpdate {
    supply: u64,
}

#[derive(Debug, Clone)]
struct MetadataAccountUpdate {
    primary_sale_happened: bool,
    is_mutable: bool,
}

#[derive(Debug, Clone)]
enum AccountUpdate {
    TokenAccount(TokenAccountUpdate),
    #[allow(dead_code)]
    MintAccount(MintAccountUpdate),
    MetadataAccount(MetadataAccountUpdate),
    None,
}

macro_rules! update_field {
    ($field:expr, $value:expr) => {
        assert_ne!($field, $value);
        $field = $value;
    };
}

async fn index_account_update(setup: &TestSetup, pubkey: Pubkey, update: AccountUpdate, slot: u64) {
    let account_bytes = cached_fetch_account(&setup, pubkey.clone(), None).await;

    let account_info = root_as_account_info(&account_bytes).unwrap();
    let account_data = account_info.data().unwrap().iter().collect::<Vec<_>>();

    let modified_account_data = match update {
        AccountUpdate::TokenAccount(TokenAccountUpdate {
            owner,
            delegate,
            state,
        }) => {
            let mut account = TokenAccount::unpack(&account_data).unwrap();

            update_field!(account.owner, owner);
            update_field!(account.delegate, delegate);
            update_field!(account.state, state);

            let mut data = vec![0; TokenAccount::LEN];
            TokenAccount::pack(account, data.as_mut_slice()).unwrap();
            data
        }
        AccountUpdate::MintAccount(MintAccountUpdate { supply }) => {
            let mut account = Mint::unpack(&account_data).unwrap();

            update_field!(account.supply, supply);

            let mut data = vec![0; Mint::LEN];
            Mint::pack(account, data.as_mut_slice()).unwrap();
            data
        }
        AccountUpdate::MetadataAccount(MetadataAccountUpdate {
            primary_sale_happened,
            is_mutable,
        }) => {
            let mut account: mpl_token_metadata::accounts::Metadata =
                mpl_token_metadata::accounts::Metadata::from_bytes(&account_data).unwrap();

            update_field!(account.primary_sale_happened, primary_sale_happened);
            update_field!(account.is_mutable, is_mutable);

            account.try_to_vec().unwrap()
        }
        AccountUpdate::None => account_data,
    };

    let fbb = flatbuffers::FlatBufferBuilder::new();

    let account_info = ReplicaAccountInfoV2 {
        pubkey: &account_info.pubkey().unwrap().0,
        lamports: account_info.lamports(),
        owner: account_info.owner().unwrap().0.as_ref(),
        executable: account_info.executable(),
        rent_epoch: account_info.rent_epoch(),
        data: &modified_account_data,
        write_version: 0,
        txn_signature: None,
    };
    let is_startup = false;

    let fbb = serialize_account(fbb, &account_info, slot, is_startup);
    index_account_bytes(setup, fbb.finished_data().to_vec()).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_account_updates() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;
    let mint = Pubkey::try_from("843gdpsTE4DoJz3ZoBsEjAqT8UgAcyF5YojygGgGZE1f").unwrap();

    let nft_accounts = get_nft_accounts(&setup, mint).await;

    let request = api::GetAsset {
        id: mint.to_string(),
        ..api::GetAsset::default()
    };

    let random_pub_key = Pubkey::try_from("1111111QLbz7JHiBTspS962RLKV8GndWFwiEaqKM").unwrap();
    let random_pub_key2 = Pubkey::try_from("1111111ogCyDbaRMvkdsHB3qfdyFYaG1WtRUAfdh").unwrap();

    #[derive(Clone)]
    struct NamedUpdate {
        name: String,
        account: Pubkey,
        update: AccountUpdate,
    }

    let token_updated = NamedUpdate {
        name: "token".to_string(),
        account: nft_accounts.token,
        update: AccountUpdate::TokenAccount(TokenAccountUpdate {
            owner: random_pub_key,
            delegate: COption::Some(random_pub_key2),
            state: AccountState::Initialized,
        }),
    };
    let mint_updated = NamedUpdate {
        name: "mint".to_string(),
        account: nft_accounts.mint,
        // You can't easily change an NFT's mint account. The supply is fixed to 1 unless you burn
        // the token account, the decimals are fixed at 0 and the freeze authority is not displayed
        // in the API.
        update: AccountUpdate::None,
    };
    let metadata_updated = NamedUpdate {
        name: "metadata".to_string(),
        account: nft_accounts.metadata,
        update: AccountUpdate::MetadataAccount(MetadataAccountUpdate {
            primary_sale_happened: true,
            is_mutable: false,
        }),
    };
    let named_updates = vec![token_updated, mint_updated, metadata_updated];

    // Test that stale updates are rejected and new updates are accepted
    for named_update in named_updates.clone() {
        if let AccountUpdate::None = named_update.update {
            continue;
        }
        apply_migrations_and_delete_data(setup.db.clone()).await;
        index_nft(&setup, mint).await;

        let response = setup.das_api.get_asset(request.clone()).await.unwrap();
        insta::assert_json_snapshot!(name.clone(), response);

        index_account_update(
            &setup,
            named_update.account,
            named_update.update.clone(),
            DEFAULT_SLOT - 1,
        )
        .await;
        let response_stale_lot = setup.das_api.get_asset(request.clone()).await.unwrap();
        assert_eq!(
            response, response_stale_lot,
            "Update for {} account was not rejected",
            named_update.name
        );

        index_account_update(
            &setup,
            named_update.account,
            named_update.update.clone(),
            DEFAULT_SLOT + 1,
        )
        .await;
        let response_new_slot = setup.das_api.get_asset(request.clone()).await.unwrap();

        assert_ne!(response, response_new_slot);
        insta::assert_json_snapshot!(
            format!("{}-{}-updated", name, named_update.name),
            response_new_slot
        );
    }

    // Test that the different metadata/mint/token updates use different slots and don't interfere
    // with each other
    for named_update in named_updates.clone() {
        apply_migrations_and_delete_data(setup.db.clone()).await;
        index_nft(&setup, mint).await;

        let other_named_updates = named_updates
            .clone()
            .into_iter()
            .filter(|u| u.name != named_update.name)
            .collect::<Vec<_>>();

        let ordered_name_updates = other_named_updates
            .into_iter()
            .chain(vec![named_update])
            .collect::<Vec<_>>();

        for (i, named_update) in ordered_name_updates.into_iter().enumerate() {
            index_account_update(
                &setup,
                named_update.account,
                named_update.update.clone(),
                DEFAULT_SLOT + named_updates.len() as u64 - i as u64,
            )
            .await;
        }
        insta::assert_json_snapshot!(
            format!("{}-with-all-updates", name),
            setup.das_api.get_asset(request.clone()).await.unwrap()
        );
    }
}
