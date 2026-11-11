use std::collections::HashMap;

use cadence_macros::{is_global_default_set, statsd_count};
use log::error;
use plerkle_messenger::{Messenger, MessengerConfig};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedSender},
    task::JoinHandle,
    time::{interval, Duration},
};

use crate::{error::IngesterError, metric, worker::connect_messenger};

pub type AckSender = UnboundedSender<(&'static str, String)>;

pub fn ack_worker<T: Messenger>(
    config: MessengerConfig,
) -> (JoinHandle<Result<(), IngesterError>>, AckSender) {
    let (tx, mut rx) = unbounded_channel::<(&'static str, String)>();
    (
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));
            let mut acks: HashMap<&str, Vec<String>> = HashMap::new();
            let mut msg = connect_messenger::<T>(config, "acknowledgements").await?;
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
        }),
        tx,
    )
}
