use std::collections::HashMap;

use cadence_macros::{is_global_default_set, statsd_count};
use log::error;
use plerkle_messenger::{Messenger, MessengerConfig};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedSender},
    task::JoinHandle,
    time::{interval, Duration},
};

use crate::metric;

pub fn ack_worker<T: Messenger>(
    config: MessengerConfig,
) -> (JoinHandle<()>, UnboundedSender<(&'static str, String)>) {
    let (tx, mut rx) = unbounded_channel::<(&'static str, String)>();
    (
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));
            let mut acks: HashMap<&str, Vec<String>> = HashMap::new();
            let source = T::new(config).await;
            if let Ok(mut msg) = source {
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if acks.is_empty() {
                                continue;
                            }
                            let len = acks.len();
                            for (stream, msgs)  in acks.iter_mut() {
                                if let Err(e) = msg.ack_msg(stream, msgs).await {
                                    error!("Error acking message: {}", e);
                                }
                                metric! {
                                    statsd_count!("ingester.ack", len as i64, "stream" => stream);
                                }
                                msgs.clear();
                            }

                        }
                        Some(msg) = rx.recv() => {
                            let (stream, msg) = msg;
                            let ackstream = acks.entry(stream).or_default();
                            ackstream.push(msg);
                        }
                    }
                }
            }
        }),
        tx,
    )
}
