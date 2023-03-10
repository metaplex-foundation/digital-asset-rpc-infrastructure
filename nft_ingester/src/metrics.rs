use std::net::UdpSocket;

use cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient};
use cadence_macros::set_global_default;

use crate::config::{IngesterConfig, CODE_VERSION};

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
        let cons = config
            .messenger_config
            .connection_config
            .get("consumer_id")
            .unwrap()
            .as_str()
            .unwrap();
        let builder = StatsdClient::builder("das_ingester", queuing_sink);
        let client = builder
            .with_tag("env", env)
            .with_tag("consumer_id", cons)
            .with_tag("version", CODE_VERSION)
            .build();
        set_global_default(client);
    }
}
