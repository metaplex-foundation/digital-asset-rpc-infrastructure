use {
    crate::{redis::RedisStreamMessageError, version::VERSION as VERSION_INFO},
    das_bubblegum::ProofReport,
    das_core::MetadataJsonTaskError,
    http_body_util::Full,
    hyper::{
        body::{Bytes, Incoming},
        service::service_fn,
        Request, Response,
    },
    hyper_util::{rt::TokioIo, server::conn::auto},
    program_transformers::{error::ProgramTransformerError, AccountInfo},
    prometheus::{
        HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder,
    },
    std::{convert::Infallible, net::SocketAddr, sync::Once},
    tokio::{net::TcpListener, sync::mpsc::error::SendError},
    tracing::{error, info},
};

lazy_static::lazy_static! {
    static ref REGISTRY: Registry = Registry::new();

    static ref VERSION_INFO_METRIC: IntCounterVec = IntCounterVec::new(
        Opts::new("version_info", "Plugin version info"),
        &["buildts", "git", "package", "proto", "rustc", "solana", "version"]
    ).unwrap();

    static ref REDIS_STREAM_LENGTH: IntGaugeVec = IntGaugeVec::new(
        Opts::new("redis_stream_length", "Length of stream in Redis"),
        &["stream"]
    ).unwrap();

    static ref REDIS_XADD_STATUS_COUNT: IntCounterVec = IntCounterVec::new(
        Opts::new("redis_xadd_status_count", "Status of messages sent to Redis stream"),
        &["stream", "label", "status"]
    ).unwrap();

    static ref REDIS_XREAD_COUNT: IntCounterVec = IntCounterVec::new(
        Opts::new("redis_xread_count", "Count of messages seen"),
        &["stream", "consumer"]
    ).unwrap();

    static ref REDIS_XACK_COUNT: IntCounterVec = IntCounterVec::new(
        Opts::new("redis_xack_count", "Total number of processed messages"),
        &["stream", "consumer"]
    ).unwrap();

    static ref PGPOOL_CONNECTIONS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("pgpool_connections", "Total number of connections in Postgres Pool"),
        &["kind"]
    ).unwrap();

    static ref PROGRAM_TRANSFORMER_TASK_STATUS_COUNT: IntCounterVec = IntCounterVec::new(
        Opts::new("program_transformer_task_status_count", "Status of processed messages"),
        &["stream", "consumer", "status"],
    ).unwrap();

    static ref INGEST_JOB_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("ingest_job_time", "Time taken for ingest jobs"),
        &["stream", "consumer"]
    ).unwrap();

    static ref DOWNLOAD_METADATA_FETCHED_COUNT: IntGaugeVec = IntGaugeVec::new(
        Opts::new("download_metadata_fetched_count", "Status of download metadata task"),
        &["status"]
    ).unwrap();

    static ref INGEST_TASKS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("ingest_tasks", "Number of tasks spawned for ingest"),
        &["stream", "consumer"]
    ).unwrap();

    static ref ACK_TASKS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("ack_tasks", "Number of tasks spawned for ack redis messages"),
        &["stream", "consumer"]
    ).unwrap();

    static ref GRPC_TASKS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("grpc_tasks", "Number of tasks spawned for writing grpc messages to redis "),
        &["label","stream"]
    ).unwrap();

    static ref BUBBLEGUM_TREE_TOTAL_LEAVES: IntGaugeVec = IntGaugeVec::new(
        Opts::new("bubblegum_tree_total_leaves", "Total number of leaves in the bubblegum tree"),
        &["tree"]
    ).unwrap();

    static ref BUBBLEGUM_TREE_INCORRECT_PROOFS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("bubblegum_tree_incorrect_proofs", "Number of incorrect proofs in the bubblegum tree"),
        &["tree"]
    ).unwrap();

    static ref BUBBLEGUM_TREE_NOT_FOUND_PROOFS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("bubblegum_tree_not_found_proofs", "Number of not found proofs in the bubblegum tree"),
        &["tree"]
    ).unwrap();

    static ref BUBBLEGUM_TREE_CORRECT_PROOFS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("bubblegum_tree_correct_proofs", "Number of correct proofs in the bubblegum tree"),
        &["tree"]
    ).unwrap();

    static ref BUBBLEGUM_TREE_CORRUPT_PROOFS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("bubblegum_tree_corrupt_proofs", "Number of corrupt proofs in the bubblegum tree"),
        &["tree"]
    ).unwrap();

    static ref DOWNLOAD_METADATA_PUBLISH_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("download_metadata_publish_time", "Time taken for publish download notification to redis"),
        &[]
    ).unwrap();

    static ref CURRENT_INGESTER_SLOT: IntGaugeVec = IntGaugeVec::new(
        Opts::new("current_ingester_slot", "Current slot processed by the ingester"),
        &[]
    ).unwrap();

}

