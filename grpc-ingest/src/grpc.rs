use {
    crate::{
        config::ConfigGrpc, prom::redis_xadd_status_inc, redis::metrics_xlen, util::create_shutdown,
    },
    anyhow::Context,
    futures::{channel::mpsc, stream::StreamExt, SinkExt},
    lru::LruCache,
    redis::{streams::StreamMaxlen, RedisResult, Value as RedisValue},
    std::num::NonZeroUsize,
    std::{collections::HashMap, sync::Arc, time::Duration},
    tokio::{
        spawn,
        task::JoinSet,
        time::{sleep, Instant},
    },
    tracing::warn,
    yellowstone_grpc_client::GeyserGrpcClient,
    yellowstone_grpc_proto::{
        geyser::SubscribeRequest, prelude::subscribe_update::UpdateOneof, prost::Message,
    },
    yellowstone_grpc_tools::config::GrpcRequestToProto,
};

pub async fn run(config: ConfigGrpc) -> anyhow::Result<()> {
    let config = Arc::new(config);
    let (tx, mut rx) = mpsc::channel::<UpdateOneof>(config.geyser_update_message_buffer_size); // Adjust buffer size as needed

    // Connect to Redis
    let client = redis::Client::open(config.redis.url.clone())?;
    let connection = client.get_multiplexed_tokio_connection().await?;

    // Check stream length for the metrics
    let jh_metrics_xlen = spawn({
        let connection = connection.clone();
        let streams = vec![
            config.accounts.stream.clone(),
            config.transactions.stream.clone(),
        ];
        async move { metrics_xlen(connection, &streams).await }
    });
    tokio::pin!(jh_metrics_xlen);

    // Spawn gRPC client connections
    for endpoint in config.geyser_endpoints.clone() {
        let config = Arc::clone(&config);
        let mut tx = tx.clone();

        let mut client = GeyserGrpcClient::build_from_shared(endpoint)?
            .x_token(config.x_token.clone())?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(10))
            .connect()
            .await
            .context("failed to connect to gRPC")?;

        spawn(async move {
            let mut accounts = HashMap::with_capacity(1);
            let mut transactions = HashMap::with_capacity(1);

            accounts.insert("das".to_string(), config.accounts.filter.clone().to_proto());
            transactions.insert(
                "das".to_string(),
                config.transactions.filter.clone().to_proto(),
            );

            let request = SubscribeRequest {
                accounts,
                transactions,
                ..Default::default()
            };

            let (_subscribe_tx, mut stream) = client.subscribe_with_request(Some(request)).await?;

            while let Some(Ok(msg)) = stream.next().await {
                if let Some(update) = msg.update_oneof {
                    tx.send(update)
                        .await
                        .expect("Failed to send update to management thread");
                }
            }
            Ok::<(), anyhow::Error>(())
        });
    }

    // Management thread
    let mut shutdown = create_shutdown()?;
    let mut tasks = JoinSet::new();
    let mut pipe = redis::pipe();
    let mut pipe_accounts = 0;
    let mut pipe_transactions = 0;
    let deadline = sleep(config.redis.pipeline_max_idle);
    tokio::pin!(deadline);

    let mut seen_update_events = LruCache::<String, ()>::new(
        NonZeroUsize::new(config.solana_seen_event_cache_max_size).expect("Non zero value"),
    );

    let result = loop {
        tokio::select! {
            result = &mut jh_metrics_xlen => match result {
                Ok(Ok(_)) => unreachable!(),
                Ok(Err(error)) => break Err(error),
                Err(error) => break Err(error.into()),
            },
            Some(signal) = shutdown.next() => {
                warn!("{signal} received, waiting spawned tasks...");
                break Ok(());
            },
            Some(update) = rx.next() => {
                match update {
                    UpdateOneof::Account(account) => {
                        let slot_pubkey = format!("{}:{}", account.slot, hex::encode(account.account.as_ref().map(|account| account.pubkey.clone()).unwrap_or_default()));

                        if seen_update_events.get(&slot_pubkey).is_some() {
                            continue;
                        } else {
                            seen_update_events.put(slot_pubkey, ());
                        };

                        pipe.xadd_maxlen(
                            &config.accounts.stream,
                            StreamMaxlen::Approx(config.accounts.stream_maxlen),
                            "*",
                            &[(&config.accounts.stream_data_key, account.encode_to_vec())],
                        );

                        pipe_accounts += 1;
                    }
                    UpdateOneof::Transaction(transaction) => {
                        let slot_signature = hex::encode(transaction.transaction.as_ref().map(|t| t.signature.clone()).unwrap_or_default()).to_string();

                        if seen_update_events.get(&slot_signature).is_some() {
                            continue;
                        } else {
                            seen_update_events.put(slot_signature, ());
                        };

                        pipe.xadd_maxlen(
                            &config.transactions.stream,
                            StreamMaxlen::Approx(config.transactions.stream_maxlen),
                            "*",
                            &[(&config.transactions.stream_data_key, transaction.encode_to_vec())]
                        );

                        pipe_transactions += 1;
                    }
                    _ => continue,
                }
                if pipe_accounts + pipe_transactions >= config.redis.pipeline_max_size {
                    let mut pipe = std::mem::replace(&mut pipe, redis::pipe());
                    let pipe_accounts = std::mem::replace(&mut pipe_accounts, 0);
                    let pipe_transactions = std::mem::replace(&mut pipe_transactions, 0);
                    deadline.as_mut().reset(Instant::now() + config.redis.pipeline_max_idle);

                    tasks.spawn({
                        let mut connection = connection.clone();
                        let config = Arc::clone(&config);
                        async move {
                            let result: RedisResult<RedisValue> =
                                pipe.atomic().query_async(&mut connection).await;

                            let status = result.map(|_| ()).map_err(|_| ());
                            redis_xadd_status_inc(&config.accounts.stream, status, pipe_accounts);
                            redis_xadd_status_inc(&config.transactions.stream, status, pipe_transactions);

                            Ok::<(), anyhow::Error>(())
                        }
                    });
                }
            },
            _ = &mut deadline => {
                if pipe_accounts + pipe_transactions > 0 {
                    let mut pipe = std::mem::replace(&mut pipe, redis::pipe());
                    let pipe_accounts = std::mem::replace(&mut pipe_accounts, 0);
                    let pipe_transactions = std::mem::replace(&mut pipe_transactions, 0);
                    deadline.as_mut().reset(Instant::now() + config.redis.pipeline_max_idle);

                    tasks.spawn({
                        let mut connection = connection.clone();
                        let config = Arc::clone(&config);
                        async move {
                            let result: RedisResult<RedisValue> =
                                pipe.atomic().query_async(&mut connection).await;

                            let status = result.map(|_| ()).map_err(|_| ());
                            redis_xadd_status_inc(&config.accounts.stream, status, pipe_accounts);
                            redis_xadd_status_inc(&config.transactions.stream, status, pipe_transactions);

                            Ok::<(), anyhow::Error>(())
                        }
                    });
                }
            },
        };

        while tasks.len() >= config.redis.max_xadd_in_process {
            if let Some(result) = tasks.join_next().await {
                result??;
            }
        }
    };

    tokio::select! {
        Some(signal) = shutdown.next() => {
            anyhow::bail!("{signal} received, force shutdown...");
        }
        result = async move {
            while let Some(result) = tasks.join_next().await {
                result??;
            }
            Ok::<(), anyhow::Error>(())
        } => result?,
    };

    result
}
