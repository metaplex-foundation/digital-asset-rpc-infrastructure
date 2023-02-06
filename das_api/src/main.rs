pub mod api;
mod builder;
mod config;
mod error;
mod validation;

use std::time::{Duration, Instant};
use {
    crate::api::DasApi,
    crate::builder::RpcApiBuilder,
    crate::config::load_config,
    crate::config::Config,
    crate::error::DasApiError,
    cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient},
    cadence_macros::set_global_default,
    std::net::SocketAddr,
    std::net::UdpSocket,
};

use hyper::{http, Method};
use log::{debug, info};
use tower_http::cors::{Any, CorsLayer};

use jsonrpsee::server::{
    logger::{Logger, TransportProtocol},
    middleware::proxy_get_request::ProxyGetRequestLayer,
    RpcModule, ServerBuilder,
};

use cadence_macros::{is_global_default_set, statsd_time};

pub fn safe_metric<F: Fn()>(f: F) {
    if is_global_default_set() {
        f()
    }
}

fn setup_metrics(config: &Config) {
    let uri = config.metrics_host.clone();
    let port = config.metrics_port;
    let env = config.env.clone().unwrap_or_else(|| "dev".to_string());
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

impl Logger for MetricMiddleware {
    type Instant = Instant;

    fn on_request(&self, _t: TransportProtocol) -> Self::Instant {
        Instant::now()
    }

    fn on_result(
        &self,
        name: &str,
        success: bool,
        started_at: Self::Instant,
        _t: TransportProtocol,
    ) {
        let stat = match success {
            true => "success",
            false => "failure",
        };
        info!(
            "Call to '{}' {} took {:?}",
            name,
            stat,
            started_at.elapsed()
        );
        safe_metric(|| {
            let success = success.to_string();
            statsd_time!("api_call", started_at.elapsed(), "method" => name, "success" => &success);
        });
    }

    fn on_connect(
        &self,
        remote_addr: SocketAddr,
        _request: &jsonrpsee::server::logger::HttpRequest,
        _t: TransportProtocol,
    ) {
        debug!("Connecting from {}", remote_addr)
    }

    fn on_call(
        &self,
        method_name: &str,
        params: jsonrpsee::types::Params,
        kind: jsonrpsee::server::logger::MethodKind,
        transport: TransportProtocol,
    ) {
        debug!("Call: {} {:?}", method_name, params);
    }

    fn on_response(&self, result: &str, started_at: Self::Instant, transport: TransportProtocol) {
        debug!("Response: {}", result);
    }

    fn on_disconnect(&self, remote_addr: SocketAddr, transport: TransportProtocol) {
        debug!("Disconnecting from {}", remote_addr);
    }
}

#[tokio::main]
async fn main() -> Result<(), DasApiError> {
    let config = load_config()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));
    let cors = CorsLayer::new()
        .allow_methods([Method::POST, Method::GET])
        .allow_origin(Any)
        .allow_headers([hyper::header::CONTENT_TYPE]);
    setup_metrics(&config);
    let middleware = tower::ServiceBuilder::new()
        .layer(cors)
        .layer(ProxyGetRequestLayer::new("/health", "healthz")?);

    let server = ServerBuilder::default()
        .set_middleware(middleware)
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
