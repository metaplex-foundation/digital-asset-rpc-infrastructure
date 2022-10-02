mod get_asset_by_id;
mod get_assets_by_creator;
mod get_assets_by_group;
mod get_assets_by_owner;
mod get_candy_machine_by_id;

use sea_orm::{JsonValue, Set};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

use crate::{
    adapter::{Collection, Creator, TokenProgramVersion, TokenStandard, Uses},
    dao::{
        asset, asset_authority, asset_creators, asset_data, asset_grouping, candy_machine,
        candy_machine_data,
        sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
    },
    json::ChainDataV1,
};

#[derive(Clone)]
pub struct MetadataArgs {
    /// The name of the asset
    pub name: String,
    /// The symbol for the asset
    pub symbol: String,
    /// URI pointing to JSON representing the asset
    pub uri: String,
    /// Royalty basis points that goes to creators in secondary sales (0-10000)
    pub seller_fee_basis_points: u16,
    // Immutable, once flipped, all sales of this metadata are considered secondary.
    pub primary_sale_happened: bool,
    // Whether or not the data struct is mutable, default is not
    pub is_mutable: bool,
    /// nonce for easy calculation of editions, if present
    pub edition_nonce: Option<u8>,
    /// Since we cannot easily change Metadata, we add the new DataV2 fields here at the end.
    pub token_standard: Option<TokenStandard>,
    /// Collection
    pub collection: Option<Collection>,
    /// Uses
    pub uses: Option<Uses>,
    pub token_program_version: TokenProgramVersion,
    pub creators: Vec<Creator>,
}

pub fn create_candy_machine(
    id: Vec<u8>,
    features: Option<u64>,
    authority: Vec<u8>,
    mint_authority: Option<Vec<u8>>,
    wallet: Option<Vec<u8>>,
    token_mint: Option<Vec<u8>>,
    items_redeemed: u64,
    candy_guard_pda: Option<Vec<u8>>,
    version: u8,
) -> (candy_machine::ActiveModel, candy_machine::Model) {
    (
        candy_machine::ActiveModel {
            id: Set(id),
            features: Set(features),
            authority: Set(authority),
            mint_authority: Set(mint_authority),
            wallet: Set(wallet),
            token_mint: Set(token_mint),
            items_redeemed: Set(items_redeemed),
            candy_guard_pda: Set(candy_guard_pda),
            version: Set(version),
            collection_mint: Set(collection_mint),
            allow_thaw: Set(allow_thaw),
            frozen_count: Set(frozen_count),
            mint_start: Set(mint_start),
            freeze_time: Set(freeze_time),
            freeze_fee: Set(freeze_fee),
            created_at: Set(created_at),
            last_minted: Set(last_minted),
        },
        candy_machine::Model {
            id,
            features,
            authority,
            mint_authority,
            wallet,
            token_mint,
            items_redeemed,
            candy_guard_pda,
            version,
            collection_mint,
            allow_thaw,
            frozen_count,
            mint_start,
            freeze_time,
            freeze_fee,
            created_at,
            last_minted,
        },
    )
}

pub fn create_candy_machine_data(
    row_num: i64,
    uuid: Option<String>,
    price: Option<u64>,
    symbol: String,
    seller_fee_basis_points: u16,
    max_supply: u64,
    is_mutable: bool,
    retain_authority: Option<bool>,
    go_live_date: Option<i64>,
    items_available: u64,
    candy_machine_id: Vec<u8>,
) -> (candy_machine_data::ActiveModel, candy_machine_data::Model) {
    (
        candy_machine_data::ActiveModel {
            uuid: Set(uuid.clone()),
            price: Set(price),
            symbol: Set(symbol.clone()),
            seller_fee_basis_points: Set(seller_fee_basis_points),
            max_supply: Set(max_supply),
            is_mutable: Set(is_mutable),
            retain_authority: Set(retain_authority),
            go_live_date: Set(go_live_date),
            items_available: Set(items_available),
            candy_machine_id: Set(candy_machine_id.clone()),
            ..Default::default()
        },
        candy_machine_data::Model {
            id: row_num,
            uuid,
            price,
            symbol,
            seller_fee_basis_points,
            max_supply,
            is_mutable,
            retain_authority,
            go_live_date,
            items_available,
            candy_machine_id,
            whitelist_mode: todo!(),
            whitelist_mint: todo!(),
            whitelist_presale: todo!(),
            whitelist_discount_price: todo!(),
            gatekeeper_network: todo!(),
            gatekeeper_expire_on_use: todo!(),
            config_line_settings_prefix_name: todo!(),
            config_line_settings_name_length: todo!(),
            config_line_settings_prefix_uri: todo!(),
            config_line_settings_uri_length: todo!(),
            config_line_settings_is_sequential: todo!(),
            end_setting_number: todo!(),
            end_setting_type: todo!(),
            hidden_settings_name: todo!(),
            hidden_settings_uri: todo!(),
            hidden_settings_hash: todo!(),
        },
    )
}

