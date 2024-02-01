#[cfg(test)]
use blockbuster::token_metadata::types::TokenStandard as TSBlockbuster;
use digital_asset_types::dao::asset_data;
use digital_asset_types::dao::sea_orm_active_enums::{ChainMutability, Mutability};
use digital_asset_types::dapi::common::v1_content_from_json;
use digital_asset_types::json::ChainDataV1;
use digital_asset_types::rpc::Content;
use digital_asset_types::rpc::File;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

pub async fn load_test_json(file_name: &str) -> serde_json::Value {
    let json = tokio::fs::read_to_string(format!("tools/data/{}", file_name))
        .await
        .unwrap();
    serde_json::from_str(&json).unwrap()
}

pub async fn parse_onchain_json(json: serde_json::Value) -> Content {
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
        metadata_url: String::from("some url"),
        metadata_mutability: Mutability::Mutable,
        metadata: json,
        slot_updated: 0,
        reindex: None,
        raw_name: Some(String::from("Handalf").into_bytes().to_vec()),
        raw_symbol: Some(String::from("").into_bytes().to_vec()),
        base_info_seq: Some(0),
    };

    v1_content_from_json(&asset_data).unwrap()
}

#[tokio::test]
async fn simple_content() {
    let j = load_test_json("mad_lad.json").await;
    let parsed = parse_onchain_json(j.clone()).await;
    assert_eq!(
        parsed.files,
        Some(vec![
            File {
                uri: Some("https://madlads.s3.us-west-2.amazonaws.com/images/1.png".to_string()),
                mime: Some("image/png".to_string()),
                quality: None,
                contexts: None,
            },
            File {
                uri: Some(
                    "https://arweave.net/qJ5B6fx5hEt4P7XbicbJQRyTcbyLaV-OQNA1KjzdqOQ/1.png"
                        .to_string(),
                ),
                mime: Some("image/png".to_string()),
                quality: None,
                contexts: None,
            }
        ])
    );

    assert_eq!(
        parsed
            .clone()
            .links
            .unwrap()
            .get("image")
            .unwrap()
            .as_str()
            .unwrap(),
        "https://madlads.s3.us-west-2.amazonaws.com/images/1.png"
    );
    assert_eq!(
        parsed
            .links
            .unwrap()
            .get("external_url")
            .unwrap()
            .as_str()
            .unwrap(),
        "https://madlads.com"
    );
}

#[tokio::test]
async fn complex_content() {
    let j = load_test_json("infinite_fungi.json").await;
    let parsed = parse_onchain_json(j).await;
    assert_eq!(
        parsed.files,
        Some(vec![
            File {
                uri: Some(
                    "https://arweave.net/_a4sXT6fOHI-5VHFOHLEF73wqKuZtJgE518Ciq9DGyI?ext=gif"
                        .to_string(),
                ),
                mime: Some("image/gif".to_string()),
                quality: None,
                contexts: None,
            },
            File {
                uri: Some(
                    "https://arweave.net/HVOJ3bTpqMJJJtd5nW2575vPTekLa_SSDsQc7AqV_Ho?ext=mp4"
                        .to_string()
                ),
                mime: Some("video/mp4".to_string()),
                quality: None,
                contexts: None,
            },
        ])
    );
    assert_eq!(
        parsed
            .clone()
            .links
            .unwrap()
            .get("image")
            .unwrap()
            .as_str()
            .unwrap(),
        "https://arweave.net/_a4sXT6fOHI-5VHFOHLEF73wqKuZtJgE518Ciq9DGyI?ext=gif"
    );
    assert_eq!(
        parsed
            .links
            .unwrap()
            .get("animation_url")
            .unwrap()
            .as_str()
            .unwrap(),
        "https://arweave.net/HVOJ3bTpqMJJJtd5nW2575vPTekLa_SSDsQc7AqV_Ho?ext=mp4"
    );
}
