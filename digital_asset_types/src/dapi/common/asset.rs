use crate::dao::token_accounts;
use crate::dao::FullAsset;
use crate::dao::PageOptions;
use crate::dao::Pagination;
use crate::dao::{asset, asset_authority, asset_creators, asset_data, asset_grouping};
use crate::rpc::filter::{AssetSortBy, AssetSortDirection, AssetSorting};
use crate::rpc::options::Options;
use crate::rpc::response::TokenAccountList;
use crate::rpc::response::TransactionSignatureList;
use crate::rpc::response::{AssetList, DasError};
use crate::rpc::TokenInfo;
use crate::rpc::TokenInscriptionInfo;
use crate::rpc::{
    Asset as RpcAsset, Authority, Compression, Content, Creator, File, Group, Interface,
    MetadataMap, MplCoreInfo, Ownership, Royalty, Scope, Supply, TokenAccount as RpcTokenAccount,
    Uses,
};
use blockbuster::programs::token_inscriptions::InscriptionData;
use jsonpath_lib::JsonPathError;
use log::warn;
use mime_guess::Mime;

use sea_orm::DbErr;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use url::Url;

pub fn to_uri(uri: String) -> Option<Url> {
    Url::parse(&uri).ok()
}

pub fn get_mime(url: Url) -> Option<Mime> {
    mime_guess::from_path(Path::new(url.path())).first()
}

pub fn get_mime_type_from_uri(uri: String) -> String {
    let default_mime_type = "image/png".to_string();
    to_uri(uri)
        .and_then(get_mime)
        .map_or(default_mime_type, |m| m.to_string())
}

pub fn file_from_str(str: String) -> File {
    let mime = get_mime_type_from_uri(str.clone());
    File {
        uri: Some(str),
        mime: Some(mime),
        quality: None,
        contexts: None,
    }
}

pub fn build_asset_response(
    assets: Vec<FullAsset>,
    limit: u64,
    pagination: &Pagination,
    options: &Options,
) -> AssetList {
    let total = assets.len() as u32;
    let (page, before, after, cursor) = match pagination {
        Pagination::Keyset { before, after } => {
            let bef = before.clone().and_then(|x| String::from_utf8(x).ok());
            let aft = after.clone().and_then(|x| String::from_utf8(x).ok());
            (None, bef, aft, None)
        }
        Pagination::Page { page } => (Some(*page), None, None, None),
        Pagination::Cursor(_) => {
            if let Some(last_asset) = assets.last() {
                let cursor_str = bs58::encode(&last_asset.asset.id.clone()).into_string();
                (None, None, None, Some(cursor_str))
            } else {
                (None, None, None, None)
            }
        }
    };

    let (items, errors) = asset_list_to_rpc(assets, options);
    AssetList {
        total,
        limit: limit as u32,
        page: page.map(|x| x as u32),
        before,
        after,
        items,
        errors,
        cursor,
    }
}

pub fn build_transaction_signatures_response(
    items: Vec<(String, String)>,
    limit: u64,
    pagination: &Pagination,
) -> TransactionSignatureList {
    let total = items.len() as u32;
    let (page, before, after) = match pagination {
        Pagination::Keyset { before, after } => {
            let bef = before.clone().and_then(|x| String::from_utf8(x).ok());
            let aft = after.clone().and_then(|x| String::from_utf8(x).ok());
            (None, bef, aft)
        }
        Pagination::Page { page } => (Some(*page), None, None),
        Pagination::Cursor { .. } => (None, None, None),
    };
    TransactionSignatureList {
        total,
        limit: limit as u32,
        page: page.map(|x| x as u32),
        before,
        after,
        items,
    }
}

