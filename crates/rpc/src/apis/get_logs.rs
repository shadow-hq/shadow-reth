//! Contains logic for a shadow RPC equivalent of `eth_getLogs`.

use super::AddressRepresentation;
use jsonrpsee::core::RpcResult;
use reth_primitives::hex;
use reth_provider::{BlockNumReader, BlockReaderIdExt};
use serde::{Deserialize, Serialize};
use shadow_reth_common::ShadowLog;

use crate::{
    shadow_logs_query::{exec_query, ValidatedQueryParams},
    ShadowRpc,
};

/// Unvalidated parameters for `shadow_getLogs` RPC requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLogsParameters {
    /// Contains contract addresses from which logs should originate.
    pub address: Option<AddressRepresentation>,
    /// Hash of block from which logs should originate. Using this field is equivalent
    /// to passing identical values for `fromBlock` and `toBlock`.
    pub block_hash: Option<String>,
    /// Start of block range from which logs should originate.
    pub from_block: Option<String>,
    /// End of block range from which logs should originate.
    pub to_block: Option<String>,
    /// Array of 32-byte data topics.
    pub topics: Option<Vec<String>>,
}

/// Inner result type for `shadow_getLogs` RPC responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLogsResult {
    /// Contract address from which the log originated.
    pub address: String,
    /// Hash of block from which the log originated.
    pub block_hash: String,
    /// Block number from which the log originated.
    pub block_number: String,
    /// Contains one or more 32-byte non-indexed arguments of the log.
    pub data: Option<String>,
    /// Integer of the log index in the containing block.
    pub log_index: String,
    /// Indicates whether the log has been removed from the canonical chain.
    pub removed: bool,
    /// Array of topics.
    pub topics: [Option<String>; 4],
    /// Hash of transaction from which the log originated.
    pub transaction_hash: String,
    /// Integer of the transaction index position from which the log originated.
    pub transaction_index: String,
}

impl From<ShadowLog> for GetLogsResult {
    fn from(value: ShadowLog) -> Self {
        Self {
            address: value.address,
            block_hash: value.block_hash,
            block_number: hex::encode(value.block_number.to_be_bytes()),
            data: value.data,
            log_index: value.block_log_index.to_string(),
            removed: value.removed,
            topics: [value.topic_0, value.topic_1, value.topic_2, value.topic_3],
            transaction_hash: value.transaction_hash,
            transaction_index: value.transaction_index.to_string(),
        }
    }
}

pub(crate) async fn get_logs<P>(
    rpc: &ShadowRpc<P>,
    params: GetLogsParameters,
) -> RpcResult<Vec<GetLogsResult>>
where
    P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
{
    let validated_param_objs =
        ValidatedQueryParams::from_get_logs_parameters(&rpc.provider, params)?;

    let mut results: Vec<GetLogsResult> = vec![];
    for query_params in [validated_param_objs] {
        let intermediate_results = exec_query(query_params, &rpc.sqlite_manager.pool).await?;
        let mut result = intermediate_results
            .into_iter()
            .map(GetLogsResult::from)
            .collect::<Vec<GetLogsResult>>();
        results.append(&mut result);
    }

    Ok(results)
}
