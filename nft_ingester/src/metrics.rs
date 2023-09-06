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

// Returns a boolean indicating whether the redis message should be ACK'd.
// If the message is not ACK'd, it will be retried as long as it is under the retry limit.
pub fn capture_result(
    id: String,
    stream: &str,
    label: (&str, &str),
    tries: usize,
    res: Result<(), IngesterError>,
    proc: Instant,
    txn_sig: Option<&str>,
    account: Option<String>,
) -> bool {
    let mut should_ack = false;
    match res {
        Ok(_) => {
            metric! {
                statsd_time!("ingester.proc_time", proc.elapsed().as_millis() as u64, label.0 => &label.1, "stream" => stream);
            }
            if tries == 0 {
                metric! {
                    statsd_count!("ingester.ingest_success", 1, label.0 => &label.1, "stream" => stream);
                }
            } else {
                metric! {
                    statsd_count!("ingester.redeliver_success", 1, label.0 => &label.1, "stream" => stream);
                }
            }
            should_ack = true;
        }
        Err(err) if err == IngesterError::NotImplemented => {
            metric! {
                statsd_count!("ingester.not_implemented", 1, label.0 => &label.1, "stream" => stream, "error" => "ni");
            }
            should_ack = true;
        }
        Err(IngesterError::DeserializationError(e)) => {
            metric! {
                statsd_count!("ingester.ingest_error", 1, label.0 => &label.1, "stream" => stream, "error" => "de");
            }
            if let Some(sig) = txn_sig {
                warn!("Error deserializing txn {}: {:?}", sig, e);
            } else if let Some(account) = account {
                warn!("Error deserializing account {}: {:?}", account, e);
            } else {
                warn!("{}", e);
            }
            // Non-retryable error.
            should_ack = true;
        }
        Err(IngesterError::ParsingError(e)) => {
            metric! {
                statsd_count!("ingester.ingest_error", 1, label.0 => &label.1, "stream" => stream, "error" => "parse");
            }
            if let Some(sig) = txn_sig {
                warn!("Error parsing txn {}: {:?}", sig, e);
            } else if let Some(account) = account {
                warn!("Error parsing account {}: {:?}", account, e);
            } else {
                warn!("{}", e);
            }
            // Non-retryable error.
            should_ack = true;
        }
        Err(IngesterError::DatabaseError(e)) => {
            metric! {
                statsd_count!("ingester.database_error", 1, label.0 => &label.1, "stream" => stream, "error" => "db");
            }
            if let Some(sig) = txn_sig {
                warn!("Error database txn {}: {:?}", sig, e);
            } else {
                warn!("{}", e);
            }
            should_ack = false;
        }
        Err(IngesterError::AssetIndexError(e)) => {
            metric! {
                statsd_count!("ingester.index_error", 1, label.0 => &label.1, "stream" => stream, "error" => "index");
            }
            if let Some(sig) = txn_sig {
                warn!("Error indexing transaction {}: {:?}", sig, e);
            } else {
                warn!("Error indexing account: {:?}", e);
            }
            should_ack = false;
        }
        Err(err) => {
            if let Some(sig) = txn_sig {
                error!("Error handling update for txn {}: {:?}", sig, err);
            } else if let Some(account) = account {
                error!("Error handling update for account {}: {:?}", account, err);
            } else {
                error!("Error handling update: {:?}", err);
            }
            metric! {
                statsd_count!("ingester.ingest_update_error", 1, label.0 => &label.1, "stream" => stream, "error" => "u");
            }
            should_ack = false;
        }
    }
    should_ack
}
