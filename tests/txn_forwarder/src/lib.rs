use {
    anyhow::Context,
    futures::stream::{BoxStream, StreamExt},
    log::{error, info},
    serde::de::DeserializeOwned,
    solana_client::{
        client_error::ClientError, client_error::Result as RpcClientResult,
        nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
        rpc_request::RpcRequest,
    },
    solana_sdk::{
        pubkey::Pubkey,
        signature::{ParseSignatureError, Signature},
    },
    std::{fmt, io::Result as IoResult, str::FromStr},
    tokio::{
        fs::File,
        io::{stdin, AsyncBufReadExt, BufReader},
        sync::mpsc,
        time::{sleep, Duration},
    },
    tokio_stream::wrappers::LinesStream,
};

#[derive(Debug, thiserror::Error)]
pub enum FindSignaturesError {
    #[error("Failed to fetch signatures: {0}")]
    Fetch(#[from] ClientError),
    #[error("Failed to parse signature: {0}")]
    Parse(#[from] ParseSignatureError),
}

pub fn find_signatures(
    address: Pubkey,
    client: RpcClient,
    buffer: usize,
) -> mpsc::Receiver<Result<Signature, FindSignaturesError>> {
    let (chan, rx) = mpsc::channel(buffer);
    tokio::spawn(async move {
        let mut last_signature = None;
        loop {
            info!(
                "fetching signatures for {} before {:?}",
                address, last_signature
            );
            let config = GetConfirmedSignaturesForAddress2Config {
                before: last_signature,
                until: None,
                ..Default::default()
            };
            match client
                .get_signatures_for_address_with_config(&address, config)
                .await
            {
                Ok(vec) => {
                    info!(
                        "fetched {} signatures for address {:?} before {:?}",
                        vec.len(),
                        address,
                        last_signature
                    );
                    for tx in vec.iter() {
                        match Signature::from_str(&tx.signature) {
                            Ok(signature) => {
                                last_signature = Some(signature);
                                if tx.confirmation_status.is_some() && tx.err.is_none() {
                                    chan.send(Ok(signature)).await.map_err(|_| ())?;
                                }
                            }
                            Err(error) => {
                                chan.send(Err(error.into())).await.map_err(|_| ())?;
                            }
                        }
                    }
                    if vec.is_empty() {
                        break;
                    }
                }
                Err(error) => {
                    chan.send(Err(error.into())).await.map_err(|_| ())?;
                }
            }
        }
        Ok::<(), ()>(())
    });
    rx
}

pub async fn rpc_send_with_retries<T, E>(
    client: &RpcClient,
    request: RpcRequest,
    value: serde_json::Value,
    max_retries: u8,
    error_key: E,
) -> RpcClientResult<T>
where
    T: DeserializeOwned,
    E: fmt::Debug,
{
    let mut retries = 0;
    let mut delay = Duration::from_millis(500);
    loop {
        match client.send(request, value.clone()).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if retries < max_retries {
                    error!("retrying {request} {error_key:?}: {error}");
                    sleep(delay).await;
                    delay *= 2;
                    retries += 1;
                } else {
                    return Err(error);
                }
            }
        }
    }
}

pub async fn read_lines(path: &str) -> anyhow::Result<BoxStream<'static, IoResult<String>>> {
    Ok(if path == "-" {
        LinesStream::new(BufReader::new(stdin()).lines()).boxed()
    } else {
        let file = File::open(path)
            .await
            .with_context(|| format!("failed to read file: {:?}", path))?;
        LinesStream::new(BufReader::new(file).lines()).boxed()
    }
    .filter_map(|line| async move {
        match line {
            Ok(line) => {
                let line = line.trim();
                (!line.is_empty()).then(|| Ok(line.to_string()))
            }
            Err(error) => Some(Err(error)),
        }
    })
    .boxed())
}
