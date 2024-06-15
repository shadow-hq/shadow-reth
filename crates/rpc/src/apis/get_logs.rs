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

#[cfg(test)]
mod tests {
    use reth_primitives::{Block, Header};
    use reth_provider::test_utils::MockEthProvider;

    use super::ValidatedQueryParams;

    use super::{AddressRepresentation, GetLogsParameters};

    #[tokio::test]
    async fn test_query_param_validation() {
        let mock_provider = MockEthProvider::default();

        let first_block =
            Block { header: Header { number: 0, ..Default::default() }, ..Default::default() };
        let first_block_hash = first_block.hash_slow();

        let last_block =
            Block { header: Header { number: 10, ..Default::default() }, ..Default::default() };
        let last_block_hash = last_block.hash_slow();

        mock_provider
            .extend_blocks([(first_block_hash, first_block), (last_block_hash, last_block)]);

        let params_with_block_hash = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0000000000000000000000000000000000000000".to_string(),
            ])),
            block_hash: Some(last_block_hash.to_string()),
            from_block: None,
            to_block: None,
            topics: None,
        };

        assert!(ValidatedQueryParams::new(&mock_provider, params_with_block_hash).is_ok());

        let params_with_defaults = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0000000000000000000000000000000000000000".to_string(),
            ])),
            block_hash: None,
            from_block: None,
            to_block: None,
            topics: None,
        };

        let validated = ValidatedQueryParams::new(&mock_provider, params_with_defaults);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x0000000000000000000000000000000000000000".to_string()],
                from_block: 10,
                to_block: 10,
                topics: [None, None, None, None]
            }
        );

        let params_with_block_tags = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0000000000000000000000000000000000000000".to_string(),
            ])),
            block_hash: None,
            from_block: Some("earliest".to_string()),
            to_block: Some("latest".to_string()),
            topics: None,
        };
        let validated = ValidatedQueryParams::new(&mock_provider, params_with_block_tags);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x0000000000000000000000000000000000000000".to_string()],
                from_block: 0,
                to_block: 10,
                topics: [None, None, None, None]
            }
        );

        let params_with_non_array_address = GetLogsParameters {
            address: Some(AddressRepresentation::String(
                "0x0000000000000000000000000000000000000000".to_string(),
            )),
            block_hash: None,
            from_block: Some("earliest".to_string()),
            to_block: Some("latest".to_string()),
            topics: None,
        };
        let validated = ValidatedQueryParams::new(&mock_provider, params_with_non_array_address);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x0000000000000000000000000000000000000000".to_string()],
                from_block: 0,
                to_block: 10,
                topics: [None, None, None, None]
            }
        );

        let params_with_bytes_as_address = GetLogsParameters {
            address: Some(AddressRepresentation::Bytes([
                192, 42, 170, 57, 178, 35, 254, 141, 10, 14, 92, 79, 39, 234, 217, 8, 60, 117, 108,
                194,
            ])),
            block_hash: None,
            from_block: Some("earliest".to_string()),
            to_block: Some("latest".to_string()),
            topics: None,
        };
        let validated = ValidatedQueryParams::new(&mock_provider, params_with_bytes_as_address);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string()],
                from_block: 0,
                to_block: 10,
                topics: [None, None, None, None]
            }
        );

        let params_with_invalid_address = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0000000000000000000000000000000000000000".to_string(),
            ])),
            block_hash: Some(first_block_hash.to_string()),
            from_block: Some(first_block_hash.to_string()),
            to_block: Some(last_block_hash.to_string()),
            topics: None,
        };
        assert!(ValidatedQueryParams::new(&mock_provider, params_with_invalid_address).is_err());

        let params_with_block_hash_and_range = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0000000000000000000000000000000000000000".to_string(),
            ])),
            block_hash: Some(first_block_hash.to_string()),
            from_block: Some(first_block_hash.to_string()),
            to_block: Some(last_block_hash.to_string()),
            topics: None,
        };
        assert!(
            ValidatedQueryParams::new(&mock_provider, params_with_block_hash_and_range).is_err()
        );
    }
}
