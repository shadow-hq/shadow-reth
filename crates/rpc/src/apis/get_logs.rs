//! Contains logic for a shadow RPC equivalent of `eth_getLogs`.

use super::{AddressRepresentation, RpcLog};
use jsonrpsee::core::RpcResult;
use reth_provider::{BlockNumReader, BlockReaderIdExt};
use serde::{Deserialize, Serialize};

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

pub(crate) async fn get_logs<P>(
    rpc: &ShadowRpc<P>,
    params: GetLogsParameters,
) -> RpcResult<Vec<RpcLog>>
where
    P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
{
    let validated_param_objs =
        ValidatedQueryParams::from_get_logs_parameters(&rpc.provider, params)?;

    let mut results: Vec<RpcLog> = vec![];
    for query_params in [validated_param_objs] {
        let intermediate_results = exec_query(query_params, &rpc.sqlite_manager.pool).await?;
        let mut result = intermediate_results
            .into_iter()
            .map(RpcLog::from)
            .collect::<Vec<RpcLog>>();
        results.append(&mut result);
    }

    Ok(results)
}
