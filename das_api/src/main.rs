mod api;
mod api_impl;
mod config;
mod error;
mod validation;

use {
    crate::api::RpcApiBuilder,
    crate::api_impl::DasApi,
    crate::config::load_config,
    crate::config::Config,
    crate::error::DasApiError,
    cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient},
    cadence_macros::set_global_default,
    jsonrpsee::http_server::{HttpServerBuilder, RpcModule},
    std::net::SocketAddr,
    std::net::UdpSocket,
    tokio,
};

fn setup_metrics(config: &Config) {
    let uri = config.metrics_host.clone();
    let port = config.metrics_port.clone();
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    socket.set_nonblocking(true).unwrap();
    let host = (uri, port);
    let udp_sink = BufferedUdpMetricSink::from(host, socket).unwrap();
    let queuing_sink = QueuingMetricSink::from(udp_sink);
    let client = StatsdClient::from_sink("das_api", queuing_sink);
    set_global_default(client);
}

#[tokio::main]
async fn main() -> Result<(), DasApiError> {
    let config = load_config()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));
    let server = HttpServerBuilder::default()
        .health_api("/healthz", "healthz")?
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
