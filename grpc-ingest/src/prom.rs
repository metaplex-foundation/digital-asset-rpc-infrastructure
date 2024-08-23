use {
    crate::{redis::RedisStreamMessageError, version::VERSION as VERSION_INFO},
    das_core::MetadataJsonTaskError,
    hyper::{
        server::conn::AddrStream,
        service::{make_service_fn, service_fn},
        Body, Request, Response, Server, StatusCode,
    },
    program_transformers::error::ProgramTransformerError,
    prometheus::{IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry, TextEncoder},
    std::{net::SocketAddr, sync::Once},
    tracing::{error, info},
};

lazy_static::lazy_static! {
    static ref REGISTRY: Registry = Registry::new();

    static ref VERSION: IntCounterVec = IntCounterVec::new(
        Opts::new("version", "Plugin version info"),
        &["buildts", "git", "package", "proto", "rustc", "solana", "version"]
    ).unwrap();

    static ref REDIS_XLEN_TOTAL: IntGaugeVec = IntGaugeVec::new(
        Opts::new("redis_xlen_total", "Length of stream in Redis"),
        &["stream"]
    ).unwrap();

    static ref REDIS_XADD_STATUS: IntCounterVec = IntCounterVec::new(
        Opts::new("redis_xadd_status", "Status of messages sent to Redis stream"),
        &["stream", "status"]
    ).unwrap();

    static ref REDIS_XACK_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("redis_xack_total", "Total number of processed messages"),
        &["stream"]
    ).unwrap();

    static ref PGPOOL_CONNECTIONS_TOTAL: IntGaugeVec = IntGaugeVec::new(
        Opts::new("pgpool_connections_total", "Total number of connections in Postgres Pool"),
        &["kind"]
    ).unwrap();

    static ref PROGRAM_TRANSFORMER_TASKS_TOTAL: IntGauge = IntGauge::new(
        "program_transformer_tasks_total", "Number of tasks spawned for program transform"
    ).unwrap();

    static ref PROGRAM_TRANSFORMER_TASK_STATUS: IntCounterVec = IntCounterVec::new(
        Opts::new("program_transformer_task_status", "Status of processed messages"),
        &["status"],
    ).unwrap();

    static ref DOWNLOAD_METADATA_INSERTED_TOTAL: IntCounter = IntCounter::new(
        "download_metadata_inserted_total", "Total number of inserted tasks for download metadata"
    ).unwrap();

    static ref INGEST_TASKS_TOTAL: IntGaugeVec = IntGaugeVec::new(
        Opts::new("ingest_tasks_total", "Number of tasks spawned for ingest"),
        &["stream"]
    ).unwrap();
}

pub fn run_server(address: SocketAddr) -> anyhow::Result<()> {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        macro_rules! register {
            ($collector:ident) => {
                REGISTRY
                    .register(Box::new($collector.clone()))
                    .expect("collector can't be registered");
            };
        }
        register!(VERSION);
        register!(REDIS_XLEN_TOTAL);
        register!(REDIS_XADD_STATUS);
        register!(REDIS_XACK_TOTAL);
        register!(PGPOOL_CONNECTIONS_TOTAL);
        register!(PROGRAM_TRANSFORMER_TASKS_TOTAL);
        register!(PROGRAM_TRANSFORMER_TASK_STATUS);
        register!(DOWNLOAD_METADATA_INSERTED_TOTAL);
        register!(INGEST_TASKS_TOTAL);

        VERSION
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

    let make_service = make_service_fn(move |_: &AddrStream| async move {
        Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| async move {
            let response = match req.uri().path() {
                "/metrics" => metrics_handler(),
                _ => not_found_handler(),
            };
            Ok::<_, hyper::Error>(response)
        }))
    });
    let server = Server::try_bind(&address)?.serve(make_service);
    info!("prometheus server started: {address:?}");
    tokio::spawn(async move {
        if let Err(error) = server.await {
            error!("prometheus server failed: {error:?}");
        }
    });

    Ok(())
}

fn metrics_handler() -> Response<Body> {
    let metrics = TextEncoder::new()
        .encode_to_string(&REGISTRY.gather())
        .unwrap_or_else(|error| {
            error!("could not encode custom metrics: {}", error);
            String::new()
        });
    Response::builder().body(Body::from(metrics)).unwrap()
}

fn not_found_handler() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

pub fn redis_xlen_set(stream: &str, len: usize) {
    REDIS_XLEN_TOTAL
        .with_label_values(&[stream])
        .set(len as i64);
}

pub fn redis_xadd_status_inc(stream: &str, status: Result<(), ()>, delta: usize) {
    REDIS_XADD_STATUS
        .with_label_values(&[stream, if status.is_ok() { "success" } else { "failed" }])
        .inc_by(delta as u64);
}

pub fn redis_xack_inc(stream: &str, delta: usize) {
    REDIS_XACK_TOTAL
        .with_label_values(&[stream])
        .inc_by(delta as u64)
}

#[derive(Debug, Clone, Copy)]
pub enum PgpoolConnectionsKind {
    Total,
    Idle,
}

pub fn pgpool_connections_set(kind: PgpoolConnectionsKind, size: usize) {
    PGPOOL_CONNECTIONS_TOTAL
        .with_label_values(&[match kind {
            PgpoolConnectionsKind::Total => "total",
            PgpoolConnectionsKind::Idle => "idle",
        }])
        .set(size as i64)
}

pub fn program_transformer_tasks_total_set(size: usize) {
    PROGRAM_TRANSFORMER_TASKS_TOTAL.set(size as i64)
}

pub fn ingest_tasks_total_inc(stream: &str) {
    INGEST_TASKS_TOTAL.with_label_values(&[stream]).inc()
}

pub fn ingest_tasks_total_dec(stream: &str) {
    INGEST_TASKS_TOTAL.with_label_values(&[stream]).dec()
}

pub fn ingest_tasks_reset(stream: &str) {
    INGEST_TASKS_TOTAL.with_label_values(&[stream]).set(0)
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
        }
    }
}

pub fn program_transformer_task_status_inc(kind: ProgramTransformerTaskStatusKind) {
    PROGRAM_TRANSFORMER_TASK_STATUS
        .with_label_values(&[kind.to_str()])
        .inc()
}

pub fn download_metadata_inserted_total_inc() {
    DOWNLOAD_METADATA_INSERTED_TOTAL.inc()
}
