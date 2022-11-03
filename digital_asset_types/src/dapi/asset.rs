
use crate::dao::prelude::{Asset, AssetData};
use crate::dao::sea_orm_active_enums::{SpecificationAssetClass, SpecificationVersions};
use crate::dao::{asset, asset_authority, asset_creators, asset_data, asset_grouping};
use crate::dao::{FullAsset, FullAssetList};




use crate::rpc::{Asset as RpcAsset, Authority, Compression, Content, Creator, File, Group, Interface, MetadataItem, Ownership, Royalty, Scope, Supply, Uses};
use jsonpath_lib::JsonPathError;
use mime_guess::Mime;
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use url::Url;


pub fn to_uri(uri: String) -> Option<Url> {
    Url::parse(&*uri).ok()
}

pub fn get_mime(url: Url) -> Option<Mime> {
    mime_guess::from_path(Path::new(url.path())).first()
}

pub fn get_mime_type_from_uri(uri: String) -> Option<String> {
    to_uri(uri).and_then(get_mime).map(|m| m.to_string())
}

pub fn file_from_str(str: String) -> File {
    let mime = get_mime_type_from_uri(str.clone());
    File {
        uri: Some(str),
        mime,
        quality: None,
        contexts: None,
    }
}

pub fn track_top_level_file(
    file_map: &mut HashMap<String, File>,
    top_level_file: Option<&serde_json::Value>,
) {
    if top_level_file.is_some() {
        let img = top_level_file.and_then(|x| x.as_str()).unwrap();
        let entry = file_map.get(img);
        if entry.is_none() {
            file_map.insert(img.to_string(), file_from_str(img.to_string()));
        }
    }
}

pub fn safe_select<'a>(
    selector: &mut impl FnMut(&str) -> Result<Vec<&'a Value>, JsonPathError>,
    expr: &str,
) -> Option<&'a Value> {
    selector(expr)
        .ok()
        .filter(|d| !Vec::is_empty(d))
        .as_mut()
        .and_then(|v| v.pop())
}

pub fn v1_content_from_json(asset_data: &asset_data::Model) -> Result<Content, DbErr> {
    // todo -> move this to the bg worker for pre processing
    let metadata = &asset_data.metadata;
    let mut selector_fn = jsonpath_lib::selector(metadata);
    let mut chain_data_selector_fn = jsonpath_lib::selector(&asset_data.chain_data);
    let selector = &mut selector_fn;
    let chain_data_selector = &mut chain_data_selector_fn;
    println!("{}", metadata.to_string());
    let mut meta: Vec<MetadataItem> = Vec::new();
    let mut description_meta = MetadataItem::new("description");
    let _image = safe_select(selector, "$.image");
    let name = safe_select(chain_data_selector, "$.name");
    if let Some(name) = name {
        description_meta.set_item("name", name.clone());
    }
    let desc = safe_select(selector, "$.description");
    if let Some(desc) = desc {
        description_meta.set_item("description", desc.clone());
    }
    meta.push(description_meta);
    let _description_meta = MetadataItem::new("description");
    let symbol = safe_select(chain_data_selector, "$.symbol")
        .map(|x| MetadataItem::single("symbol", "symbol", x.clone()));
    if let Some(symbol) = symbol {
        meta.push(symbol);
    }
    let image = safe_select(selector, "$.image");
    let animation = safe_select(selector, "$.animation_url");
    let external_url = safe_select(selector, "$.external_url").map(|val| {
        let mut links = HashMap::new();
        links.insert("external_url".to_string(), val[0].to_owned());
        links
    });
    let _metadata = safe_select(selector, "description");
    let mut actual_files: HashMap<String, File> = HashMap::new();
    selector("$.properties.files[*]")
        .ok()
        .filter(|d| !Vec::is_empty(d))
        .map(|files| {
            for v in files.iter() {
                if v.is_object() {
                    let uri = v.get("uri");
                    let mime_type = v.get("type");
                    match (uri, mime_type) {
                        (Some(u), Some(m)) => {
                            let str_uri = u.as_str().unwrap().to_string();
                            let str_mime = m.as_str().unwrap().to_string();
                            actual_files.insert(
                                str_uri.clone(),
                                File {
                                    uri: Some(str_uri),
                                    mime: Some(str_mime),
                                    quality: None,
                                    contexts: None,
                                },
                            );
                        }
                        (Some(u), None) => {
                            let str_uri = serde_json::to_string(u).unwrap();
                            actual_files.insert(str_uri.clone(), file_from_str(str_uri));
                        }
                        _ => {}
                    }
                } else if v.is_string() {
                    let str_uri = v.as_str().unwrap().to_string();
                    actual_files.insert(str_uri.clone(), file_from_str(str_uri));
                }
            }
        });

    track_top_level_file(&mut actual_files, image);
    track_top_level_file(&mut actual_files, animation);
    let files: Vec<File> = actual_files.into_values().collect();

    Ok(Content {
        schema: "https://schema.metaplex.com/nft1.0.json".to_string(),
        files: Some(files),
        metadata: Some(meta),
        links: external_url,
    })
}

