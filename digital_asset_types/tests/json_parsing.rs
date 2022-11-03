#[cfg(test)]
use digital_asset_types::dapi::asset::v1_content_from_json;
use digital_asset_types::dao::asset_data;
use digital_asset_types::dao::sea_orm_active_enums::{ChainMutability, Mutability};
use digital_asset_types::json::ChainDataV1;
use blockbuster::token_metadata::state::TokenStandard as TSBlockbuster;
use digital_asset_types::rpc::Content;
use mpl_bubblegum::state::metaplex_adapter::{
    MetadataArgs, TokenProgramVersion, TokenStandard,
};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use tokio;
use digital_asset_types::rpc::File;

pub async fn test_json(uri: String) -> Content {
    let metadata_1 = MetadataArgs {
        name: String::from("Handalf"),
        symbol: String::from(""),
        uri,
        primary_sale_happened: true,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        token_program_version: TokenProgramVersion::Original,
        creators: vec![].to_vec(),
        seller_fee_basis_points: 0,
    };

    let body: serde_json::Value = reqwest::get(metadata_1.uri.clone())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let asset_data = asset_data::Model {
        id: Keypair::new().pubkey().to_bytes().to_vec(),
        chain_data_mutability: ChainMutability::Mutable,
        chain_data: serde_json::to_value(ChainDataV1 {
            name: String::from("Handalf"),
            symbol: String::from(""),
            edition_nonce: None,
            primary_sale_happened: true,
            token_standard: Some(TSBlockbuster::NonFungible),
            uses: None,
        })
            .unwrap(),
        metadata_url: metadata_1.uri,
        metadata_mutability: Mutability::Mutable,
        metadata: body,
        slot_updated: 0,
    };

    v1_content_from_json(&asset_data).unwrap()
}

#[tokio::test]
async fn simple_v1_content() {
    let c = test_json("https://arweave.net/pIe_btAJIcuymBjOFAmVZ3GSGPyi2yY_30kDdHmQJzs".to_string()).await;
    assert_eq!(
        c.files,
        Some(vec![File {
            uri: Some(
                "https://arweave.net/UicDlez8No5ruKmQ1-Ik0x_NNxc40mT8NEGngWyXyMY".to_string()
            ),
            mime: None,
            quality: None,
            contexts: None,
        }, ])
    )
}

#[tokio::test]
async fn more_complex_content_v1() {
    let c = test_json("https://arweave.net/gfO_TkYttQls70pTmhrdMDz9pfMUXX8hZkaoIivQjGs".to_string()).await;
    assert_eq!(
        c.files.map(|mut s|{
            s.sort_by_key(|f|{
              f.uri.clone()
            });
            s
        } ),
        Some(vec![File {
            uri: Some(
                "https://arweave.net/hdtrCCqLXF2UWwf3h6YEFj8VF1ObDMGfGeQheVuXuG4".to_string()
            ),
            mime: None,
            quality: None,
            contexts: None,
        }, File {
            uri: Some(
                "https://arweave.net/hdtrCCqLXF2UWwf3h6YEFj8VF1ObDMGfGeQheVuXuG4?ext=png".to_string(),
            ),
            mime: Some("image/png".to_string()),
            quality: None,
            contexts: None,
        }])
    )
}
