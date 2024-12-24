use {
    crate::{
        asset_upserts::{
            upsert_assets_metadata_account_columns, upsert_assets_mint_account_columns,
            upsert_assets_token_account_columns, AssetMetadataAccountColumns,
            AssetMintAccountColumns, AssetTokenAccountColumns,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
        find_model_with_retry, DownloadMetadataInfo,
    },
    blockbuster::{
        mpl_core::types::{Plugin, PluginAuthority, PluginType, UpdateAuthority},
        programs::mpl_core_program::MplCoreAccountData,
    },
    digital_asset_types::{
        dao::{
            asset, asset_authority, asset_creators, asset_data, asset_grouping,
            sea_orm_active_enums::{
                ChainMutability, Mutability, OwnerType, SpecificationAssetClass,
            },
        },
        json::ChainDataV1,
    },
    heck::ToSnakeCase,
    sea_orm::{
        entity::{ActiveValue, ColumnTrait, EntityTrait},
        prelude::*,
        query::{JsonValue, QueryFilter},
        sea_query::{query::OnConflict, Alias, Condition, Expr},
        ConnectionTrait, CursorTrait, Statement, TransactionTrait,
    },
    serde_json::{value::Value, Map},
    solana_sdk::pubkey::Pubkey,
    tracing::warn,
};

pub async fn burn_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    id: Pubkey,
    slot: u64,
) -> ProgramTransformerResult<()> {
    let slot_i = slot as i64;
    let model = asset::ActiveModel {
        id: ActiveValue::Set(id.to_bytes().to_vec()),
        slot_updated: ActiveValue::Set(Some(slot_i)),
        burnt: ActiveValue::Set(true),
        ..Default::default()
    };
    asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([asset::Column::SlotUpdated, asset::Column::Burnt])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Expr::tbl(Alias::new("excluded"), asset::Column::Burnt)
                                .ne(Expr::tbl(asset::Entity, asset::Column::Burnt)),
                        )
                        .add(Expr::tbl(asset::Entity, asset::Column::SlotUpdated).lte(slot_i)),
                )
                .to_owned(),
        )
        .exec_without_returning(conn)
        .await?;
    Ok(())
}

const RETRY_INTERVALS: &[u64] = &[0, 5, 10];

