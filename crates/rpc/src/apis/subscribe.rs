//! Contains logic for a shadow RPC equivalent of `eth_subscribe` of `type` `logs`.

use super::AddressRepresentation;
use crate::ShadowRpc;
use jsonrpsee::{
    core::SubscriptionResult, types::ErrorObject, PendingSubscriptionSink, SubscriptionSink,
};
use reth_provider::{BlockNumReader, BlockReaderIdExt};
use serde::{Deserialize, Serialize};
use shadow_reth_common::ShadowSqliteDb;
use tokio::sync::broadcast::Receiver;

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
    indexed_block_hash_receiver: Receiver<String>,
    accepted_sink: SubscriptionSink,
    params: SubscribeParameters,
) -> Result<(), ErrorObject<'static>> {
    // let validated_param_objs = ValidatedQueryParams::new(&provider, params)?;

    // todo: add new query params obj, reuse filter serialization and validation logic
    // todo: query db for logs matching params and block hash

    todo!();
}
