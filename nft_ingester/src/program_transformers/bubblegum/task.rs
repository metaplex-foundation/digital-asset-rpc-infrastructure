use std::fmt::{Display, Formatter};
use async_trait::async_trait;
use digital_asset_types::dao::asset_data;
use crate::{BgTask, IngesterError};
use sea_orm::*;
use digital_asset_types::dao::prelude::AssetData;
use sea_orm::{entity::*, query::*, DatabaseConnection};
pub struct DownloadMetadata {
    pub asset_data_id: i64,
    pub uri: String,
}

#[async_trait]
impl BgTask for DownloadMetadata {
    async fn task(&self, db: &DatabaseConnection) -> Result<(), IngesterError> {
        let body: serde_json::Value = reqwest::get(self.uri.clone()) // Need to check for malicious sites ?
            .await?
            .json()
            .await?;
        let model = asset_data::ActiveModel {
            id: Unchanged(self.asset_data_id),
            metadata: Set(body),
            ..Default::default()
        };
        asset_data::Entity::update(model)
            .filter(asset_data::Column::Id.eq(self.asset_data_id))
            .exec(db)
            .await
            .map(|_| ())
            .map_err(|db| {
                IngesterError::TaskManagerError(format!(
                    "Database error with {}, error: {}",
                    self, db
                ))
            })
    }
}

impl Display for DownloadMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DownloadMetadata from {} for {}",
            self.uri, self.asset_data_id
        )
    }
}