fn metrics_handler() -> Result<Response<Full<Bytes>>, Infallible> {
    let metrics = TextEncoder::new()
        .encode_to_string(&REGISTRY.gather())
        .unwrap_or_else(|error| {
            error!("could not encode custom metrics: {error}");
            String::new()
        });

    Ok(Response::builder()
        .header("content-type", "text/plain")
        .body(Full::new(Bytes::from(metrics)))
        .unwrap())
}

async fn handle_metrics_request(
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    match req.uri().path() {
        "/metrics" => metrics_handler(),
        _ => Ok(not_found_handler()),
    }
}

fn not_found_handler() -> Response<Full<Bytes>> {
    Response::builder()
        .status(404)
        .body(Full::new(Bytes::from("Not Found")))
        .unwrap()
}

pub fn run_metrics_server(address: SocketAddr) -> anyhow::Result<()> {
    // Register once
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        macro_rules! register {
            ($collector:ident) => {
                REGISTRY
                    .register(Box::new($collector.clone()))
                    .expect("collector can't be registered");
            };
        }

        register!(VERSION_INFO_METRIC);
        register!(REDIS_STREAM_LENGTH);
        register!(REDIS_XADD_STATUS_COUNT);
        register!(REDIS_XREAD_COUNT);
        register!(REDIS_XACK_COUNT);
        register!(PGPOOL_CONNECTIONS);
        register!(PROGRAM_TRANSFORMER_TASK_STATUS_COUNT);
        register!(INGEST_JOB_TIME);
        register!(DOWNLOAD_METADATA_FETCHED_COUNT);
        register!(INGEST_TASKS);
        register!(ACK_TASKS);
        register!(GRPC_TASKS);
        register!(BUBBLEGUM_TREE_TOTAL_LEAVES);
        register!(BUBBLEGUM_TREE_INCORRECT_PROOFS);
        register!(BUBBLEGUM_TREE_NOT_FOUND_PROOFS);
        register!(BUBBLEGUM_TREE_CORRECT_PROOFS);
        register!(BUBBLEGUM_TREE_CORRUPT_PROOFS);
        register!(DOWNLOAD_METADATA_PUBLISH_TIME);
        register!(CURRENT_INGESTER_SLOT);

        VERSION_INFO_METRIC
            .with_label_values(&[
                VERSION_INFO.buildts,
                VERSION_INFO.git,
                VERSION_INFO.package,
                VERSION_INFO.proto,
                VERSION_INFO.rustc,
                VERSION_INFO.solana,
                VERSION_INFO.version,
            ])
            .inc();
    });

    tokio::spawn(async move {
        let listener = match TcpListener::bind(address).await {
            Ok(l) => {
                info!("Prometheus server started at http://{address}/metrics");
                l
            }
            Err(e) => {
                error!("Failed to bind Prometheus server: {e:?}");
                return;
            }
        };

        loop {
            let (stream, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(e) => {
                    error!("Prometheus accept failed: {e:?}");
                    continue;
                }
            };

            let io = TokioIo::new(stream);
            let service = service_fn(move |req: Request<Incoming>| handle_metrics_request(req));

            tokio::spawn(async move {
                let builder = auto::Builder::new(hyper_util::rt::TokioExecutor::new());
                let conn = builder.serve_connection(io, service);
                if let Err(e) = conn.await {
                    error!("Prometheus connection failed: {e:?}");
                }
            });
        }
    });

    Ok(())
}

pub fn redis_xlen_set(stream: &str, len: usize) {
    REDIS_STREAM_LENGTH
        .with_label_values(&[stream])
        .set(len as i64);
}

pub fn ingest_job_time_set(stream: &str, consumer: &str, value: f64) {
    INGEST_JOB_TIME
        .with_label_values(&[stream, consumer])
        .observe(value);
}

