use jsonrpsee::{core::SubscriptionResult, PendingSubscriptionSink, SubscriptionMessage};
use reth_provider::{BlockNumReader, BlockReaderIdExt};

use crate::ShadowRpc;

impl<P> ShadowRpc<P>
where
    P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
{
    /// TODO: Blah.
    pub async fn subscribe_impl(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        let sink = pending.accept().await?;

        let mut rx_handle = self.shadow_log_rx.resubscribe();

        tokio::spawn(async move {
            while let Ok(log) = rx_handle.recv().await {
                sink.send(SubscriptionMessage::from_json(&log)?).await?;
            }

            Ok(())
        })
        .await?
    }
}
