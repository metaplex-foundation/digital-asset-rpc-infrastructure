use crate::{
    tasks::{FromTaskData, IntoTaskData},
    BgTask, IngesterError, TaskData,
};
use async_trait::async_trait;
use digital_asset_types::dao::asset_data;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

const TASK_NAME: &str = "DownloadMetadata";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadata {
    pub asset_data_id: Vec<u8>,
    pub uri: String,
}
impl DownloadMetadata {
    pub fn sanitize(&mut self) {
        self.uri = self.uri.trim().replace('\0', "");
    }
}
impl IntoTaskData for DownloadMetadata {
    fn into_task_data(self) -> Result<TaskData, IngesterError> {
        let data = serde_json::to_value(self)
            .map_err(<serde_json::Error as Into<IngesterError>>::into)?;
        Ok(TaskData {
            name: TASK_NAME,
            data,
        })
    }
}

impl FromTaskData<DownloadMetadata> for DownloadMetadata {
    fn from_task_data(data: TaskData) -> Result<Self, IngesterError> {
        serde_json::from_value(data.data).map_err(|e| e.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadataTask {}

#[async_trait]
impl BgTask for DownloadMetadataTask {
    fn name(&self) -> &'static str {
        TASK_NAME
    }

    fn lock_duration(&self) -> i64 {
        5
    }

    fn max_attempts(&self) -> i16 {
        5
    }

    async fn task(
        &self,
        db: &DatabaseTransaction,
        data: serde_json::Value,
    ) -> Result<(), IngesterError> {
        let download_metadata: DownloadMetadata = serde_json::from_value(data)?;
        let body: serde_json::Value = reqwest::get(download_metadata.uri.clone()) // Need to check for malicious sites ?
            .await?
            .json()
            .await?;
        let model = asset_data::ActiveModel {
            id: Unchanged(download_metadata.asset_data_id.clone()),
            metadata: Set(body),
            ..Default::default()
        };
        asset_data::Entity::update(model)
            .filter(asset_data::Column::Id.eq(download_metadata.asset_data_id.clone()))
            .exec(db)
            .await
            .map(|_| ())
            .map_err(|db| {
                IngesterError::TaskManagerError(format!(
                    "Database error with {}, error: {}",
                    self.name(),
                    db
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
