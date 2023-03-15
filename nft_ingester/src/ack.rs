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
    stream: &'static str,
    config: MessengerConfig,
) -> (JoinHandle<()>, UnboundedSender<String>) {
    let (tx, mut rx) = unbounded_channel::<String>();

    (
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(500));
            let mut acks = Vec::new();
            let source = T::new(config).await;
            if let Ok(mut msg) = source {
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                        let len = acks.len();
                            if let Err(e) = msg.ack_msg(&stream, &acks).await {
                                error!("Error acking message: {}", e);
                            }
                            metric! {
                                statsd_count!("ingester.ack", len as i64, "stream" => stream);
                            }
                            acks.clear();
                        }
                        Some(msg_id) = rx.recv() => {
                            acks.push(msg_id);
                        }
                    }
                }
            }
        }),
        tx,
    )
}