pub fn redis_xadd_status_inc(stream: &str, label: &str, status: Result<(), ()>, delta: usize) {
    REDIS_XADD_STATUS_COUNT
        .with_label_values(&[
            stream,
            label,
            if status.is_ok() { "success" } else { "failed" },
        ])
        .inc_by(delta as u64);
}

pub fn redis_xread_inc(stream: &str, consumer: &str, delta: usize) {
    REDIS_XREAD_COUNT
        .with_label_values(&[stream, consumer])
        .inc_by(delta as u64)
}

pub fn redis_xack_inc(stream: &str, consumer: &str, delta: usize) {
    REDIS_XACK_COUNT
        .with_label_values(&[stream, consumer])
        .inc_by(delta as u64)
}

#[derive(Debug, Clone, Copy)]
pub enum PgpoolConnectionsKind {
    Total,
    Idle,
}

pub fn pgpool_connections_set(kind: PgpoolConnectionsKind, size: usize) {
    PGPOOL_CONNECTIONS
        .with_label_values(&[match kind {
            PgpoolConnectionsKind::Total => "total",
            PgpoolConnectionsKind::Idle => "idle",
        }])
        .set(size as i64)
}

pub fn ingest_tasks_total_inc(stream: &str, consumer: &str) {
    INGEST_TASKS.with_label_values(&[stream, consumer]).inc()
}

pub fn ingest_tasks_total_dec(stream: &str, consumer: &str) {
    INGEST_TASKS.with_label_values(&[stream, consumer]).dec()
}

pub fn ack_tasks_total_inc(stream: &str, consumer: &str) {
    ACK_TASKS.with_label_values(&[stream, consumer]).inc()
}

pub fn ack_tasks_total_dec(stream: &str, consumer: &str) {
    ACK_TASKS.with_label_values(&[stream, consumer]).dec()
}

pub fn grpc_tasks_total_inc(label: &str, stream: &str) {
    GRPC_TASKS.with_label_values(&[label, stream]).inc()
}

pub fn grpc_tasks_total_dec(label: &str, stream: &str) {
    GRPC_TASKS.with_label_values(&[label, stream]).dec()
}

pub fn download_metadata_json_task_status_count_inc(status: u16) {
    DOWNLOAD_METADATA_FETCHED_COUNT
        .with_label_values(&[&status.to_string()])
        .inc();
}

pub fn download_metadata_publish_time(value: f64) {
    DOWNLOAD_METADATA_PUBLISH_TIME
        .with_label_values::<&str>(&[])
        .observe(value);
}

#[derive(Debug, Clone, Copy)]
pub enum ProgramTransformerTaskStatusKind {
    Success,
    NotImplemented,
    DeserializationError,
    ParsingError,
    ChangeLogEventMalformed,
    StorageWriteError,
    SerializatonError,
    DatabaseError,
    AssetIndexError,
    DownloadMetadataNotify,
    DownloadMetadataSeaOrmError,
    DownloadMetadataFetchError,
    DownloadMetadataAssetNotFound,
    RedisMessageDeserializeError,
    SnapshotSendError,
}

impl From<ProgramTransformerError> for ProgramTransformerTaskStatusKind {
    fn from(error: ProgramTransformerError) -> Self {
        match error {
            ProgramTransformerError::ChangeLogEventMalformed => {
                ProgramTransformerTaskStatusKind::ChangeLogEventMalformed
            }
            ProgramTransformerError::StorageWriteError(_) => {
                ProgramTransformerTaskStatusKind::StorageWriteError
            }
            ProgramTransformerError::NotImplemented => {
                ProgramTransformerTaskStatusKind::NotImplemented
            }
            ProgramTransformerError::DeserializationError(_) => {
                ProgramTransformerTaskStatusKind::DeserializationError
            }
            ProgramTransformerError::SerializatonError(_) => {
                ProgramTransformerTaskStatusKind::SerializatonError
            }
            ProgramTransformerError::ParsingError(_) => {
                ProgramTransformerTaskStatusKind::ParsingError
            }
            ProgramTransformerError::DatabaseError(_) => {
                ProgramTransformerTaskStatusKind::DatabaseError
            }
            ProgramTransformerError::AssetIndexError(_) => {
                ProgramTransformerTaskStatusKind::AssetIndexError
            }
            ProgramTransformerError::DownloadMetadataNotify(_) => {
                ProgramTransformerTaskStatusKind::DownloadMetadataNotify
            }
        }
    }
}