pub async fn save_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    id: Pubkey,
    account_data: &MplCoreAccountData,
    slot: u64,
) -> ProgramTransformerResult<Option<DownloadMetadataInfo>> {
    // Notes:
    // The address of the Core asset is used for Core Asset ID.  There are no token or mint accounts.
    // There are no `MasterEdition` or `Edition` accounts associated with Core assets.
    let id_array = id.to_bytes();
    let id_vec = id_array.to_vec();

    // Note: This indexes both Core Assets and Core Collections.
    let asset = match account_data {
        MplCoreAccountData::Asset(indexable_asset)
        | MplCoreAccountData::Collection(indexable_asset) => indexable_asset,
        _ => return Err(ProgramTransformerError::NotImplemented),
    };

    println!("asset: {:?}", asset);

    //-----------------------
    // Asset authority table
    //-----------------------

    // If it is an `Address` type, use the value directly.  If it is a `Collection`, search for and
    // use the collection's authority.
    let update_authority = match asset.update_authority {
        UpdateAuthority::Address(address) => address.to_bytes().to_vec(),
        UpdateAuthority::Collection(address) => find_model_with_retry(
            conn,
            "mpl_core",
            &asset_authority::Entity::find()
                .filter(asset_authority::Column::AssetId.eq(address.to_bytes().to_vec())),
            RETRY_INTERVALS,
        )
        .await?
        .map(|model| model.authority)
        .unwrap_or_default(),
        UpdateAuthority::None => Pubkey::default().to_bytes().to_vec(),
    };

    let slot_i = slot as i64;

    let txn = conn.begin().await?;

    let set_lock_timeout = "SET LOCAL lock_timeout = '1s';";
    let set_local_app_name =
        "SET LOCAL application_name = 'das::program_transformers::mpl_core_program::v1_asset';";
    let set_lock_timeout_stmt =
        Statement::from_string(txn.get_database_backend(), set_lock_timeout.to_string());
    let set_local_app_name_stmt =
        Statement::from_string(txn.get_database_backend(), set_local_app_name.to_string());
    txn.execute(set_lock_timeout_stmt).await?;
    txn.execute(set_local_app_name_stmt).await?;

    let model = asset_authority::ActiveModel {
        asset_id: ActiveValue::Set(id_vec.clone()),
        authority: ActiveValue::Set(update_authority.clone()),
        seq: ActiveValue::Set(0),
        slot_updated: ActiveValue::Set(slot_i),
        ..Default::default()
    };

    asset_authority::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset_authority::Column::AssetId)
                .update_columns([
                    asset_authority::Column::Authority,
                    asset_authority::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Expr::tbl(Alias::new("excluded"), asset_authority::Column::Authority)
                                .ne(Expr::tbl(
                                    asset_authority::Entity,
                                    asset_authority::Column::Authority,
                                )),
                        )
                        .add(
                            Expr::tbl(
                                asset_authority::Entity,
                                asset_authority::Column::SlotUpdated,
                            )
                            .lte(slot_i),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(&txn)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    if matches!(account_data, MplCoreAccountData::Collection(_)) {
        update_group_asset_authorities(conn, id_vec.clone(), update_authority.clone(), slot_i)
            .await?;
    }

    //-----------------------
    // asset_data table
    //-----------------------

    let name = asset.name.clone().into_bytes();
    let uri = asset.uri.trim().replace('\0', "");

    // Notes:
    // There is no symbol for a Core asset.
    // Edition nonce hardcoded to `None`.
    // There is no primary sale concept for Core Assets, hardcoded to `false`.
    // Token standard is hardcoded to `None`.
    let mut chain_data = ChainDataV1 {
        name: asset.name.clone(),
        symbol: "".to_string(),
        edition_nonce: None,
        primary_sale_happened: false,
        token_standard: None,
        uses: None,
    };

    chain_data.sanitize();
    let chain_data_json = serde_json::to_value(chain_data)
        .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;

    // Note:
    // Mutability set based on core asset data having an update authority.
    // Individual plugins could have some or no authority giving them individual mutability status.
    let chain_mutability = match asset.update_authority {
        UpdateAuthority::None => ChainMutability::Immutable,
        _ => ChainMutability::Mutable,
    };

    let asset_data_model = asset_data::ActiveModel {
        chain_data_mutability: ActiveValue::Set(chain_mutability),
        chain_data: ActiveValue::Set(chain_data_json),
        metadata_url: ActiveValue::Set(uri.clone()),
        metadata: ActiveValue::Set(JsonValue::String("processing".to_string())),
        metadata_mutability: ActiveValue::Set(Mutability::Mutable),
        slot_updated: ActiveValue::Set(slot_i),
        reindex: ActiveValue::Set(Some(true)),
        id: ActiveValue::Set(id_vec.clone()),
        raw_name: ActiveValue::Set(Some(name.to_vec())),
        raw_symbol: ActiveValue::Set(None),
        base_info_seq: ActiveValue::Set(Some(0)),
    };
    asset_data::Entity::insert(asset_data_model)
        .on_conflict(
            OnConflict::columns([asset_data::Column::Id])
                .update_columns([
                    asset_data::Column::ChainDataMutability,
                    asset_data::Column::ChainData,
                    asset_data::Column::MetadataUrl,
                    asset_data::Column::MetadataMutability,
                    asset_data::Column::SlotUpdated,
                    asset_data::Column::Reindex,
                    asset_data::Column::RawName,
                    asset_data::Column::RawSymbol,
                    asset_data::Column::BaseInfoSeq,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::ChainDataMutability,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::ChainDataMutability,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::ChainData,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::ChainData,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::MetadataUrl,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::MetadataUrl,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::MetadataMutability,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::MetadataMutability,
                                    )),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset_data::Column::Reindex)
                                        .ne(Expr::tbl(
                                            asset_data::Entity,
                                            asset_data::Column::Reindex,
                                        )),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset_data::Column::RawName)
                                        .ne(Expr::tbl(
                                            asset_data::Entity,
                                            asset_data::Column::RawName,
                                        )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::RawSymbol,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::RawSymbol,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::BaseInfoSeq,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::BaseInfoSeq,
                                    )),
                                ),
                        )
                        .add(
                            Expr::tbl(asset_data::Entity, asset_data::Column::SlotUpdated)
                                .lte(slot_i),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(&txn)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    //-----------------------
    // asset table
    //-----------------------

    let ownership_type = OwnerType::Single;
    let (owner, class) = match account_data {
        MplCoreAccountData::Asset(_) => (
            asset.owner.map(|owner| owner.to_bytes().to_vec()),
            SpecificationAssetClass::MplCoreAsset,
        ),
        MplCoreAccountData::Collection(_) => (
            Some(update_authority.clone()),
            SpecificationAssetClass::MplCoreCollection,
        ),
        _ => return Err(ProgramTransformerError::NotImplemented),
    };

    // Get royalty amount and creators from `Royalties` plugin if available.
    let default_creators = Vec::new();
    let (royalty_amount, creators) = asset
        .plugins
        .get(&PluginType::Royalties)
        .and_then(|plugin_schema| {
            if let Plugin::Royalties(royalties) = &plugin_schema.data {
                Some((royalties.basis_points, &royalties.creators))
            } else {
                None
            }
        })
        .unwrap_or((0, &default_creators));

    // Serialize known plugins into JSON.
    let mut plugins_json = serde_json::to_value(&asset.plugins)
        .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;

    // Improve JSON output.
    remove_plugins_nesting(&mut plugins_json, "data");
    transform_plugins_authority(&mut plugins_json);
    convert_keys_to_snake_case(&mut plugins_json);

    // Serialize any unknown plugins into JSON.
    let unknown_plugins_json = if !asset.unknown_plugins.is_empty() {
        let mut unknown_plugins_json = serde_json::to_value(&asset.unknown_plugins)
            .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;

        // Improve JSON output.
        transform_plugins_authority(&mut unknown_plugins_json);
        convert_keys_to_snake_case(&mut unknown_plugins_json);

        Some(unknown_plugins_json)
    } else {
        None
    };

    // Serialize known external plugins into JSON.
    let mut external_plugins_json = serde_json::to_value(&asset.external_plugins)
        .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;

    // Improve JSON output.
    remove_plugins_nesting(&mut external_plugins_json, "adapter_config");
    transform_plugins_authority(&mut external_plugins_json);
    convert_keys_to_snake_case(&mut external_plugins_json);

    // Serialize any unknown external plugins into JSON.
    let unknown_external_plugins_json = if !asset.unknown_external_plugins.is_empty() {
        let mut unknown_external_plugins_json =
            serde_json::to_value(&asset.unknown_external_plugins)
                .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;

        // Improve JSON output.
        transform_plugins_authority(&mut unknown_external_plugins_json);
        convert_keys_to_snake_case(&mut unknown_external_plugins_json);

        Some(unknown_external_plugins_json)
    } else {
        None
    };

    upsert_assets_metadata_account_columns(
        AssetMetadataAccountColumns {
            mint: id_vec.clone(),
            owner_type: ownership_type.clone(),
            specification_asset_class: Some(class.clone()),
            royalty_amount: royalty_amount as i32,
            asset_data: Some(id_vec.clone()),
            slot_updated_metadata_account: slot,
            mpl_core_plugins: Some(plugins_json.clone()),
            mpl_core_unknown_plugins: unknown_plugins_json.clone(),
            mpl_core_collection_num_minted: asset.num_minted.map(|val| val as i32),
            mpl_core_collection_current_size: asset.current_size.map(|val| val as i32),
            mpl_core_plugins_json_version: Some(1),
            mpl_core_external_plugins: Some(external_plugins_json.clone()),
            mpl_core_unknown_external_plugins: unknown_external_plugins_json.clone(),
        },
        &txn,
    )
    .await?;

    let supply = Decimal::from(1);

    // Note: these need to be separate for Token Metadata but here could be one upsert.
    upsert_assets_mint_account_columns(
        AssetMintAccountColumns {
            mint: id_vec.clone(),
            supply,
            slot_updated_mint_account: slot as i64,
            extensions: None,
        },
        &txn,
    )
    .await?;

    // Get transfer delegate from `TransferDelegate` plugin if available.
    let transfer_delegate =
        asset
            .plugins
            .get(&PluginType::TransferDelegate)
            .and_then(|plugin_schema| match &plugin_schema.authority {
                PluginAuthority::Owner => owner.clone(),
                PluginAuthority::UpdateAuthority => Some(update_authority.clone()),
                PluginAuthority::Address { address } => Some(address.to_bytes().to_vec()),
                PluginAuthority::None => None,
            });

    let frozen = asset
        .plugins
        .get(&PluginType::FreezeDelegate)
        .and_then(|plugin_schema| {
            if let Plugin::FreezeDelegate(freeze_delegate) = &plugin_schema.data {
                Some(freeze_delegate.frozen)
            } else {
                None
            }
        })
        .unwrap_or(false);

    upsert_assets_token_account_columns(
        AssetTokenAccountColumns {
            mint: id_vec.clone(),
            owner,
            frozen,
            // Note use transfer delegate for the existing delegate field.
            delegate: transfer_delegate.clone(),
            slot_updated_token_account: Some(slot_i),
        },
        &txn,
    )
    .await?;
    //-----------------------
    // asset_grouping table
    //-----------------------

    if let UpdateAuthority::Collection(address) = asset.update_authority {
        let model = asset_grouping::ActiveModel {
            asset_id: ActiveValue::Set(id_vec.clone()),
            group_key: ActiveValue::Set("collection".to_string()),
            group_value: ActiveValue::Set(Some(address.to_string())),
            // Note all Core assets in a collection are verified.
            verified: ActiveValue::Set(true),
            group_info_seq: ActiveValue::Set(Some(0)),
            slot_updated: ActiveValue::Set(Some(slot_i)),
            ..Default::default()
        };

        asset_grouping::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    asset_grouping::Column::AssetId,
                    asset_grouping::Column::GroupKey,
                ])
                .update_columns([
                    asset_grouping::Column::GroupValue,
                    asset_grouping::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Expr::tbl(Alias::new("excluded"), asset_grouping::Column::GroupValue)
                                .ne(Expr::tbl(
                                    asset_grouping::Entity,
                                    asset_grouping::Column::GroupValue,
                                )),
                        )
                        .add(
                            Expr::tbl(asset_grouping::Entity, asset_grouping::Column::SlotUpdated)
                                .lte(slot_i),
                        ),
                )
                .to_owned(),
            )
            .exec_without_returning(&txn)
            .await
            .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;
    }

    //-----------------------
    // creators table
    //-----------------------

    let creators = creators
        .iter()
        .enumerate()
        .map(|(i, creator)| asset_creators::ActiveModel {
            asset_id: ActiveValue::Set(id_vec.clone()),
            position: ActiveValue::Set(i as i16),
            creator: ActiveValue::Set(creator.address.to_bytes().to_vec()),
            share: ActiveValue::Set(creator.percentage as i32),
            // Note all creators are verified for Core Assets.
            verified: ActiveValue::Set(true),
            slot_updated: ActiveValue::Set(Some(slot_i)),
            seq: ActiveValue::Set(Some(0)),
            ..Default::default()
        })
        .collect::<Vec<_>>();

    if !creators.is_empty() {
        asset_creators::Entity::insert_many(creators)
            .on_conflict(
                OnConflict::columns([
                    asset_creators::Column::AssetId,
                    asset_creators::Column::Position,
                ])
                .update_columns([
                    asset_creators::Column::Creator,
                    asset_creators::Column::Share,
                    asset_creators::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::any()
                        .add(
                            Condition::all()
                                .add(
                                    Condition::any()
                                        .add(
                                            Expr::tbl(
                                                Alias::new("excluded"),
                                                asset_creators::Column::Creator,
                                            )
                                            .ne(
                                                Expr::tbl(
                                                    asset_creators::Entity,
                                                    asset_creators::Column::Creator,
                                                ),
                                            ),
                                        )
                                        .add(
                                            Expr::tbl(
                                                Alias::new("excluded"),
                                                asset_creators::Column::Share,
                                            )
                                            .ne(
                                                Expr::tbl(
                                                    asset_creators::Entity,
                                                    asset_creators::Column::Share,
                                                ),
                                            ),
                                        ),
                                )
                                .add(
                                    Expr::tbl(
                                        asset_creators::Entity,
                                        asset_creators::Column::SlotUpdated,
                                    )
                                    .lte(slot_i),
                                ),
                        )
                        .add(
                            Expr::tbl(asset_creators::Entity, asset_creators::Column::SlotUpdated)
                                .is_null(),
                        ),
                )
                .to_owned(),
            )
            .exec_without_returning(&txn)
            .await
            .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;
    }

    // Commit the database transaction.
    txn.commit().await?;

    // Return early if there is no URI.
    if uri.is_empty() {
        warn!(
            "URI is empty for mint {}. Skipping background task.",
            bs58::encode(id_vec.clone()).into_string()
        );
        return Ok(None);
    }

    // Otherwise return with info for background downloading.
    Ok(Some(DownloadMetadataInfo::new(id_vec.clone(), uri, slot_i)))
}

