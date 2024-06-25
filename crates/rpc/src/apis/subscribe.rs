//! Contains logic for a shadow RPC equivalent of `eth_subscribe` of `type` `logs`.

use super::AddressRepresentation;
use crate::{
    apis::RpcLog,
    shadow_logs_query::{exec_query, ValidatedQueryParams},
    ShadowRpc,
};
use jsonrpsee::{
    core::SubscriptionResult,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
    PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink,
};
use reth_provider::{BlockNumReader, BlockReaderIdExt};
use reth_tracing::tracing::warn;
use serde::{Deserialize, Serialize};
use shadow_reth_common::ShadowSqliteDb;
use tokio::sync::broadcast::{error::RecvError, Receiver};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeParameters {
    pub address: Option<AddressRepresentation>,
    pub topics: Option<Vec<String>>,
}

pub(crate) async fn subscribe<P>(
    rpc: &ShadowRpc<P>,
    pending: PendingSubscriptionSink,
    params: SubscribeParameters,
) -> SubscriptionResult
where
    P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
{
    let sink = pending.accept().await?;
    tokio::spawn({
        let provider = rpc.provider.clone();
        let sqlite_manager = rpc.sqlite_manager.clone();
        let indexed_block_hash_receiver = rpc.indexed_block_hash_receiver.resubscribe();
        async move {
            let _ = handle_accepted(
                provider,
                sqlite_manager,
                indexed_block_hash_receiver,
                sink,
                params,
            )
            .await;
        }
    });

    Ok(())
}

async fn handle_accepted(
    provider: impl BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
    sqlite_manager: ShadowSqliteDb,
    mut indexed_block_hash_receiver: Receiver<String>,
    accepted_sink: SubscriptionSink,
    params: SubscribeParameters,
) -> Result<(), ErrorObject<'static>> {
    loop {
        match indexed_block_hash_receiver.recv().await {
            Ok(block_hash) => {
                let query_params = ValidatedQueryParams::from_subscribe_parameters(
                    &provider,
                    params.clone(),
                    block_hash,
                )?;
                let intermediate_results = exec_query(query_params, &sqlite_manager.pool).await?;
                for result in intermediate_results.into_iter().map(RpcLog::from) {
                    let message = SubscriptionMessage::from_json(&result).map_err(|e| {
                        ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None)
                    })?;

                    accepted_sink.send(message).await.map_err(|e| {
                        ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None)
                    })?;
                }
            }
            Err(RecvError::Lagged(lag_count)) => {
                warn!("lagged by {} messages; consider increasing buffer if syncing", lag_count);
            }
            Err(RecvError::Closed) => {
                // The ExEx has exited, so we should exit as well.
                break;
            }
        }
    }

    Ok(())
}