pub fn create_sorting(sorting: AssetSorting) -> (sea_orm::query::Order, Option<asset::Column>) {
    let sort_column = match sorting.sort_by {
        AssetSortBy::Id => Some(asset::Column::Id),
        AssetSortBy::Created => Some(asset::Column::CreatedAt),
        AssetSortBy::Updated => Some(asset::Column::SlotUpdated),
        AssetSortBy::RecentAction => Some(asset::Column::SlotUpdated),
        AssetSortBy::None => None,
    };
    let sort_direction = match sorting.sort_direction.unwrap_or_default() {
        AssetSortDirection::Desc => sea_orm::query::Order::Desc,
        AssetSortDirection::Asc => sea_orm::query::Order::Asc,
    };
    (sort_direction, sort_column)
}

pub fn create_pagination(page_options: &PageOptions) -> Result<Pagination, DbErr> {
    if let Some(cursor) = &page_options.cursor {
        Ok(Pagination::Cursor(cursor.clone()))
    } else {
        match (
            page_options.before.as_ref(),
            page_options.after.as_ref(),
            page_options.page,
        ) {
            (_, _, None) => Ok(Pagination::Keyset {
                before: page_options.before.clone(),
                after: page_options.after.clone(),
            }),
            (None, None, Some(p)) => Ok(Pagination::Page { page: p }),
            _ => Err(DbErr::Custom("Invalid Pagination".to_string())),
        }
    }
}