pub fn create_asset_data(
    metadata: MetadataArgs,
    row_num: i64,
) -> (asset_data::ActiveModel, asset_data::Model) {
    let chain_data = ChainDataV1 {
        name: metadata.name,
        symbol: metadata.symbol,
        edition_nonce: metadata.edition_nonce,
        primary_sale_happened: metadata.primary_sale_happened,
        token_standard: metadata.token_standard,
        uses: None,
    };

    let chain_data_json = serde_json::to_value(chain_data).unwrap();

    let chain_mutability = match metadata.is_mutable {
        true => ChainMutability::Mutable,
        false => ChainMutability::Immutable,
    };

    (
        asset_data::ActiveModel {
            chain_data_mutability: Set(chain_mutability),
            schema_version: Set(1),
            chain_data: Set(chain_data_json),
            metadata_url: Set(metadata.uri),
            metadata: Set(JsonValue::String("processing".to_string())),
            metadata_mutability: Set(Mutability::Mutable),
            ..Default::default()
        },
        asset_data::Model {
            id: row_num,
            chain_data_mutability: ChainMutability::Mutable,
            schema_version: 1,
            chain_data: serde_json::to_value(ChainDataV1 {
                name: String::from("Test #`row_num`"),
                symbol: String::from("BUBBLE"),
                edition_nonce: None,
                primary_sale_happened: true,
                token_standard: Some(TokenStandard::NonFungible),
                uses: None,
            })
            .unwrap(),
            metadata_url: Keypair::new().pubkey().to_string(),
            metadata_mutability: Mutability::Mutable,
            metadata: JsonValue::String("processing".to_string()),
        },
    )
}

pub fn create_asset(
    id: Vec<u8>,
    owner: Vec<u8>,
    owner_type: OwnerType,
    delegate: Option<Vec<u8>>,
    frozen: bool,
    supply: i64,
    supply_mint: Option<Vec<u8>>,
    compressed: bool,
    compressible: bool,
    tree_id: Option<Vec<u8>>,
    specification_version: i32,
    nonce: i64,
    leaf: Option<Vec<u8>>,
    royalty_target_type: RoyaltyTargetType,
    royalty_target: Option<Vec<u8>>,
    royalty_amount: i32,
    chain_data_id: Option<i64>,
) -> (asset::ActiveModel, asset::Model) {
    (
        asset::ActiveModel {
            id: Set(id.clone()),
            owner: Set(owner.clone()),
            owner_type: Set(owner_type.clone()),
            delegate: Set(delegate.clone()),
            frozen: Set(frozen),
            supply: Set(supply),
            supply_mint: Set(supply_mint.clone()),
            compressed: Set(compressed),
            compressible: Set(compressible),
            tree_id: Set(tree_id.clone()),
            specification_version: Set(specification_version),
            nonce: Set(nonce),
            leaf: Set(leaf.clone()),
            royalty_target_type: Set(royalty_target_type.clone()),
            royalty_target: Set(royalty_target.clone()),
            royalty_amount: Set(royalty_amount), //basis points
            chain_data_id: Set(chain_data_id),
            ..Default::default()
        },
        asset::Model {
            id,
            owner,
            owner_type,
            delegate,
            frozen,
            supply,
            supply_mint,
            compressed,
            compressible,
            tree_id,
            specification_version,
            nonce,
            leaf,
            royalty_target_type,
            royalty_target,
            royalty_amount,
            chain_data_id,
            burnt: false,
            created_at: None,
            seq: 1,
        },
    )
}

pub fn create_asset_creator(
    asset_id: Vec<u8>,
    creator: Vec<u8>,
    share: i32,
    verified: bool,
    row_num: i64,
) -> (asset_creators::ActiveModel, asset_creators::Model) {
    (
        asset_creators::ActiveModel {
            asset_id: Set(asset_id.clone()),
            creator: Set(creator.clone()),
            share: Set(share),
            verified: Set(verified),
            ..Default::default()
        },
        asset_creators::Model {
            id: row_num,
            asset_id,
            creator,
            share,
            verified,
            seq: 1,
        },
    )
}

pub fn create_asset_authority(
    asset_id: Vec<u8>,
    update_authority: Vec<u8>,
    row_num: i64,
) -> (asset_authority::ActiveModel, asset_authority::Model) {
    (
        asset_authority::ActiveModel {
            asset_id: Set(asset_id.clone()),
            authority: Set(update_authority.clone()),
            ..Default::default()
        },
        asset_authority::Model {
            asset_id,
            authority: update_authority,
            id: row_num,
            scopes: None,
            seq: 1,
        },
    )
}

pub fn create_asset_grouping(
    asset_id: Vec<u8>,
    collection: Pubkey,
    row_num: i64,
) -> (asset_grouping::ActiveModel, asset_grouping::Model) {
    (
        asset_grouping::ActiveModel {
            asset_id: Set(asset_id.clone()),
            group_key: Set(String::from("collection")),
            group_value: Set(bs58::encode(collection).into_string()),
            ..Default::default()
        },
        asset_grouping::Model {
            asset_id,
            group_value: bs58::encode(collection).into_string(),
            id: row_num,
            group_key: "collection".to_string(),
            seq: 1,
        },
    )
}
