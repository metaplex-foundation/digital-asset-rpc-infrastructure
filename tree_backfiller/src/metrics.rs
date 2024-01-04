use anyhow::Result;
use cadence::{BufferedUdpMetricSink, Counted, Gauged, QueuingMetricSink, StatsdClient, Timed};
use clap::Parser;
use log::error;
use std::time::Duration;
use std::{net::UdpSocket, sync::Arc};

#[derive(Clone, Parser, Debug)]
pub struct MetricsArgs {
    #[arg(long, env, default_value = "127.0.0.1")]
    pub metrics_host: String,
    #[arg(long, env, default_value = "8125")]
    pub metrics_port: u16,
    #[arg(long, env, default_value = "das.backfiller")]
    pub metrics_prefix: String,
}

#[derive(Clone, Debug)]
pub struct Metrics(Arc<StatsdClient>);

impl Metrics {
    pub fn try_from_config(config: MetricsArgs) -> Result<Self> {
        let host = (config.metrics_host, config.metrics_port);

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;

        let udp_sink = BufferedUdpMetricSink::from(host, socket)?;
        let queuing_sink = QueuingMetricSink::from(udp_sink);
        let client = StatsdClient::from_sink(&config.metrics_prefix, queuing_sink);

        Ok(Metrics(Arc::new(client)))
    }

    pub fn time(&self, key: &str, duration: Duration) {
        if let Err(e) = self.0.time(key, duration) {
            error!("submitting time: {:?}", e)
        }
    }

    pub fn increment(&self, key: &str) {
        if let Err(e) = self.0.count(key, 1) {
            error!("submitting increment: {:?}", e)
        }
    }
}
