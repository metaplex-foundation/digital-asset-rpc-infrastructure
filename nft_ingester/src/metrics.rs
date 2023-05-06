use std::net::UdpSocket;

use cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient};
use cadence_macros::{is_global_default_set, set_global_default, statsd_count, statsd_time};
use log::{error, warn};
use tokio::time::Instant;

use crate::{
    config::{IngesterConfig, CODE_VERSION},
    error::IngesterError,
};

#[macro_export]
macro_rules! metric {
    {$($block:stmt;)*} => {
        if is_global_default_set() {
            $(
                $block
            )*
        }
    };
}

pub fn setup_metrics(config: &IngesterConfig) {
    let uri = config.metrics_host.clone();
    let port = config.metrics_port;
    let env = config.env.clone().unwrap_or("dev".to_string());
    if uri.is_some() || port.is_some() {
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let host = (uri.unwrap(), port.unwrap());
        let udp_sink = BufferedUdpMetricSink::from(host, socket).unwrap();
        let queuing_sink = QueuingMetricSink::from(udp_sink);
        let builder = StatsdClient::builder("das_ingester", queuing_sink);
        let client = builder
            .with_tag("env", env)
            .with_tag("version", CODE_VERSION)
            .build();
        set_global_default(client);
    }
}

pub fn capture_result(
    id: String,
    stream: &str,
    label: (&str, &str),
    tries: usize,
    res: Result<(), IngesterError>,
    proc: Instant,
) -> Option<String> {
    let mut ret_id = None;
    match res {
        Ok(_) => {
            metric! {
                statsd_time!("ingester.proc_time", proc.elapsed().as_millis() as u64, label.0 => label.1, "stream" => stream);
            }
            if tries == 0 {
                metric! {
                    statsd_count!("ingester.ingest_success", 1, label.0 => label.1, "stream" => stream);
                }
            } else {
                metric! {
                    statsd_count!("ingester.redeliver_success", 1, label.0 => label.1, "stream" => stream);
                }
            }
            ret_id = Some(id);
        }
        Err(err) if err == IngesterError::NotImplemented => {
            metric! {
                statsd_count!("ingester.not_implemented", 1, label.0 => label.1, "stream" => stream, "error" => "ni");
            }
            ret_id = Some(id);
        }
        Err(IngesterError::DeserializationError(e)) => {
            metric! {
                statsd_count!("ingester.ingest_error", 1, label.0 => label.1, "stream" => stream, "error" => "de");
            }
            warn!("{}", e);
            ret_id = Some(id);
        }
        Err(IngesterError::ParsingError(e)) => {
            metric! {
                statsd_count!("ingester.ingest_error", 1, label.0 => label.1, "stream" => stream, "error" => "parse");
            }
            warn!("{}", e);
            ret_id = Some(id);
        }
        Err(err) => {
            error!("Error handling account update: {:?}", err);
            metric! {
                statsd_count!("ingester.ingest_update_error", 1, label.0 => label.1, "stream" => stream, "error" => "u");
            }
        }
    }
    ret_id
}
