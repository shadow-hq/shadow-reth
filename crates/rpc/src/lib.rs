//! ShadowRPC is a reth RPC extension which allows for reading
//! shadow data written to SQLite by [`reth-shadow-exex`]

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

/// Contains logic for custom RPC API methods.
pub(crate) mod apis;
pub(crate) mod shadow_logs_query;

use std::path::PathBuf;

use apis::{GetLogsParameters, RpcLog, SubscribeParameters};
use eyre::{eyre, Result};
use jsonrpsee::{
    core::{RpcResult, SubscriptionResult},
    proc_macros::rpc,
};
use reth_node_api::FullNodeComponents;
use reth_node_builder::rpc::RpcContext;
use reth_provider::{BlockNumReader, BlockReaderIdExt};
use shadow_reth_common::ShadowSqliteDb;
use tokio::sync::broadcast::Receiver;

#[rpc(server, namespace = "shadow")]
pub trait ShadowRpcApi {
    /// Returns shadow logs.
    #[method(name = "getLogs")]
    async fn get_logs(&self, params: GetLogsParameters) -> RpcResult<Vec<RpcLog>>;

    /// Create a shadow logs subscription.
    #[subscription(name = "subscribe" => "subscription", unsubscribe = "unsubscribe", item = RpcLog)]
    async fn subscribe(&self, params: SubscribeParameters) -> SubscriptionResult;
}

/// Wrapper around an RPC provider and a database connection pool.
#[derive(Debug)]
pub struct ShadowRpc<P> {
    provider: P,
    /// Database manager.
    sqlite_manager: ShadowSqliteDb,
    /// Receives block hashes as they are indexed by the exex.
    indexed_block_hash_receiver: Receiver<String>,
}

impl<Provider> ShadowRpc<Provider> {
    /// Instatiate a Shadow RPC API, building a connection pool to the SQLite database
    /// and initializing tables.
    pub async fn new(
        provider: Provider,
        db_path: &str,
        indexed_block_hash_receiver: Receiver<String>,
    ) -> Result<ShadowRpc<Provider>> {
        Ok(Self {
            provider,
            sqlite_manager: ShadowSqliteDb::new(db_path).await?,
            indexed_block_hash_receiver,
        })
    }