impl From<MetadataJsonTaskError> for ProgramTransformerTaskStatusKind {
    fn from(error: MetadataJsonTaskError) -> Self {
        match error {
            MetadataJsonTaskError::SeaOrm(_) => {
                ProgramTransformerTaskStatusKind::DownloadMetadataSeaOrmError
            }
            MetadataJsonTaskError::Fetch(_) => {
                ProgramTransformerTaskStatusKind::DownloadMetadataFetchError
            }
            MetadataJsonTaskError::AssetNotFound => {
                ProgramTransformerTaskStatusKind::DownloadMetadataAssetNotFound
            }
        }
    }
}

impl From<RedisStreamMessageError> for ProgramTransformerTaskStatusKind {
    fn from(_: RedisStreamMessageError) -> Self {
        ProgramTransformerTaskStatusKind::RedisMessageDeserializeError
    }
}

impl From<sea_orm::DbErr> for ProgramTransformerTaskStatusKind {
    fn from(_: sea_orm::DbErr) -> Self {
        ProgramTransformerTaskStatusKind::DatabaseError
    }
}

impl From<SendError<AccountInfo>> for ProgramTransformerTaskStatusKind {
    fn from(_: SendError<AccountInfo>) -> Self {
        ProgramTransformerTaskStatusKind::SnapshotSendError
    }
}

impl ProgramTransformerTaskStatusKind {
    pub const fn to_str(self) -> &'static str {
        match self {
            ProgramTransformerTaskStatusKind::Success => "success",
            ProgramTransformerTaskStatusKind::NotImplemented => "not_implemented",
            ProgramTransformerTaskStatusKind::DeserializationError => "deserialization_error",
            ProgramTransformerTaskStatusKind::ParsingError => "parsing_error",
            ProgramTransformerTaskStatusKind::ChangeLogEventMalformed => {
                "changelog_event_malformed"
            }
            ProgramTransformerTaskStatusKind::StorageWriteError => "storage_write_error",
            ProgramTransformerTaskStatusKind::SerializatonError => "serialization_error",
            ProgramTransformerTaskStatusKind::DatabaseError => "database_error",
            ProgramTransformerTaskStatusKind::AssetIndexError => "asset_index_error",
            ProgramTransformerTaskStatusKind::DownloadMetadataNotify => "download_metadata_notify",
            ProgramTransformerTaskStatusKind::DownloadMetadataSeaOrmError => {
                "download_metadata_sea_orm_error"
            }
            ProgramTransformerTaskStatusKind::DownloadMetadataFetchError => {
                "download_metadata_fetch_error"
            }
            ProgramTransformerTaskStatusKind::DownloadMetadataAssetNotFound => {
                "download_metadata_asset_not_found"
            }
            ProgramTransformerTaskStatusKind::RedisMessageDeserializeError => {
                "redis_message_deserialize_error"
            }
            ProgramTransformerTaskStatusKind::SnapshotSendError => "snapshot_send_error",
        }
    }
}

pub fn program_transformer_task_status_inc(
    stream: &str,
    consumer: &str,
    kind: ProgramTransformerTaskStatusKind,
) {
    PROGRAM_TRANSFORMER_TASK_STATUS_COUNT
        .with_label_values(&[stream, consumer, kind.to_str()])
        .inc()
}

pub fn update_tree_proof_report(report: &ProofReport) {
    BUBBLEGUM_TREE_TOTAL_LEAVES
        .with_label_values(&[&report.tree_pubkey.to_string()])
        .set(report.total_leaves as i64);

    BUBBLEGUM_TREE_INCORRECT_PROOFS
        .with_label_values(&[&report.tree_pubkey.to_string()])
        .set(report.incorrect_proofs as i64);

    BUBBLEGUM_TREE_NOT_FOUND_PROOFS
        .with_label_values(&[&report.tree_pubkey.to_string()])
        .set(report.not_found_proofs as i64);

    BUBBLEGUM_TREE_CORRECT_PROOFS
        .with_label_values(&[&report.tree_pubkey.to_string()])
        .set(report.correct_proofs as i64);

    BUBBLEGUM_TREE_CORRUPT_PROOFS
        .with_label_values(&[&report.tree_pubkey.to_string()])
        .set(report.corrupt_proofs as i64);
}

pub fn current_ingester_slot_set(slot: i64) {
    CURRENT_INGESTER_SLOT
        .with_label_values::<&str>(&[])
        .set(slot);
}
