use anyhow::Result;
use cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient, Timed};
use cadence_macros::set_global_default;
use clap::Parser;
use std::net::UdpSocket;

#[derive(Clone, Parser, Debug)]
pub struct MetricsArgs {
    #[arg(long, env, default_value = "127.0.0.1")]
    pub metrics_host: String,
    #[arg(long, env, default_value = "8125")]
    pub metrics_port: u16,
    #[arg(long, env, default_value = "das.backfiller")]
    pub metrics_prefix: String,
}

pub fn setup_metrics(config: MetricsArgs) -> Result<()> {
    let host = (config.metrics_host, config.metrics_port);

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_nonblocking(true)?;

    let udp_sink = BufferedUdpMetricSink::from(host, socket)?;
    let queuing_sink = QueuingMetricSink::from(udp_sink);

    let client = StatsdClient::from_sink(&config.metrics_prefix, queuing_sink);

    set_global_default(client);

    Ok(())
}