pub fn get_content(asset: &asset::Model, data: &asset_data::Model) -> Result<Content, DbErr> {
    match asset.specification_version {
        SpecificationVersions::V1 => v1_content_from_json(data),
        SpecificationVersions::V0 => v1_content_from_json(data),
        _ => Err(DbErr::Custom("Version Not Implemented".to_string())),
    }
}

pub fn to_authority(authority: Vec<asset_authority::Model>) -> Vec<Authority> {
    authority
        .iter()
        .map(|a| Authority {
            address: bs58::encode(&a.authority).into_string(),
            scopes: vec![Scope::Full],
        })
        .collect()
}

pub fn to_creators(creators: Vec<asset_creators::Model>) -> Vec<Creator> {
    creators
        .iter()
        .map(|a| Creator {
            address: bs58::encode(&a.creator).into_string(),
            share: a.share,
            verified: a.verified,
        })
        .collect()
}

pub fn to_grouping(groups: Vec<asset_grouping::Model>) -> Vec<Group> {
    groups
        .iter()
        .map(|a| Group {
            group_key: a.group_key.clone(),
            group_value: a.group_value.clone(),
        })
        .collect()
}

pub fn get_interface(asset: &asset::Model) -> Interface {
    match (
        &asset.specification_version,
        &asset.specification_asset_class,
    ) {
        (SpecificationVersions::V1, SpecificationAssetClass::Nft) => Interface::V1NFT,
        (SpecificationVersions::V1, SpecificationAssetClass::PrintableNft) => Interface::V1NFT,
        (SpecificationVersions::V0, SpecificationAssetClass::Nft) => Interface::LEGACY_NFT,
        _ => Interface::V1NFT,
    }
}

//TODO -> impl custom erro type
pub fn asset_to_rpc(asset: FullAsset) -> Result<RpcAsset, DbErr> {
    let FullAsset {
        asset,
        data,
        authorities,
        creators,
        groups,
    } = asset;
    let rpc_authorities = to_authority(authorities);
    let rpc_creators = to_creators(creators);
    let rpc_groups = to_grouping(groups);
    let interface = get_interface(&asset);
    let content = get_content(&asset, &data)?;
    let mut chain_data_selector_fn = jsonpath_lib::selector(&data.chain_data);
    let chain_data_selector = &mut chain_data_selector_fn;
    let basis_points = safe_select(chain_data_selector, "$.primary_sale_happened")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let edition_nonce = safe_select(chain_data_selector, "$.edition_nonce")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Ok(RpcAsset {
        interface: interface.clone(),
        id: bs58::encode(asset.id).into_string(),
        content: Some(content),
        authorities: Some(rpc_authorities),
        mutability: data.chain_data_mutability.into(),
        compression: Some(Compression {
            eligible: asset.compressible,
            compressed: asset.compressed,
            asset_hash: asset
                .leaf
                .map(|s| bs58::encode(s).into_string())
                .unwrap_or_default(),
            data_hash: asset
                .data_hash
                .map(|e| e.trim().to_string())
                .unwrap_or_default(),
            creator_hash: asset
                .creator_hash
                .map(|e| e.trim().to_string())
                .unwrap_or_default(),
        }),
        grouping: Some(rpc_groups),
        royalty: Some(Royalty {
            royalty_model: asset.royalty_target_type.into(),
            target: asset.royalty_target.map(|s| bs58::encode(s).into_string()),
            percent: (asset.royalty_amount as f64) * 0.0001,
            basis_points: asset.royalty_amount as u32,
            primary_sale_happened: basis_points,
            locked: false,
        }),
        creators: Some(rpc_creators),
        ownership: Ownership {
            frozen: asset.frozen,
            delegated: asset.delegate.is_some(),
            delegate: asset.delegate.map(|s| bs58::encode(s).into_string()),
            ownership_model: asset.owner_type.into(),
            owner: asset
                .owner
                .map(|o| bs58::encode(o).into_string())
                .unwrap_or("".to_string()),
        },
        supply: match interface {
            Interface::V1NFT => Some(
                Supply{
                    edition_nonce: edition_nonce,
                    print_current_supply: 0,
                    print_max_supply: 0
                }
            ),
            _ => None
        },
        uses: data.chain_data.get("uses").map(|u| Uses {
            use_method: u
                .get("use_method")
                .and_then(|s| s.as_str())
                .unwrap_or("Single")
                .to_string()
                .into(),
            total: u.get("total").and_then(|t| t.as_u64()).unwrap_or(0),
            remaining: u.get("remaining").and_then(|t| t.as_u64()).unwrap_or(0),
        }),
    })
}

