use {
    opentelemetry_sdk::trace::{self, Sampler},
    std::env,
    tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt},
};

pub fn init() -> anyhow::Result<()> {
    let open_tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name(env::var("CARGO_PKG_NAME")?)
        .with_auto_split_batch(true)
        .with_trace_config(trace::config().with_sampler(Sampler::TraceIdRatioBased(0.25)))
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;
    let jeager_layer = tracing_opentelemetry::layer().with_tracer(open_tracer);

    let env_filter = EnvFilter::builder()
        .parse(env::var(EnvFilter::DEFAULT_ENV).unwrap_or_else(|_| "info,sqlx=warn".to_owned()))?;

    let is_atty = atty::is(atty::Stream::Stdout) && atty::is(atty::Stream::Stderr);
    let io_layer = tracing_subscriber::fmt::layer().with_ansi(is_atty);

    let registry = tracing_subscriber::registry()
        .with(jeager_layer)
        .with(env_filter)
        .with(io_layer);

    if env::var_os("RUST_LOG_JSON").is_some() {
        let json_layer = tracing_subscriber::fmt::layer().json().flatten_event(true);
        registry.with(json_layer).try_init()
    } else {
        registry.try_init()
    }
    .map_err(Into::into)
}
