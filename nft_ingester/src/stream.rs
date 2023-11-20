use crate::{error::IngesterError, metric};
use cadence_macros::{is_global_default_set, statsd_count, statsd_gauge};
use futures::future::{BoxFuture, FutureExt};
use log::error;
use plerkle_messenger::{Messenger, MessengerConfig};
use tokio::time::{self, Duration};

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

    pub fn start<T: Messenger>(self) -> Option<BoxFuture<'static, anyhow::Result<()>>> {
        metric! {
            let i = self.interval.clone();
            let messenger_config = self.messenger_config.clone();
            let stream = self.stream;

           return Some(async move {
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
            Ok(())
            }.boxed());
        }

        None
    }
}
