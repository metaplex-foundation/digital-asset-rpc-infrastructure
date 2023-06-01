use {
    anyhow::Context,
    clap::Parser,
    figment::{util::map, value::Value},
    futures::{
        future::{try_join_all, BoxFuture, FutureExt},
        stream::StreamExt,
    },
    log::info,
    plerkle_messenger::{MessengerConfig, ACCOUNT_STREAM, TRANSACTION_STREAM},
    plerkle_serialization::serializer::seralize_encoded_transaction_with_status,
    prometheus::{IntCounterVec, Opts, Registry},
    solana_client::{
        nonblocking::rpc_client::RpcClient, rpc_config::RpcTransactionConfig,
        rpc_request::RpcRequest,
    },
    solana_sdk::{
        commitment_config::{CommitmentConfig, CommitmentLevel},
        pubkey::Pubkey,
        signature::Signature,
    },
    solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding},
    std::{
        collections::{BTreeMap, HashMap},
        env,
        str::FromStr,
        sync::Arc,
    },
    tokio::{
        sync::{mpsc, Mutex},
        time::Duration,
    },
    txn_forwarder::{find_signatures, read_lines, rpc_send_with_retries, save_metrics},
};

lazy_static::lazy_static! {
    pub static ref TXN_FORWARDER_SENT: IntCounterVec = IntCounterVec::new(
        Opts::new("txn_forwarder_sent", "Number of sent transactions"),
        &["tree"]
    ).unwrap();
}

#[derive(Parser)]
#[command(next_line_help = true)]
struct Cli {
    #[arg(long)]
    redis_url: String,
    #[arg(long)]
    rpc_url: String,
    #[arg(long, short, default_value_t = 25)]
    concurrency: usize,
    /// Size of Redis connections pool
    #[arg(long, default_value_t = 25)]
    concurrency_redis: usize,
    /// Size of signatures queue
    #[arg(long, default_value_t = 25_000)]
    signatures_history_queue: usize,
    #[arg(long, short, default_value_t = 3)]
    max_retries: u8,
    #[arg(long)]
    prom_group: Option<String>,
    /// Path to prometheus output
    #[arg(long)]
    prom: Option<String>,
    /// Prometheus metrics file update interval
    #[arg(long, default_value_t = 1_000)]
    prom_save_interval: u64,
    #[command(subcommand)]
    action: Action,
}

#[derive(clap::Subcommand, Clone)]
enum Action {
    Address {
        #[arg(long)]
        address: String,
        #[arg(long)]
        include_failed: Option<bool>,
    },
    Addresses {
        #[arg(long)]
        file: String,
    },
    Single {
        #[arg(long)]
        txn: String,
    },
    Scenario {
        #[arg(long)]
        scenario_file: String,
    },
}

#[derive(Debug, Clone)]
struct MessengerPool {
    tx: mpsc::Sender<Box<dyn plerkle_messenger::Messenger>>,
    rx: Arc<Mutex<mpsc::Receiver<Box<dyn plerkle_messenger::Messenger>>>>,
}

impl MessengerPool {
    async fn new(size: usize, config: &BTreeMap<String, Value>) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(size);

        for _ in 0..size {
            let messenenger_config = MessengerConfig {
                messenger_type: plerkle_messenger::MessengerType::Redis,
                connection_config: config.clone(),
            };
            let mut messenger = plerkle_messenger::select_messenger(messenenger_config).await?;
            messenger.add_stream(TRANSACTION_STREAM).await?;
            messenger.add_stream(ACCOUNT_STREAM).await?;
            messenger
                .set_buffer_size(TRANSACTION_STREAM, 10000000000000000)
                .await;
            if tx.try_send(messenger).is_err() {
                panic!("expect empty channel");
            }
        }