// Modify the JSON structure to remove the `Plugin` name and just display its data.
// For example, this will transform `FreezeDelegate` JSON from:
// "data":{"freeze_delegate":{"frozen":false}}}
// to:
// "data":{"frozen":false}
fn remove_plugins_nesting(plugins_json: &mut Value, nested_key: &str) {
    match plugins_json {
        Value::Object(plugins) => {
            // Handle the case where plugins_json is an object.
            for (_, plugin) in plugins.iter_mut() {
                remove_nesting_from_plugin(plugin, nested_key);
            }
        }
        Value::Array(plugins_array) => {
            // Handle the case where plugins_json is an array.
            for plugin in plugins_array.iter_mut() {
                remove_nesting_from_plugin(plugin, nested_key);
            }
        }
        _ => {}
    }
}

fn remove_nesting_from_plugin(plugin: &mut Value, nested_key: &str) {
    if let Some(Value::Object(nested_key)) = plugin.get_mut(nested_key) {
        // Extract the plugin data and remove it.
        if let Some((_, inner_plugin_data)) = nested_key.iter().next() {
            let inner_plugin_data_clone = inner_plugin_data.clone();
            // Clear the `nested_key` object.
            nested_key.clear();
            // Move the plugin data fields to the top level of `nested_key`.
            if let Value::Object(inner_plugin_data) = inner_plugin_data_clone {
                for (field_name, field_value) in inner_plugin_data.iter() {
                    nested_key.insert(field_name.clone(), field_value.clone());
                }
            }
        }
    }
}

