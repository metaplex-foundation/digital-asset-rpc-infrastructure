use std::{collections::{HashSet, VecDeque}, str::FromStr, sync::Arc};
use plerkle_serialization::{
    serializer::seralize_encoded_transaction_with_status
};
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
};
use solana_sdk::{
    pubkey::Pubkey, signature::Signature
};

use tokio::{
    sync::{mpsc::{self, UnboundedReceiver}},
    task::JoinHandle,
};
use tokio_stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

pub fn find_sigs<'a>(
    address: Pubkey,
    client: RpcClient,
    failed: bool
) -> Result<(JoinHandle<Result<(), String>>, UnboundedReceiver<String>), String> {
    let mut last_sig = None;
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let jh = tokio::spawn(async move {
        loop {
            let before = last_sig;
            let sigs = client
                .get_signatures_for_address_with_config(
                    &address,
                    GetConfirmedSignaturesForAddress2Config {
                        before,
                        until: None,
                        ..GetConfirmedSignaturesForAddress2Config::default()
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
            for sig in sigs.iter() {
                let sig_str = Signature::from_str(&sig.signature).map_err(|e| e.to_string())?;
                last_sig = Some(sig_str);
                if sig.confirmation_status.is_none() || sig.err.is_some() {
                    continue;
                }
                tx.send(sig_str.to_string()).map_err(|e| e.to_string())?;
            }
            if sigs.is_empty() || sigs.len() < 1000 {
                break;
            }
        }
        Ok(())
    });
    Ok((jh, rx))
}



pub struct Siggrabbenheimer {
    address: Pubkey,
    handle: Option<JoinHandle<Result<(), String>>>,
    chan: UnboundedReceiver<String>,
}
impl Siggrabbenheimer {
    pub fn new(client: RpcClient, address: Pubkey, failed: bool) -> Self {
        let (handle, chan) = find_sigs(address, client, failed).unwrap();
        
        Self {
            address,
            chan,
            handle: Some(handle),
        }
    }
}

impl Stream for Siggrabbenheimer {
    type Item = String;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Option<String>>
    {
        self.chan.poll_recv(cx)    
    }
}