    /// Initializes ShadowRpc, to be called from the `.extend_rpc_modules` reth hook
    /// on node startup.
    pub fn init<Node>(
        ctx: RpcContext<'_, Node>,
        db_path_obj: PathBuf,
        indexed_block_hash_receiver: Receiver<String>,
    ) -> Result<()>
    where
        Node: FullNodeComponents<Provider = Provider>,
        Node::Provider: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
    {
        // Clone the provider so we can move it into the RPC builder thread
        let provider = ctx.provider().clone();

        // Start a new thread, build the ShadowRpc, and join it.
        //
        // We have to do it this way to avoid spawning a runtime within a runtime.
        let shadow_rpc = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to spawn blocking runtime");
            rt.block_on(ShadowRpc::new(
                provider,
                db_path_obj.to_str().ok_or_else(|| eyre!("failed to parse DB path"))?,
                indexed_block_hash_receiver,
            ))
        })
        .join()
        .map_err(|_| eyre!("failed to join ShadowRpc thread"))??;

        // Merge the ShadowRpc into the reth context, which will make the API available.
        ctx.modules
            .merge_configured(shadow_rpc.into_rpc())
            .map_err(|e| eyre!("failed to extend w/ ShadowRpc: {e}"))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use jsonrpsee::rpc_params;
    use reth_primitives::{hex, Block, Header};
    use reth_provider::test_utils::MockEthProvider;
    use shadow_reth_common::{ShadowLog, ToLowerHex};

    use crate::{
        apis::{AddressRepresentation, GetLogsParameters, RpcLog, SubscribeParameters},
        ShadowRpc, ShadowRpcApiServer,
    };

    #[tokio::test]
    async fn test_shadow_subscribe() {
        let mock_provider = MockEthProvider::default();

        let first_block = Block {
            header: Header { number: 18870000, ..Default::default() },
            ..Default::default()
        };
        let first_block_hash = first_block.hash_slow();

        let last_block = Block {
            header: Header { number: 18870001, ..Default::default() },
            ..Default::default()
        };
        let last_block_hash = last_block.hash_slow();

        mock_provider.extend_blocks([
            (first_block_hash, first_block.clone()),
            (last_block_hash, last_block.clone()),
        ]);

        let (tx, rx) = tokio::sync::broadcast::channel(1);

        let rpc = ShadowRpc::new(mock_provider, ":memory:", rx).await.unwrap();

        let logs = vec![
            ShadowLog {
                address: "0x0fbc0a9be1e87391ed2c7d2bb275bec02f53241f".to_string(),
                block_hash: "0x4131d538cf705c267da7f448ec7460b177f40d28115ad290ba6a1fd734afe280"
                    .to_string(),
                block_log_index: 0,
                block_number: 18870000,
                block_timestamp: 1703595263,
                transaction_index: 167,
                transaction_hash: "0x8bf2361656e0ea6f338ad17ac3cd616f8eea9bb17e1afa1580802e9d3231c203"
                    .to_string(),
                transaction_log_index: 26,
                removed: false,
                data: Some("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000049dc9ce34ad2a2177480000000000000000000000000000000000000000000000000432f754f7158ad80000000000000000000000000000000000000000000000000000000000000000".to_string()),
                topic_0: Some("0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822".to_string()),
                topic_1: Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()),
                topic_2: Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()),
                topic_3: None,
            },
            ShadowLog {
                address: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                block_hash: last_block_hash.to_string(),
                block_log_index: 0,
                block_number: 18870001,
                block_timestamp: 1703595275,
                transaction_index: 2,
                transaction_hash: "0xd02dc650cc9a34def3d7a78808a36a8cb2e292613c2989f4313155e8e4af9b0f".to_string(),
                transaction_log_index: 0,
                removed: false,
                data: Some("0x0000000000000000000000000000000000000000000000001bc16d674ec80000".to_string()),
                topic_0: Some("0xe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c".to_string()),
                topic_1: Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()),
                topic_2: None,
                topic_3: None,
            },
        ];

        // Keep a clone of the log we expect to receive via the subscription for assert
        let expected_log = RpcLog::from(logs[1].clone());
        rpc.sqlite_manager.bulk_insert_into_shadow_log_table(logs).await.unwrap();

        let params = SubscribeParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0fbc0a9be1e87391ed2c7d2bb275bec02f53241f".to_string(),
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                "0xc55126051b22ebb829d00368f4b12bde432de5da".to_string(),
            ])),
            topics: None,
        };

        // Create a subscription on `shadow_subscribe`
        let mut sub = rpc
            .into_rpc()
            .subscribe_unbounded("shadow_subscribe", rpc_params!(params))
            .await
            .unwrap();

        // Send the last block hash to the rpc receiver to mock the exex indexing the block
        tx.send(last_block_hash.to_lower_hex()).expect("failed to send block hash");

        // Receive the RPC log from the subscription
        let (result, _id) = sub.next::<RpcLog>().await.unwrap().unwrap();

        assert_eq!(result, expected_log);
    }

    #[tokio::test]
    async fn test_shadow_get_logs() {
        let mock_provider = MockEthProvider::default();

        let first_block = Block {
            header: Header { number: 18870000, ..Default::default() },
            ..Default::default()
        };
        let first_block_hash = first_block.hash_slow();

        let last_block = Block {
            header: Header { number: 18870001, ..Default::default() },
            ..Default::default()
        };
        let last_block_hash = last_block.hash_slow();

        mock_provider.extend_blocks([
            (first_block_hash, first_block.clone()),
            (last_block_hash, last_block.clone()),
        ]);

        let (_, rx) = tokio::sync::broadcast::channel(1);

        let rpc = ShadowRpc::new(mock_provider, ":memory:", rx).await.unwrap();

        let logs = vec![
            ShadowLog {
                address: "0x0fbc0a9be1e87391ed2c7d2bb275bec02f53241f".to_string(),
                block_hash: "0x4131d538cf705c267da7f448ec7460b177f40d28115ad290ba6a1fd734afe280"
                    .to_string(),
                block_log_index: 0,
                block_number: 18870000,
                block_timestamp: 1703595263,
                transaction_index: 167,
                transaction_hash: "0x8bf2361656e0ea6f338ad17ac3cd616f8eea9bb17e1afa1580802e9d3231c203"
                    .to_string(),
                transaction_log_index: 26,
                removed: false,
                data: Some("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000049dc9ce34ad2a2177480000000000000000000000000000000000000000000000000432f754f7158ad80000000000000000000000000000000000000000000000000000000000000000".to_string()),
                topic_0: Some("0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822".to_string()),
                topic_1: Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()),
                topic_2: Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()),
                topic_3: None,
            },
            ShadowLog {
                address: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                block_hash: "0x3cac643a6a1af584681a6a6dc632cd110a479c9c642e2da92b73fefb45739165".to_string(),
                block_log_index: 0,
                block_number: 18870001,
                block_timestamp: 1703595275,
                transaction_index: 2,
                transaction_hash: "0xd02dc650cc9a34def3d7a78808a36a8cb2e292613c2989f4313155e8e4af9b0f".to_string(),
                transaction_log_index: 0,
                removed: false,
                data: Some("0x0000000000000000000000000000000000000000000000001bc16d674ec80000".to_string()),
                topic_0: Some("0xe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c".to_string()),
                topic_1: Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()),
                topic_2: None,
                topic_3: None,
            },
        ];

        rpc.sqlite_manager.bulk_insert_into_shadow_log_table(logs).await.unwrap();

        let params = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0fbc0a9be1e87391ed2c7d2bb275bec02f53241f".to_string(),
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                "0xc55126051b22ebb829d00368f4b12bde432de5da".to_string(),
            ])),
            block_hash: None,
            from_block: Some("0x11feef0".to_string()),
            to_block: Some("0x11feef1".to_string()),
            topics: None,
        };

        let resp = rpc.get_logs(params).await.unwrap();

        let expected = vec![
            RpcLog {
                address: "0x0fbc0a9be1e87391ed2c7d2bb275bec02f53241f".to_string(),
                block_hash: "0x4131d538cf705c267da7f448ec7460b177f40d28115ad290ba6a1fd734afe280".to_string(),
                block_number: hex::encode(18870000u64.to_be_bytes()),
                data: Some("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000049dc9ce34ad2a2177480000000000000000000000000000000000000000000000000432f754f7158ad80000000000000000000000000000000000000000000000000000000000000000".to_string()),
                log_index: 0u64.to_string(),
                removed: false,
                topics: [Some("0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822".to_string()), Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()), Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()), None],
                transaction_hash: "0x8bf2361656e0ea6f338ad17ac3cd616f8eea9bb17e1afa1580802e9d3231c203".to_string(),
                transaction_index: 167u64.to_string()
            },
            RpcLog {
                address: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                block_hash: "0x3cac643a6a1af584681a6a6dc632cd110a479c9c642e2da92b73fefb45739165".to_string(),
                block_number: hex::encode(18870001u64.to_be_bytes()),
                data: Some("0x0000000000000000000000000000000000000000000000001bc16d674ec80000".to_string()),
                log_index: 0u64.to_string(),
                removed: false,
                topics: [Some("0xe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c".to_string()), Some("0x0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad".to_string()), None, None],
                transaction_hash: "0xd02dc650cc9a34def3d7a78808a36a8cb2e292613c2989f4313155e8e4af9b0f".to_string(),
                transaction_index: 2u64.to_string()
            }
        ];

        assert_eq!(resp, expected);
    }
}