// Modify the JSON for `PluginAuthority` to have consistent output no matter the enum type.
// For example, from:
// "authority":{"Address":{"address":"D7whDWAP5gN9x4Ff6T9MyQEkotyzmNWtfYhCEWjbUDBM"}}
// to:
// "authority":{"address":"4dGxsCAwSCopxjEYY7sFShFUkfKC6vzsNEXJDzFYYFXh","type":"Address"}
// and from:
// "authority":"UpdateAuthority"
// to:
// "authority":{"address":null,"type":"UpdateAuthority"}
fn transform_plugins_authority(plugins_json: &mut Value) {
    match plugins_json {
        Value::Object(plugins) => {
            // Transform plugins in an object
            for (_, plugin) in plugins.iter_mut() {
                if let Some(plugin_obj) = plugin.as_object_mut() {
                    transform_authority_in_object(plugin_obj);
                    transform_data_authority_in_object(plugin_obj);
                    transform_linked_app_data_parent_key_in_object(plugin_obj);
                }
            }
        }
        Value::Array(plugins_array) => {
            // Transform plugins in an array
            for plugin in plugins_array.iter_mut() {
                if let Some(plugin_obj) = plugin.as_object_mut() {
                    transform_authority_in_object(plugin_obj);
                    transform_data_authority_in_object(plugin_obj);
                    transform_linked_app_data_parent_key_in_object(plugin_obj);
                }
            }
        }
        _ => {}
    }
}

