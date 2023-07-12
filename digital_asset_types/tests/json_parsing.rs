#[cfg(test)]
use blockbuster::token_metadata::state::TokenStandard as TSBlockbuster;
use digital_asset_types::dao::asset_data;
use digital_asset_types::dao::sea_orm_active_enums::{ChainMutability, Mutability};
use digital_asset_types::dapi::common::v1_content_from_json;
use digital_asset_types::json::ChainDataV1;
use digital_asset_types::rpc::Content;
use digital_asset_types::rpc::File;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

pub async fn test_json(uri: String) -> Content {
    let body: serde_json::Value = reqwest::get(&uri).await.unwrap().json().await.unwrap();

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
        metadata_url: uri,
        metadata_mutability: Mutability::Mutable,
        metadata: body,
        slot_updated: 0,
    };

    v1_content_from_json(&asset_data).unwrap()
}

#[tokio::test]
async fn simple_v1_content() {
    let c =
        test_json("https://arweave.net/pIe_btAJIcuymBjOFAmVZ3GSGPyi2yY_30kDdHmQJzs".to_string())
            .await;
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
            .clone()
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
async fn more_complex_content_v1() {
    let c =
        test_json("https://arweave.net/gfO_TkYttQls70pTmhrdMDz9pfMUXX8hZkaoIivQjGs".to_string())
            .await;
    assert_eq!(
        c.files.map(|mut s| {
            s.sort_by_key(|f| f.uri.clone());
            s
        }),
        Some(vec![
            File {
                uri: Some(
                    "https://arweave.net/hdtrCCqLXF2UWwf3h6YEFj8VF1ObDMGfGeQheVuXuG4".to_string()
                ),
                mime: Some("image/png".to_string()),
                quality: None,
                contexts: None,
            }
        ])
    );
}

#[tokio::test]
async fn complex_content() {
    let cdn_prefix = None;
    let j = load_test_json("infinite_fungi.json").await;
    let parsed = parse_offchain_json(j, cdn_prefix).await;
    assert_eq!(
        parsed.files,
        Some(vec![
            File {
                uri: Some(
                    "https://arweave.net/_a4sXT6fOHI-5VHFOHLEF73wqKuZtJgE518Ciq9DGyI?ext=gif"
                        .to_string(),
                ),
                cdn_uri: None,
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
            .clone()
            .links
            .unwrap()
            .get("animation_url")
            .unwrap()
            .as_str()
            .unwrap(),
        "https://arweave.net/HVOJ3bTpqMJJJtd5nW2575vPTekLa_SSDsQc7AqV_Ho?ext=mp4"
    );
}