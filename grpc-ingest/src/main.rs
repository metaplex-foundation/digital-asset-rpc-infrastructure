use {
    crate::{
        config::{
            load as config_load, ConfigDownloadMetadata, ConfigGrpc, ConfigIngester,
            ConfigPrometheus,
        },
        prom::run_server as prometheus_run_server,
        tracing::init as tracing_init,
    },
    anyhow::Context,
    clap::{Parser, Subcommand},
    std::net::SocketAddr,
};

mod config;
mod download_metadata;
mod grpc;
mod ingester;
mod postgres;
mod prom;
mod redis;
mod tracing;
mod util;
mod version;

#[derive(Debug, Parser)]
#[clap(author, version)]
struct Args {
    /// Path to config file
    #[clap(short, long)]
    config: String,

    /// Prometheus listen address
    #[clap(long)]
    prometheus: Option<SocketAddr>,

    #[command(subcommand)]
    action: ArgsAction,
}

#[derive(Debug, Clone, Subcommand)]
enum ArgsAction {
    /// Subscribe on Geyser events using gRPC and send them to Redis
    #[command(name = "grpc2redis")]
    Grpc,
    /// Run ingester process (process events from Redis)
    #[command(name = "ingester")]
    Ingester,
    /// Run metadata downloader
    #[command(name = "download-metadata")]
    DownloadMetadata,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_init()?;

    let args = Args::parse();

    // Run prometheus server
    let config = config_load::<ConfigPrometheus>(&args.config)
        .await
        .with_context(|| format!("failed to parse prometheus config from: {}", args.config))?;
    if let Some(address) = args.prometheus.or(config.prometheus) {
        prometheus_run_server(address)?;
    }

    // Run grpc / ingester / download-metadata
    match args.action {
        ArgsAction::Grpc => {
            let config = config_load::<ConfigGrpc>(&args.config)
                .await
                .with_context(|| format!("failed to parse config from: {}", args.config))?;
            grpc::run(config).await
        }
        ArgsAction::Ingester => {
            let config = config_load::<ConfigIngester>(&args.config)
                .await
                .with_context(|| format!("failed to parse config from: {}", args.config))?;
            config.check();
            ingester::run(config).await
        }
        ArgsAction::DownloadMetadata => {
            let config = config_load::<ConfigDownloadMetadata>(&args.config)
                .await
                .with_context(|| format!("failed to parse config from: {}", args.config))?;
            download_metadata::run(config).await
        }
    }
}
