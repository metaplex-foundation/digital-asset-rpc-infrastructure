use blockbuster::token_metadata::state::*;
use digital_asset_types::dao::sea_orm_active_enums::{
    SpecificationAssetClass, SpecificationVersions,
};
use digital_asset_types::{
    dao::{
        asset, asset_authority, asset_creators, asset_data, asset_grouping,
        sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
    },
    json::ChainDataV1,
};
use sea_orm::{JsonValue, Set};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

#[derive(Clone)]
pub struct MockMetadataArgs {
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
    pub creators: Vec<Creator>,
}

pub fn create_asset_data(
    metadata: MockMetadataArgs,
    row_num: Vec<u8>,
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
            chain_data: Set(chain_data_json),
            metadata_url: Set(metadata.uri),
            metadata: Set(JsonValue::String("processing".to_string())),
            metadata_mutability: Set(Mutability::Mutable),
            ..Default::default()
        },
        asset_data::Model {
            id: row_num,
            chain_data_mutability: ChainMutability::Mutable,
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
            slot_updated: 0,
        },
    )
}

#[allow(clippy::too_many_arguments)]
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
    specification_version: Option<SpecificationVersions>,
    nonce: Option<i64>,
    leaf: Option<Vec<u8>>,
    royalty_target_type: RoyaltyTargetType,
    royalty_target: Option<Vec<u8>>,
    royalty_amount: i32,
) -> (asset::ActiveModel, asset::Model) {
    (
        asset::ActiveModel {
            id: Set(id.clone()),
            owner: Set(Some(owner.clone())),
            owner_type: Set(owner_type.clone()),
            delegate: Set(delegate.clone()),
            frozen: Set(frozen),
            supply: Set(supply),
            supply_mint: Set(supply_mint.clone()),
            compressed: Set(compressed),
            compressible: Set(compressible),
            tree_id: Set(tree_id.clone()),
            specification_version: Set(specification_version.clone()),
            nonce: Set(nonce),
            leaf: Set(leaf.clone()),
            royalty_target_type: Set(royalty_target_type.clone()),
            royalty_target: Set(royalty_target.clone()),
            royalty_amount: Set(royalty_amount), //basis points
            ..Default::default()
        },
        asset::Model {
            id: id.clone(),
            owner: Some(owner),
            owner_type,
            delegate,
            frozen,
            supply,
            supply_mint,
            compressed,
            compressible,
            seq: Some(0),
            tree_id,
            specification_version,
            nonce,
            leaf,
            royalty_target_type,
            royalty_target,
            royalty_amount,
            asset_data: Some(id),
            burnt: false,
            created_at: None,
            specification_asset_class: Some(SpecificationAssetClass::Nft),
            slot_updated: Some(0),
            data_hash: None,
            alt_id: None,
            creator_hash: None,
            compressed_seq: Some(0),
            owner_delegate_seq: Some(0),
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
            seq: 0,
            slot_updated: 0,
            position: 0,
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
            seq: 0,
            id: row_num,
            scopes: None,
            slot_updated: 0,
        },
    )
}

#[allow(dead_code)]
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
            seq: 0,
            id: row_num,
            group_key: "collection".to_string(),
            slot_updated: 0,
        },
    )
}
