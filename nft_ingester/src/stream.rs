use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{error::IngesterError, metric};
use cadence_macros::{is_global_default_set, statsd_count, statsd_gauge};
use plerkle_messenger::{ConsumptionType, Messenger, MessengerConfig, RecvData};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::{JoinHandle, JoinSet},
    time::{self, Duration},
};
use tokio_stream::{Stream, StreamExt};
use tracing::log::error;

pub struct MessengerStreamManager {
    config: MessengerConfig,
    stream_key: &'static str,
    message_receiver: JoinSet<Result<(), IngesterError>>,
}
impl MessengerStreamManager {
    pub fn new(stream: &'static str, messenger_config: MessengerConfig) -> Self {
        let (tx, rx) = unbounded_channel::<RecvData>();
        Self {
            config: messenger_config,
            stream_key: stream,
            message_receiver: JoinSet::new(),
        }
    }

    pub fn listen<T: Messenger>(
        &mut self,
        ct: ConsumptionType,
    ) -> Result<MessengerDataStream, IngesterError> {
        let key = self.stream_key.clone();
        let config = self.config.clone();
        let (stream, send) = MessengerDataStream::new();
        let handle = async move {
            let mut messenger = T::new(config).await?;
            loop {
                let ct = match ct {
                    ConsumptionType::All => ConsumptionType::All,
                    ConsumptionType::New => ConsumptionType::New,
                    ConsumptionType::Redeliver => ConsumptionType::Redeliver,
                };
                let key = key.clone();
                if let Ok(data) = messenger.recv(&key, ct).await {
                    let l = data.len();
                    for r in data {
                        if let Err(e) = send.send(r) {
                            error!("Error forwarding to local stream: {}", e);
                        }
                    }
                    metric! {
                        statsd_gauge!("ingester.batch_size", l as f64, "stream" => key);
                    }
                }
            }
        };
        self.message_receiver.spawn(handle);
        Ok(stream)
    }
}

pub struct MessengerDataStream {
    chan: UnboundedReceiver<RecvData>,
}

impl MessengerDataStream {
    pub fn new() -> (Self, UnboundedSender<RecvData>) {
        let (tx, rx) = unbounded_channel::<RecvData>();
        (Self { chan: rx }, tx)
    }
}

impl Stream for MessengerDataStream {
    type Item = RecvData;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.chan.poll_recv(cx)
    }
}

pub struct StreamSizeTimer {
    interval: tokio::time::Duration,
    messenger_config: MessengerConfig,
    stream: &'static str,
}

impl StreamSizeTimer {
    pub fn new(
        interval_time: Duration,
        messenger_config: MessengerConfig,
        stream: &'static str,
    ) -> Result<Self, IngesterError> {
        Ok(Self {
            interval: interval_time,
            stream,
            messenger_config: messenger_config,
        })
    }

    pub async fn start<T: Messenger>(&mut self) -> Option<JoinHandle<()>> {
        metric! {
            let i = self.interval.clone();
            let messenger_config = self.messenger_config.clone();
            let stream = self.stream;

           return Some(tokio::spawn(async move {
            let messenger = T::new(messenger_config).await;
            if let Ok(mut messenger) = messenger {
            let mut interval = time::interval(i);
                loop {
                    interval.tick().await;
                    let size = messenger.stream_size(stream).await;
                    match size {
                        Ok(size) => {
                            statsd_gauge!("ingester.stream_size", size, "stream" => stream);
                        }
                        Err(e) => {
                            statsd_count!("ingester.stream_size_error", 1, "stream" => stream);
                            error!("Error getting stream size: {}", e);
                        }
                    }
                }
            };
            }));
        }

        None
    }
}