fn transform_authority_in_object(plugin: &mut Map<String, Value>) {
    if let Some(authority) = plugin.get_mut("authority") {
        transform_authority(authority);
    }
}

fn transform_data_authority_in_object(plugin: &mut Map<String, Value>) {
    if let Some(adapter_config) = plugin.get_mut("adapter_config") {
        if let Some(data_authority) = adapter_config
            .as_object_mut()
            .and_then(|o| o.get_mut("data_authority"))
        {
            transform_authority(data_authority);
        }
    }
}

fn transform_linked_app_data_parent_key_in_object(plugin: &mut Map<String, Value>) {
    if let Some(adapter_config) = plugin.get_mut("adapter_config") {
        if let Some(parent_key) = adapter_config
            .as_object_mut()
            .and_then(|o| o.get_mut("parent_key"))
        {
            if let Some(linked_app_data) = parent_key
                .as_object_mut()
                .and_then(|o| o.get_mut("LinkedAppData"))
            {
                transform_authority(linked_app_data);
            }
        }
    }
}

fn transform_authority(authority: &mut Value) {
    match authority {
        Value::Object(authority_obj) => {
            if let Some(authority_type) = authority_obj.keys().next().cloned() {
                // Replace the nested JSON objects with desired format.
                if let Some(Value::Object(pubkey_obj)) = authority_obj.remove(&authority_type) {
                    if let Some(address_value) = pubkey_obj.get("address") {
                        authority_obj.insert("type".to_string(), Value::from(authority_type));
                        authority_obj.insert("address".to_string(), address_value.clone());
                    }
                }
            }
        }
        Value::String(authority_type) => {
            // Handle the case where authority is a string.
            let mut authority_obj = Map::new();
            authority_obj.insert("type".to_string(), Value::String(authority_type.clone()));
            authority_obj.insert("address".to_string(), Value::Null);
            *authority = Value::Object(authority_obj);
        }
        _ => {}
    }
}