        Ok(Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        })
    }

    async fn send(
        &self,
        pubkey: Option<Pubkey>,
        signature: Signature,
        tx: EncodedConfirmedTransactionWithStatusMeta,
    ) -> anyhow::Result<()> {
        // ignore if tx failed or meta is missed
        let meta = tx.transaction.meta.as_ref();
        if meta.map(|meta| meta.status.is_err()).unwrap_or(true) {
            return Ok(());
        }

        let fbb = flatbuffers::FlatBufferBuilder::new();
        let fbb = seralize_encoded_transaction_with_status(fbb, tx)
            .with_context(|| format!("failed to serialize transaction with {signature}"))?;
        let bytes = fbb.finished_data();

        let mut rx = self.rx.lock().await;
        let mut messenger = rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("failed to ger messenger"))?;
        drop(rx);

        let result = messenger.send(TRANSACTION_STREAM, bytes).await;
        if self.tx.try_send(messenger).is_err() {
            panic!("expect empty channel");
        }
        result?;

        info!("Sent transaction to stream {signature}");
        if let Some(pubkey) = pubkey {
            TXN_FORWARDER_SENT
                .with_label_values(&[&pubkey.to_string()])
                .inc();
        } else {
            TXN_FORWARDER_SENT.with_label_values(&["undefined"]).inc();
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env::set_var(
        env_logger::DEFAULT_FILTER_ENV,
        env::var_os(env_logger::DEFAULT_FILTER_ENV).unwrap_or_else(|| "info".into()),
    );
    env_logger::init();

    let cli = Cli::parse();

    // metrics
    let mut labels = HashMap::new();
    if let Some(group) = cli.prom_group {
        labels.insert("group".to_owned(), group);
    }
    let registry = Registry::new_custom(None, Some(labels)).unwrap();
    registry.register(Box::new(TXN_FORWARDER_SENT.clone()))?;
    let metrics_jh = save_metrics(
        registry,
        cli.prom,
        Duration::from_millis(cli.prom_save_interval),
    );

    let config_wrapper = Value::from(map! {
        "redis_connection_str" => cli.redis_url,
        "pipeline_size_bytes" => 1u128.to_string(),
    });
    let config = config_wrapper.into_dict().unwrap();
    let messenger = MessengerPool::new(cli.concurrency_redis, &config).await?;
    let (tx, rx) = mpsc::unbounded_channel();

    match cli.action {
        Action::Address {
            include_failed: _include_failed,
            address,
        } => {
            let pubkey = Pubkey::from_str(&address).context("failed to parse address")?;
            tx.send(
                send_address(
                    pubkey,
                    cli.rpc_url,
                    messenger,
                    cli.signatures_history_queue,
                    cli.max_retries,
                    tx.clone(),
                )
                .boxed(),
            )
            .map_err(|_| anyhow::anyhow!("failed to send job"))?;
        }
        Action::Addresses { file } => {
            let mut lines = read_lines(&file).await?;
            while let Some(maybe_line) = lines.next().await {
                let line = maybe_line?;
                let pubkey = Pubkey::from_str(&line).context("failed to parse address")?;
                let rpc_url = cli.rpc_url.clone();
                let messenger = messenger.clone();
                tx.send(
                    send_address(
                        pubkey,
                        rpc_url,
                        messenger,
                        cli.signatures_history_queue,
                        cli.max_retries,
                        tx.clone(),
                    )
                    .boxed(),
                )
                .map_err(|_| anyhow::anyhow!("failed to send job"))?;
            }
        }
        Action::Single { txn } => {
            let sig = Signature::from_str(&txn).context("failed to parse signature")?;
            tx.send(send_tx(None, sig, cli.rpc_url, cli.max_retries, messenger).boxed())
                .map_err(|_| anyhow::anyhow!("failed to send job"))?;
        }
        Action::Scenario { scenario_file } => {
            let mut lines = read_lines(&scenario_file).await?;
            while let Some(maybe_line) = lines.next().await {
                let line = maybe_line?;
                let sig = Signature::from_str(&line).context("failed to parse signature")?;
                let rpc_url = cli.rpc_url.clone();
                let messenger = messenger.clone();
                tx.send(send_tx(None, sig, rpc_url, cli.max_retries, messenger).boxed())
                    .map_err(|_| anyhow::anyhow!("failed to send job"))?;
            }
        }
    }
    drop(tx);

    let rx = Arc::new(Mutex::new(rx));
    try_join_all((0..cli.concurrency).map(|_| {
        let rx = Arc::clone(&rx);
        async move {
            loop {
                let mut locked = rx.lock().await;
                let maybe_fut = locked.recv().await;
                drop(locked);

                match maybe_fut {
                    Some(fut) => fut.await?,
                    None => return Ok::<(), anyhow::Error>(()),
                }
            }
        }
    }))
    .await?;

    metrics_jh.await
}

async fn send_address(
    pubkey: Pubkey,
    rpc_url: String,
    messenger: MessengerPool,
    signatures_history_queue: usize,
    max_retries: u8,
    tasks_tx: mpsc::UnboundedSender<BoxFuture<'static, anyhow::Result<()>>>,
) -> anyhow::Result<()> {
    let client = RpcClient::new(rpc_url.clone());
    let mut all_sig = find_signatures(pubkey, client, signatures_history_queue);
    while let Some(sig) = all_sig.recv().await {
        let rpc_url = rpc_url.clone();
        let messenger = messenger.clone();
        tasks_tx
            .send(send_tx(Some(pubkey), sig?, rpc_url, max_retries, messenger).boxed())
            .map_err(|_| anyhow::anyhow!("failed to send job"))?;
    }
    Ok(())
}

async fn send_tx(
    pubkey: Option<Pubkey>,
    signature: Signature,
    rpc_url: String,
    max_retries: u8,
    messenger: MessengerPool,
) -> anyhow::Result<()> {
    const CONFIG: RpcTransactionConfig = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Base64),
        commitment: Some(CommitmentConfig {
            commitment: CommitmentLevel::Finalized,
        }),
        max_supported_transaction_version: Some(0),
    };

    let client = RpcClient::new(rpc_url);
    let tx: EncodedConfirmedTransactionWithStatusMeta = rpc_send_with_retries(
        &client,
        RpcRequest::GetTransaction,
        serde_json::json!([signature.to_string(), CONFIG,]),
        max_retries,
        signature,
    )
    .await?;
    messenger.send(pubkey, signature, tx).await
}
