pub mod api;
mod builder;
mod config;
mod error;
mod metrics;
mod validation;

use crate::{config::DEFAULT_SERVER_PORT, metrics::run_server};
use std::sync::OnceLock;

use {
    crate::api::DasApi, crate::builder::RpcApiBuilder, crate::config::load_config,
    crate::config::Config, std::net::SocketAddr,
};

use hyper::Method;
use jsonrpsee::server::{middleware::proxy_get_request::ProxyGetRequestLayer, ServerBuilder};

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use tower_http::cors::{Any, CorsLayer};

use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

fn setup_metrics(config: &Config) -> anyhow::Result<()> {
    run_server(config.get_prom_metrics_collector_endpoint())
}

fn get_resource() -> Resource {
    static RESOURCE: OnceLock<Resource> = OnceLock::new();
    RESOURCE
        .get_or_init(|| Resource::builder().with_service_name("das-api").build())
        .clone()
}

fn setup_tracer_with_logger(config: &Config) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("das_api=info,digital_asset_types=info,sqlx::query=warn")
    });

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.get_otlp_collector_endpoint())
        .build()
        .unwrap();

    let provider = SdkTracerProvider::builder()
        .with_resource(get_resource())
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer("das-api-tracer");

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_span_events(FmtSpan::CLOSE))
        .with(OpenTelemetryLayer::new(tracer))
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config()?;

    setup_tracer_with_logger(&config);

    setup_metrics(&config)?;

    let addr = SocketAddr::from((
        [0, 0, 0, 0],
        config.server_port.unwrap_or(DEFAULT_SERVER_PORT),
    ));
    let cors = CorsLayer::new()
        .allow_methods([Method::POST, Method::GET])
        .allow_origin(Any)
        .allow_headers([hyper::header::CONTENT_TYPE]);
    let middleware = tower::ServiceBuilder::new()
        .layer(cors)
        .layer(ProxyGetRequestLayer::new("/health", "healthz")?);

    let server = ServerBuilder::default()
        .set_middleware(middleware)
        .max_connections(config.max_request_connections.unwrap_or(100))
        .max_response_body_size(u32::MAX)
        .build(addr)
        .await?;
    let api = DasApi::from_config(config).await?;
    let rpc = RpcApiBuilder::build(Box::new(api))?;
    println!("Server Started");
    let server_handle = server.start(rpc)?;

    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            println!("Shutting down server");
            server_handle.stop()?;
        }

        Err(err) => {
            println!("Unable to listen for shutdown signal: {}", err);
        }
    }
    tokio::spawn(server_handle.stopped());
    println!("Server ended");
    Ok(())
}