// Convert all keys to snake case.  Ignore values that aren't JSON objects themselves.
fn convert_keys_to_snake_case(plugins_json: &mut Value) {
    match plugins_json {
        Value::Object(obj) => {
            let keys = obj.keys().cloned().collect::<Vec<String>>();
            for key in keys {
                let snake_case_key = key.to_snake_case();
                if let Some(val) = obj.remove(&key) {
                    obj.insert(snake_case_key, val);
                }
            }
            for (_, val) in obj.iter_mut() {
                convert_keys_to_snake_case(val);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                convert_keys_to_snake_case(val);
            }
        }
        _ => {}
    }
}

/// Updates the `asset_authority` for all assets that are part of a collection in a batch.
/// This function performs a cursor-based paginated read and batch update.
async fn update_group_asset_authorities<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    group_value: Vec<u8>,
    authority: Vec<u8>,
    slot: i64,
) -> ProgramTransformerResult<()> {
    let mut after = None;

    let group_key = "collection".to_string();
    let group_value = bs58::encode(group_value).into_string();

    let mut query = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::GroupKey.eq(group_key))
        .filter(asset_grouping::Column::GroupValue.eq(group_value))
        .cursor_by(asset_grouping::Column::AssetId);
    let mut query = query.first(1_000);

    loop {
        if let Some(after) = after.clone() {
            query = query.after(after);
        }

        let entries = query.all(conn).await?;

        if entries.is_empty() {
            break;
        }

        let asset_ids = entries
            .clone()
            .into_iter()
            .map(|entry| entry.asset_id)
            .collect::<Vec<_>>();

        asset_authority::Entity::update_many()
            .col_expr(
                asset_authority::Column::Authority,
                Expr::value(authority.clone()),
            )
            .col_expr(asset_authority::Column::SlotUpdated, Expr::value(slot))
            .filter(asset_authority::Column::AssetId.is_in(asset_ids))
            .filter(asset_authority::Column::Authority.ne(authority.clone()))
            .filter(Expr::cust_with_values(
                "asset_authority.slot_updated < $1",
                vec![slot],
            ))
            .exec(conn)
            .await
            .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

        after = entries.last().map(|entry| entry.asset_id.clone());
    }

    Ok(())
}
