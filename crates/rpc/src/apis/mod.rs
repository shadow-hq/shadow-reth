mod get_logs;
mod subscribe;
mod types;

pub(crate) use get_logs::*;
pub(crate) use subscribe::*;
pub(crate) use types::*;

use crate::{ShadowRpc, ShadowRpcApiServer};
use jsonrpsee::{
    core::{async_trait, RpcResult, SubscriptionResult},
    PendingSubscriptionSink,
};
use reth_provider::{BlockNumReader, BlockReaderIdExt};

#[async_trait]
impl<P> ShadowRpcApiServer for ShadowRpc<P>
where
    P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
{
    async fn get_logs(&self, params: GetLogsParameters) -> RpcResult<Vec<RpcLog>> {
        get_logs(self, params).await
    }

    async fn subscribe(
        &self,
        pending: PendingSubscriptionSink,
        params: SubscribeParameters,
    ) -> SubscriptionResult {
        subscribe(self, pending, params).await
    }
}
