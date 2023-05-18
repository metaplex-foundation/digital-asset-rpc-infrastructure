use {
    anyhow::Context,
    futures::stream::{BoxStream, StreamExt},
    solana_client::{
        client_error::ClientError, nonblocking::rpc_client::RpcClient,
        rpc_client::GetConfirmedSignaturesForAddress2Config,
    },
    solana_sdk::{
        pubkey::Pubkey,
        signature::{ParseSignatureError, Signature},
    },
    std::{io::Result as IoResult, str::FromStr},
    tokio::{
        fs::File,
        io::{stdin, AsyncBufReadExt, BufReader},
        sync::mpsc,
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
                    if vec.is_empty() || vec.len() < 1000 {
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
