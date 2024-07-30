use {
    super::{BgTask, FromTaskData, IngesterError, IntoTaskData, TaskData},
    async_trait::async_trait,
    chrono::{NaiveDateTime, Utc},
    das_core::{DownloadMetadataInfo, DownloadMetadataNotifier},
    digital_asset_types::dao::asset_data,
    futures::future::BoxFuture,
    log::debug,
    reqwest::{Client, ClientBuilder},
    sea_orm::*,
    serde::{Deserialize, Serialize},
    std::{
        fmt::{Display, Formatter},
        time::Duration,
    },
    tokio::sync::mpsc::UnboundedSender,
    url::Url,
};

pub fn create_download_metadata_notifier(
    bg_task_sender: UnboundedSender<TaskData>,
) -> DownloadMetadataNotifier {
    Box::new(
        move |info: DownloadMetadataInfo| -> BoxFuture<
            'static,
            Result<(), Box<dyn std::error::Error + Send + Sync>>,
        > {
            let (asset_data_id, uri) = info.into_inner();
            let task = DownloadMetadata {
                asset_data_id,
                uri,
                created_at: Some(Utc::now().naive_utc()),
            };
            let task = task
                .into_task_data()
                .and_then(|task| {
                    bg_task_sender.send(task).map_err(Into::into)
                })
                .map_err(Into::into);
            Box::pin(async move { task })
        },
    )
}

const TASK_NAME: &str = "DownloadMetadata";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadata {
    pub asset_data_id: Vec<u8>,
    pub uri: String,
    #[serde(skip_serializing)]
    pub created_at: Option<NaiveDateTime>,
}

impl DownloadMetadata {
    pub fn sanitize(&mut self) {
        self.uri = self.uri.trim().replace('\0', "");
    }
}

impl IntoTaskData for DownloadMetadata {
    fn into_task_data(self) -> Result<TaskData, IngesterError> {
        let ts = self.created_at;
        let data =
            serde_json::to_value(self).map_err(<serde_json::Error as Into<IngesterError>>::into)?;
        Ok(TaskData {
            name: TASK_NAME,
            data,
            created_at: ts,
        })
    }
}

impl FromTaskData<DownloadMetadata> for DownloadMetadata {
    fn from_task_data(data: TaskData) -> Result<Self, IngesterError> {
        serde_json::from_value(data.data).map_err(|e| e.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadataTask {
    pub lock_duration: Option<i64>,
    pub max_attempts: Option<i16>,
    pub timeout: Option<Duration>,
}

impl DownloadMetadataTask {
    async fn request_metadata(
        uri: String,
        timeout: Duration,
    ) -> Result<serde_json::Value, IngesterError> {
        let client = ClientBuilder::new().timeout(timeout).build()?;
        let response = Client::get(&client, uri) // Need to check for malicious sites ?
            .send()
            .await?;

        if response.status() != reqwest::StatusCode::OK {
            Err(IngesterError::HttpError {
                status_code: response.status().as_str().to_string(),
            })
        } else {
            let val: serde_json::Value = response.json().await?;
            Ok(val)
        }
    }
}

#[derive(FromQueryResult, Debug, Default, Clone, Eq, PartialEq)]
struct MetadataUrl {
    pub metadata_url: String,
}

#[async_trait]
impl BgTask for DownloadMetadataTask {
    fn name(&self) -> &'static str {
        TASK_NAME
    }

    fn lock_duration(&self) -> i64 {
        self.lock_duration.unwrap_or(5)
    }

    fn max_attempts(&self) -> i16 {
        self.max_attempts.unwrap_or(3)
    }

    async fn task(
        &self,
        db: &DatabaseConnection,
        data: serde_json::Value,
    ) -> Result<(), IngesterError> {
        let download_metadata: DownloadMetadata = serde_json::from_value(data)?;
        let meta_url = Url::parse(&download_metadata.uri);
        let body = match meta_url {
            Ok(_) => {
                DownloadMetadataTask::request_metadata(
                    download_metadata.uri.clone(),
                    self.timeout.unwrap_or(Duration::from_secs(3)),
                )
                .await?
            }
            _ => serde_json::Value::String("Invalid Uri".to_string()), //TODO -> enumize this.
        };

        let query = asset_data::Entity::find_by_id(download_metadata.asset_data_id.clone())
            .select_only()
            .column(asset_data::Column::MetadataUrl)
            .build(DbBackend::Postgres);

        match MetadataUrl::find_by_statement(query).one(db).await? {
            Some(asset) => {
                if asset.metadata_url != download_metadata.uri {
                    debug!(
                        "skipping download metadata of old URI for {:?}",
                        bs58::encode(download_metadata.asset_data_id.clone()).into_string()
                    );
                    return Ok(());
                }
            }
            None => {
                return Err(IngesterError::UnrecoverableTaskError(format!(
                    "failed to find URI in database for {:?}",
                    bs58::encode(download_metadata.asset_data_id.clone()).into_string()
                )));
            }
        }

        let model = asset_data::ActiveModel {
            id: Unchanged(download_metadata.asset_data_id.clone()),
            metadata: Set(body),
            reindex: Set(Some(false)),
            ..Default::default()
        };
        debug!(
            "download metadata for {:?}",
            bs58::encode(download_metadata.asset_data_id.clone()).into_string()
        );
        asset_data::Entity::update(model)
            .filter(asset_data::Column::Id.eq(download_metadata.asset_data_id.clone()))
            .filter(
                Condition::all()
                    .add(asset_data::Column::MetadataUrl.eq(download_metadata.uri.clone())),
            )
            .exec(db)
            .await
            .map(|_| ())
            .map_err(|db| {
                IngesterError::TaskManagerError(format!(
                    "Database error with {}, error: {}",
                    self.name(),
                    db
                ))
            })?;

        if meta_url.is_err() {
            return Err(IngesterError::UnrecoverableTaskError(format!(
                "Failed to parse URI: {}",
                download_metadata.uri
            )));
        }
        Ok(())
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
