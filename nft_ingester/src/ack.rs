use std::collections::HashMap;

use cadence_macros::{is_global_default_set, statsd_count};
use futures::future::{BoxFuture, FutureExt};
use log::error;
use plerkle_messenger::{Messenger, MessengerConfig};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedSender},
    time::{interval, Duration},
};

use crate::metric;

pub fn ack_worker<T: Messenger>(
    config: MessengerConfig,
) -> (
    UnboundedSender<(&'static str, String)>,
    BoxFuture<'static, anyhow::Result<()>>,
) {
    let (tx, mut rx) = unbounded_channel::<(&'static str, String)>();
    let fut = async move {
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
                            if let Err(e) = msg.ack_msg(&stream, &msgs).await {
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
                        let ackstream = acks.entry(stream).or_insert_with(Vec::<String>::new);
                        ackstream.push(msg);
                    }
                }
            }
        }
        Ok(())
    }
    .boxed();
    (tx, fut)
}
