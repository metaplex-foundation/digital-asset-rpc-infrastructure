use {
    crate::{
        config::ConfigGrpc, prom::redis_xadd_status_inc, redis::metrics_xlen, util::create_shutdown,
    },
    anyhow::Context,
    futures::stream::StreamExt,
    redis::{streams::StreamMaxlen, RedisResult, Value as RedisValue},
    std::collections::HashMap,
    std::{sync::Arc, time::Duration},
    tokio::{
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

    // Connect to Redis
    let client = redis::Client::open(config.redis.url.clone())?;
    let connection = client.get_multiplexed_tokio_connection().await?;

    // Check stream length for the metrics
    let jh_metrics_xlen = tokio::spawn({
        let connection = connection.clone();
        let streams = vec![
            config.accounts.stream.clone(),
            config.transactions.stream.clone(),
        ];
        async move { metrics_xlen(connection, &streams).await }
    });
    tokio::pin!(jh_metrics_xlen);

    // Create gRPC client, subscribe and handle messages
    let mut client = GeyserGrpcClient::build_from_shared(config.endpoint.clone())?
        .x_token(config.x_token.clone())?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10))
        .connect()
        .await
        .context("failed to connect go gRPC")?;

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
    let (mut _subscribe_tx, mut stream) = client.subscribe_with_request(Some(request)).await?;

    // recv-send loop
    let mut shutdown = create_shutdown()?;
    let mut pipe = redis::pipe();
    let mut pipe_accounts = 0;
    let mut pipe_transactions = 0;
    let deadline = sleep(config.redis.pipeline_max_idle);
    tokio::pin!(deadline);
    let mut tasks = JoinSet::new();

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
            msg = stream.next() => {
                match msg {
                    Some(Ok(msg)) => match msg.update_oneof {
                        Some(UpdateOneof::Account(account)) => {
                            pipe.xadd_maxlen(
                                &config.accounts.stream,
                                StreamMaxlen::Approx(config.accounts.stream_maxlen),
                                "*",
                                &[(&config.accounts.stream_data_key, account.encode_to_vec())],
                            );
                            pipe_accounts += 1;
                        }
                        Some(UpdateOneof::Slot(_)) => continue,
                        Some(UpdateOneof::Transaction(transaction)) => {
                            pipe.xadd_maxlen(
                                &config.transactions.stream,
                                StreamMaxlen::Approx(config.transactions.stream_maxlen),
                                "*",
                                &[(&config.transactions.stream_data_key, transaction.encode_to_vec())]
                            );
                            pipe_transactions += 1;
                        }
                        Some(UpdateOneof::Block(_)) => continue,
                        Some(UpdateOneof::Ping(_)) => continue,
                        Some(UpdateOneof::Pong(_)) => continue,
                        Some(UpdateOneof::BlockMeta(_)) => continue,
                        Some(UpdateOneof::Entry(_)) => continue,
                        Some(UpdateOneof::TransactionStatus(_)) => continue,
                        None => break Err(anyhow::anyhow!("received invalid update gRPC message")),
                    },
                    Some(Err(error)) => break Err(error.into()),
                    None => break Err(anyhow::anyhow!("geyser gRPC request is finished")),
                };
                if pipe_accounts + pipe_transactions < config.redis.pipeline_max_size {
                    continue;
                }
            },
            _ = &mut deadline => {},
        };

        let mut pipe = std::mem::replace(&mut pipe, redis::pipe());
        let pipe_accounts = std::mem::replace(&mut pipe_accounts, 0);
        let pipe_transactions = std::mem::replace(&mut pipe_transactions, 0);
        deadline
            .as_mut()
            .reset(Instant::now() + config.redis.pipeline_max_idle);

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
