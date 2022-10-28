use crate::{BgTask, IngesterError};
use async_trait::async_trait;
use digital_asset_types::dao::asset_data;

use sea_orm::{DatabaseConnection, *};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadata {
    pub asset_data_id: Vec<u8>,
    pub uri: String,
}

#[async_trait]
impl BgTask for DownloadMetadata {
    fn name(&self) -> &'static str {
        "DownloadMetadata"
    }

    fn data(&self) -> Result<serde_json::Value, IngesterError> {
        serde_json::to_value(&self).map_err(|e| IngesterError::SerializatonError(e.to_string()))
    }

    fn lock_duration(&self) -> i64 {
        5
    }

    fn max_attempts(&self) -> i16 {
        5
    }

    async fn task(&self, db: &DatabaseTransaction) -> Result<(), IngesterError> {
        let body: serde_json::Value = reqwest::get(self.uri.clone()) // Need to check for malicious sites ?
            .await?
            .json()
            .await?;
        let model = asset_data::ActiveModel {
            id: Unchanged(self.asset_data_id.clone()),
            metadata: Set(body),
            ..Default::default()
        };
        asset_data::Entity::update(model)
            .filter(asset_data::Column::Id.eq(self.asset_data_id.clone()))
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
            "DownloadMetadata from {} for {:?}",
            self.uri, self.asset_data_id
        )
    }
}