pub fn track_top_level_file(
    file_map: &mut HashMap<String, File>,
    top_level_file: Option<&serde_json::Value>,
) {
    if top_level_file.is_some() {
        let img = top_level_file.and_then(|x| x.as_str());
        if let Some(img) = img {
            let entry = file_map.get(img);
            if entry.is_none() {
                file_map.insert(img.to_string(), file_from_str(img.to_string()));
            }
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
    let json_uri = asset_data.metadata_url.clone();
    let metadata = &asset_data.metadata;
    let mut selector_fn = jsonpath_lib::selector(metadata);
    let mut chain_data_selector_fn = jsonpath_lib::selector(&asset_data.chain_data);
    let selector = &mut selector_fn;
    let chain_data_selector = &mut chain_data_selector_fn;
    let mut meta: MetadataMap = MetadataMap::new();
    let name = safe_select(chain_data_selector, "$.name");
    if let Some(name) = name {
        meta.set_item("name", name.clone());
    }
    let symbol = safe_select(chain_data_selector, "$.symbol");
    if let Some(symbol) = symbol {
        meta.set_item("symbol", symbol.clone());
    }
    let desc = safe_select(selector, "$.description");
    if let Some(desc) = desc {
        meta.set_item("description", desc.clone());
    }
    let symbol = safe_select(selector, "$.attributes");
    if let Some(symbol) = symbol {
        meta.set_item("attributes", symbol.clone());
    }
    let token_standard = safe_select(chain_data_selector, "$.token_standard");
    if let Some(token_standard) = token_standard {
        meta.set_item("token_standard", token_standard.clone());
    }
    let mut links = HashMap::new();
    let link_fields = vec!["image", "animation_url", "external_url"];
    for f in link_fields {
        let l = safe_select(selector, format!("$.{}", f).as_str());
        if let Some(l) = l {
            links.insert(f.to_string(), l.to_owned());
        }
    }
    let _metadata = safe_select(selector, "description");
    let mut actual_files: HashMap<String, File> = HashMap::new();
    if let Some(files) = selector("$.properties.files[*]")
        .ok()
        .filter(|d| !Vec::is_empty(d))
    {
        for v in files.iter() {
            if v.is_object() {
                // Some assets don't follow the standard and specifiy 'url' instead of 'uri'
                let mut uri = v.get("uri");
                if uri.is_none() {
                    uri = v.get("url");
                }
                let mime_type = v.get("type");
                match (uri, mime_type) {
                    (Some(u), Some(m)) => {
                        if let Some(str_uri) = u.as_str() {
                            let file = if let Some(str_mime) = m.as_str() {
                                File {
                                    uri: Some(str_uri.to_string()),
                                    mime: Some(str_mime.to_string()),
                                    quality: None,
                                    contexts: None,
                                }
                            } else {
                                warn!("Mime is not string: {:?}", m);
                                file_from_str(str_uri.to_string())
                            };
                            actual_files.insert(str_uri.to_string().clone(), file);
                        } else {
                            warn!("URI is not string: {:?}", u);
                        }
                    }
                    (Some(u), None) => {
                        let str_uri = serde_json::to_string(u).unwrap_or_else(|_| String::new());
                        actual_files.insert(str_uri.clone(), file_from_str(str_uri));
                    }
                    _ => {}
                }
            } else if v.is_string() {
                let str_uri = v.as_str().unwrap().to_string();
                actual_files.insert(str_uri.clone(), file_from_str(str_uri));
            }
        }
    }

    track_top_level_file(&mut actual_files, links.get("image"));
    track_top_level_file(&mut actual_files, links.get("animation_url"));

    let mut files: Vec<File> = actual_files.into_values().collect();

    // List the defined image file before the other files (if one exists).
    files.sort_by(|a, _: &File| match (a.uri.as_ref(), links.get("image")) {
        (Some(x), Some(y)) => {
            if x == y {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        }
        _ => Ordering::Equal,
    });

    Ok(Content {
        schema: "https://schema.metaplex.com/nft1.0.json".to_string(),
        json_uri,
        files: Some(files),
        metadata: meta,
        links: Some(links),
    })
}

pub fn get_content(data: &asset_data::Model) -> Option<Content> {
    v1_content_from_json(data).ok()
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

pub fn to_grouping(
    groups: Vec<(asset_grouping::Model, Option<asset_data::Model>)>,
    options: &Options,
) -> Result<Vec<Group>, DbErr> {
    let result: Vec<Group> = groups
        .iter()
        .filter_map(|(asset_group, asset_data)| {
            let verified = match options.show_unverified_collections {
                // Null verified indicates legacy data, meaning it is verified.
                true => Some(asset_group.verified),
                false => None,
            };
            // Filter out items where group_value is None.
            asset_group.group_value.clone().map(|group_value| {
                let collection_metadata = asset_data.as_ref().map(|data| {
                    let mut metadata_selector_fn = jsonpath_lib::selector(&data.metadata);
                    let metadata_selector = &mut metadata_selector_fn;
                    let mut meta: MetadataMap = MetadataMap::new();

                    if let Some(name) = safe_select(metadata_selector, "$.name") {
                        meta.set_item("name", name.clone());
                    }
                    if let Some(symbol) = safe_select(metadata_selector, "$.symbol") {
                        meta.set_item("symbol", symbol.clone());
                    }
                    if let Some(image) = safe_select(metadata_selector, "$.image") {
                        meta.set_item("image", image.clone());
                    }
                    if let Some(external_url) = safe_select(metadata_selector, "$.external_url") {
                        meta.set_item("external_url", external_url.clone());
                    }

                    meta
                });

                Group {
                    group_key: asset_group.group_key.clone(),
                    group_value: Some(group_value),
                    verified,
                    collection_metadata,
                }
            })
        })
        .collect();

    Ok(result)
}

pub fn get_interface(asset: &asset::Model) -> Result<Interface, DbErr> {
    Ok(Interface::from((
        asset.specification_version.as_ref(),
        asset
            .specification_asset_class
            .as_ref()
            .ok_or(DbErr::Custom("interface not found".to_string()))?,
    )))
}

//TODO -> impl custom error type
pub fn asset_to_rpc(asset: FullAsset, options: &Options) -> Result<RpcAsset, DbErr> {
    let FullAsset {
        asset,
        data,
        authorities,
        creators,
        groups,
        inscription,
        token_info,
    } = asset;
    let rpc_authorities = to_authority(authorities);
    let rpc_creators = to_creators(creators);
    let rpc_groups = to_grouping(groups, options)?;
    let interface = get_interface(&asset)?;
    let content = get_content(&data);
    let mut chain_data_selector_fn = jsonpath_lib::selector(&data.chain_data);
    let chain_data_selector = &mut chain_data_selector_fn;

    let edition_nonce =
        safe_select(chain_data_selector, "$.edition_nonce").and_then(|v| v.as_u64());
    let basis_points = safe_select(chain_data_selector, "$.primary_sale_happened")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mutable = data.chain_data_mutability.clone().into();
    let uses = data.chain_data.get("uses").map(|u| Uses {
        use_method: u
            .get("use_method")
            .and_then(|s| s.as_str())
            .unwrap_or("Single")
            .to_string()
            .into(),
        total: u.get("total").and_then(|t| t.as_u64()).unwrap_or(0),
        remaining: u.get("remaining").and_then(|t| t.as_u64()).unwrap_or(0),
    });

    let mpl_core_info = match interface {
        Interface::MplCoreAsset | Interface::MplCoreCollection => Some(MplCoreInfo {
            num_minted: asset.mpl_core_collection_num_minted,
            current_size: asset.mpl_core_collection_current_size,
            plugins_json_version: asset.mpl_core_plugins_json_version,
        }),
        _ => None,
    };

    let inscription = if options.show_inscription {
        inscription
            .and_then(|i| {
                i.data.map(|d| -> Result<TokenInscriptionInfo, DbErr> {
                    let deserialized_data: InscriptionData =
                        serde_json::from_value(d).map_err(|e| {
                            DbErr::Custom(format!("Failed to deserialize inscription data: {}", e))
                        })?;
                    Ok(TokenInscriptionInfo {
                        authority: deserialized_data.authority,
                        root: deserialized_data.root,
                        content: deserialized_data.content,
                        encoding: deserialized_data.encoding,
                        inscription_data: deserialized_data.inscription_data,
                        order: deserialized_data.order,
                        size: deserialized_data.size,
                        validation_hash: deserialized_data.validation_hash,
                    })
                })
            })
            .and_then(|i| i.ok())
    } else {
        None
    };

    let token_info = if options.show_fungible {
        token_info.map(|token_info| TokenInfo {
            supply: token_info.supply.try_into().unwrap_or(0),
            decimals: token_info.decimals as u8,
            mint_authority: token_info
                .mint_authority
                .map(|s| bs58::encode(s).into_string()),
            freeze_authority: token_info
                .freeze_authority
                .map(|s| bs58::encode(s).into_string()),
            token_program: bs58::encode(token_info.token_program).into_string(),
        })
    } else {
        None
    };

    Ok(RpcAsset {
        interface: interface.clone(),
        id: bs58::encode(asset.id).into_string(),
        content,
        authorities: Some(rpc_authorities),
        mutable,
        compression: Some(Compression {
            eligible: asset.compressible,
            compressed: asset.compressed,
            leaf_id: asset.nonce.unwrap_or(0),
            seq: asset.seq.unwrap_or(0),
            tree: asset
                .tree_id
                .map(|s| bs58::encode(s).into_string())
                .unwrap_or_default(),
            asset_hash: asset
                .leaf
                .map(|s| bs58::encode(s).into_string())
                .unwrap_or_default(),
            data_hash: asset
                .data_hash
                .map(|e| if asset.compressed { e.trim() } else { "" }.to_string())
                .unwrap_or_default(),
            creator_hash: asset
                .creator_hash
                .map(|e| if asset.compressed { e.trim() } else { "" }.to_string())
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
        ownership: Some(Ownership {
            frozen: asset.frozen,
            delegated: asset.delegate.is_some(),
            delegate: asset.delegate.map(|s| bs58::encode(s).into_string()),
            ownership_model: asset.owner_type.into(),
            owner: asset
                .owner
                .map(|o| bs58::encode(o).into_string())
                .unwrap_or("".to_string()),
        }),
        supply: match interface {
            Interface::V1NFT
            | Interface::LEGACY_NFT
            | Interface::Nft
            | Interface::ProgrammableNFT
            | Interface::Custom => Some(Supply {
                edition_nonce,
                print_current_supply: 0,
                print_max_supply: 0,
            }),
            _ => None,
        },
        uses,
        burnt: asset.burnt,
        token_info,
        mint_extensions: asset.mint_extensions,
        inscription,
        plugins: asset.mpl_core_plugins,
        unknown_plugins: asset.mpl_core_unknown_plugins,
        mpl_core_info,
        external_plugins: asset.mpl_core_external_plugins,
        unknown_external_plugins: asset.mpl_core_unknown_external_plugins,
    })
}

pub fn asset_list_to_rpc(
    asset_list: Vec<FullAsset>,
    options: &Options,
) -> (Vec<RpcAsset>, Vec<DasError>) {
    asset_list
        .into_iter()
        .fold((vec![], vec![]), |(mut assets, mut errors), asset| {
            let id = bs58::encode(asset.asset.id.clone()).into_string();
            match asset_to_rpc(asset, options) {
                Ok(rpc_asset) => assets.push(rpc_asset),
                Err(e) => errors.push(DasError {
                    id,
                    error: e.to_string(),
                }),
            }
            (assets, errors)
        })
}

pub fn token_account_to_rpc(
    token_account: token_accounts::Model,
    _options: &Options,
) -> Result<RpcTokenAccount, DbErr> {
    let address = bs58::encode(token_account.pubkey.clone()).into_string();
    let mint = bs58::encode(token_account.mint.clone()).into_string();
    let owner = bs58::encode(token_account.owner.clone()).into_string();
    let delegate = token_account
        .delegate
        .map(|d| bs58::encode(d).into_string());
    let close_authority = token_account
        .close_authority
        .map(|d| bs58::encode(d).into_string());

    Ok(RpcTokenAccount {
        address,
        mint,
        amount: token_account.amount as u64,
        owner,
        frozen: token_account.frozen,
        delegate,
        delegated_amount: token_account.delegated_amount as u64,
        close_authority,
        extensions: None,
    })
}

pub fn token_account_list_to_rpc(
    token_accounts: Vec<token_accounts::Model>,
    options: &Options,
) -> (Vec<RpcTokenAccount>, Vec<DasError>) {
    token_accounts.into_iter().fold(
        (vec![], vec![]),
        |(mut accounts, mut errors), token_account| {
            let id = bs58::encode(token_account.pubkey.clone()).into_string();
            match token_account_to_rpc(token_account, options) {
                Ok(rpc_token_account) => accounts.push(rpc_token_account),
                Err(e) => errors.push(DasError {
                    id,
                    error: e.to_string(),
                }),
            }
            (accounts, errors)
        },
    )
}

pub fn build_token_list_response(
    token_accounts: Vec<token_accounts::Model>,
    limit: u64,
    pagination: &Pagination,
    options: &Options,
) -> TokenAccountList {
    let total = token_accounts.len() as u32;
    let (page, before, after, cursor) = match pagination {
        Pagination::Keyset { before, after } => {
            let bef = before.clone().and_then(|x| String::from_utf8(x).ok());
            let aft = after.clone().and_then(|x| String::from_utf8(x).ok());
            (None, bef, aft, None)
        }
        Pagination::Page { page } => (Some(*page as u32), None, None, None),
        Pagination::Cursor(_) => {
            if let Some(last_token_account) = token_accounts.last() {
                let cursor_str = bs58::encode(&last_token_account.pubkey.clone()).into_string();
                (None, None, None, Some(cursor_str))
            } else {
                (None, None, None, None)
            }
        }
    };

    let (items, errors) = token_account_list_to_rpc(token_accounts, options);
    TokenAccountList {
        total,
        limit: limit as u32,
        page,
        before,
        after,
        token_accounts: items,
        cursor,
        errors,
    }
}