pub fn asset_list_to_rpc(asset_list: FullAssetList) -> Vec<Result<RpcAsset, DbErr>> {
    asset_list.list.into_iter().map(asset_to_rpc).collect()
}

pub async fn get_asset(db: &DatabaseConnection, asset_id: Vec<u8>) -> Result<RpcAsset, DbErr> {
    let asset_data: (asset::Model, asset_data::Model) = Asset::find_by_id(asset_id)
        .find_also_related(AssetData)
        .one(db)
        .await
        .and_then(|o| match o {
            Some((a, Some(d))) => Ok((a, d)),
            _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
        })?;

    let (asset, data) = asset_data;
    let authorities: Vec<asset_authority::Model> = asset_authority::Entity::find()
        .filter(asset_authority::Column::AssetId.eq(asset.id.clone()))
        .all(db)
        .await?;
    let creators: Vec<asset_creators::Model> = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.eq(asset.id.clone()))
        .all(db)
        .await?;
    let grouping: Vec<asset_grouping::Model> = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.eq(asset.id.clone()))
        .all(db)
        .await?;
    asset_to_rpc(FullAsset {
        asset,
        data,
        authorities,
        creators,
        groups: grouping,
    })
}

pub async fn get_asset_list_data(
    db: &DatabaseConnection,
    assets: Vec<(asset::Model, Option<asset_data::Model>)>,
) -> Result<Vec<RpcAsset>, DbErr> {
    let mut ids = Vec::with_capacity(assets.len());
    let mut assets_map = assets.into_iter().fold(HashMap::new(), |mut x, asset| {
        if let Some(ad) = asset.1 {
            let id = asset.0.id.clone();
            let fa = FullAsset {
                asset: asset.0,
                data: ad,
                authorities: vec![],
                creators: vec![],
                groups: vec![],
            };

            x.insert(id.clone(), fa);
            ids.push(id);
        }
        x
    });

    let authorities = asset_authority::Entity::find()
        .filter(asset_authority::Column::AssetId.is_in(ids.clone()))
        .order_by_asc(asset_authority::Column::AssetId)
        .all(db)
        .await?;
    for a in authorities.into_iter() {
        if let Some(asset) = assets_map.get_mut(&a.asset_id) {
            asset.authorities.push(a);
        }
    }

    let creators = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.is_in(ids.clone()))
        .order_by_asc(asset_creators::Column::AssetId)
        .all(db)
        .await?;
    for c in creators.into_iter() {
        if let Some(asset) = assets_map.get_mut(&c.asset_id) {
            asset.creators.push(c);
        }
    }

    let grouping = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.is_in(ids.clone()))
        .order_by_asc(asset_grouping::Column::AssetId)
        .all(db)
        .await?;
    for g in grouping.into_iter() {
        if let Some(asset) = assets_map.get_mut(&g.asset_id) {
            asset.groups.push(g);
        }
    }
    let len = assets_map.len();
    let built_assets = asset_list_to_rpc(FullAssetList {
        list: assets_map.into_iter().map(|(_, v)| v).collect(),
    })
        .into_iter()
        .fold(Vec::with_capacity(len), |mut acc, i| {
            if let Ok(a) = i {
                acc.push(a);
            }
            acc
        });
    Ok(built_assets)
}

