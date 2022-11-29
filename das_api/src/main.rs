mod api;
mod api_impl;
mod config;
mod error;
mod validation;

use std::time::Instant;
use {
    crate::api::RpcApiBuilder,
    crate::api_impl::DasApi,
    crate::config::load_config,
    crate::config::Config,
    crate::error::DasApiError,
    cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient},
    cadence_macros::set_global_default,
    jsonrpsee::http_server::{HttpServerBuilder, RpcModule},
    jsonrpsee_core::middleware::{Headers, HttpMiddleware, MethodKind, Params},
    std::net::SocketAddr,
    std::net::UdpSocket,
};

use cadence_macros::{is_global_default_set, statsd_time};

pub fn safe_metric<F: Fn() -> ()>(f: F) {
    if is_global_default_set() {
        f()
    }
}

fn setup_metrics(config: &Config) {
    let uri = config.metrics_host.clone();
    let port = config.metrics_port.clone();
    let env = config.env.clone().unwrap_or("dev".to_string());
    if uri.is_some() || port.is_some() {
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let host = (uri.unwrap(), port.unwrap());
        let udp_sink = BufferedUdpMetricSink::from(host, socket).unwrap();
        let queuing_sink = QueuingMetricSink::from(udp_sink);
        let builder = StatsdClient::builder("das_api", queuing_sink);
        let client = builder.with_tag("env", env).build();
        set_global_default(client);
    }
}

#[derive(Clone)]
struct MetricMiddleware;

impl HttpMiddleware for MetricMiddleware {
    type Instant = Instant;

    // Called once the HTTP request is received, it may be a single JSON-RPC call
    // or batch.
    fn on_request(&self, _remote_addr: SocketAddr, _headers: &Headers) -> Instant {
        Instant::now()
    }

    // Called once a single JSON-RPC method call is processed, it may be called multiple times
    // on batches.
    fn on_call(&self, method_name: &str, params: Params, kind: MethodKind) {
        println!(
            "Call to method: '{}' params: {:?}, kind: {}",
            method_name, params, kind
        );
    }

    // Called once a single JSON-RPC call is completed, it may be called multiple times
    // on batches.
    fn on_result(&self, method_name: &str, success: bool, started_at: Instant) {
        println!("Call to '{}' took {:?}", method_name, started_at.elapsed());
        safe_metric(|| {
            let success = success.to_string();
            statsd_time!("api_call", started_at.elapsed(), "method" => method_name, "success" => &success);
        });
    }

    // Called the entire JSON-RPC is completed, called on once for both single calls or batches.
    fn on_response(&self, result: &str, started_at: Instant) {
        println!(
            "complete JSON-RPC response: {}, took: {:?}",
            result,
            started_at.elapsed()
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), DasApiError> {
    let config = load_config()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));
    let server = HttpServerBuilder::default()
        .health_api("/healthz", "healthz")?
        .set_middleware(MetricMiddleware)
        .build(addr)
        .await?;
    setup_metrics(&config);
    let api = DasApi::from_config(config).await?;
    let rpc = RpcApiBuilder::build(Box::new(api))?;
    println!("Server Started");
    server.start(rpc)?.await;
    println!("Server ended");
    Ok(())
}
