use std::{num::ParseIntError, str::FromStr};

use jsonrpsee::{
    core::{async_trait, RpcResult},
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
};
use reth::providers::{BlockNumReader, BlockReaderIdExt};
use reth_primitives::{revm_primitives::FixedBytes, Address, BlockNumberOrTag};
use serde::{Deserialize, Serialize};
use shadow_reth_common::ShadowLog;

use crate::{ShadowRpc, ShadowRpcApiServer};

/// Unvalidated parameters for `shadow_getLogs` RPC requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetLogsParameters {
    /// Contains contract addresses from which logs should originate.
    pub address: Vec<String>,
    /// Hash of block from which logs should originate. Using this field is equivalent
    /// to passing identical values for `fromBlock` and `toBlock`.
    #[serde(rename = "blockHash")]
    pub block_hash: Option<String>,
    /// Start of block range from which logs should originate.
    #[serde(rename = "fromBlock")]
    pub from_block: Option<String>,
    /// End of block range from which logs should originate.
    #[serde(rename = "toBlock")]
    pub to_block: Option<String>,
    /// Array of 32-byte data topics.
    pub topics: Option<Vec<String>>,
}

/// Inner result type for `shadow_getLogs` RPC responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetLogsResult {
    /// Contract address from which the log originated.
    pub address: String,
    /// Hash of block from which the log originated.
    #[serde(rename = "blockHash")]
    pub block_hash: String,
    /// Block number from which the log originated.
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    /// Contains one or more 32-byte non-indexed arguments of the log.
    pub data: Option<String>,
    /// Integer of the log index in the containing block.
    #[serde(rename = "logIndex")]
    pub log_index: String,
    /// Indicates whether the log has been removed from the canonical chain.
    pub removed: bool,
    /// Array of topics.
    pub topics: [Option<String>; 4],
    /// Hash of transaction from which the log originated.
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    /// Integer of the transaction index position from which the log originated.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: String,
}

/// Helper type for ease of use in converting rows from the `shadow_getLogs`
/// query into the `GetLogsResult` type which is used in `GetLogsResponse`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct RawGetLogsRow {
    /// Address from which a log originated.
    pub(crate) address: Vec<u8>,
    /// Hash of bock from which a log orignated.
    pub(crate) block_hash: Vec<u8>,
    /// Integer of the log index position in its containing block.
    pub(crate) block_log_index: String,
    /// Block number from which a log originated.
    pub(crate) block_number: String,
    /// Timestamp of block from which the log originated.
    pub(crate) block_timestamp: String,
    /// Contains one or more 32-byte non-indexed arguments of the log.
    pub(crate) data: Option<Vec<u8>>,
    /// Indicates whether a log was removed from the canonical chain.
    pub(crate) removed: bool,
    /// Hash of event signature.
    pub(crate) topic_0: Option<Vec<u8>>,
    /// Additional topic #1.
    pub(crate) topic_1: Option<Vec<u8>>,
    /// Additional topic #2.
    pub(crate) topic_2: Option<Vec<u8>>,
    /// Additional topic #3.
    pub(crate) topic_3: Option<Vec<u8>>,
    /// Hash of the transaction from which a log originated.
    pub(crate) transaction_hash: Vec<u8>,
    /// Integer of the transaction index position in a log's containing block.
    pub(crate) transaction_index: String,
    /// Integer of the log index position within a transaction.
    pub(crate) transaction_log_index: String,
}

/// Validated query parameter object. Instances are considered to be well-formed
/// and are used in query construction and execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedQueryParams {
    /// Start of block range from which logs will be filtered.
    pub(crate) from_block: u64,
    /// End of block range from which logs will be filtered.
    pub(crate) to_block: u64,
    /// Set of addresses from which logs will be filtered.
    pub(crate) addresses: Vec<String>,
    /// Set of log topics.
    pub(crate) topics: [Option<String>; 4],
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

impl TryFrom<RawGetLogsRow> for ShadowLog {
    type Error = ParseIntError;

    fn try_from(value: RawGetLogsRow) -> Result<Self, Self::Error> {
        Ok(Self {
            address: format!("0x{}", hex::encode(value.address)),
            block_hash: format!("0x{}", hex::encode(value.block_hash)),
            block_log_index: u64::from_str(&value.block_log_index)?,
            block_number: u64::from_str(&value.block_number)?,
            block_timestamp: u64::from_str(&value.block_timestamp)?,
            transaction_index: u64::from_str(&value.transaction_index)?,
            transaction_hash: format!("0x{}", hex::encode(value.transaction_hash)),
            transaction_log_index: u64::from_str(&value.transaction_log_index)?,
            removed: value.removed,
            data: value.data.map(|d| format!("0x{}", hex::encode(d))),
            topic_0: value.topic_0.map(|t| format!("0x{}", hex::encode(t))),
            topic_1: value.topic_1.map(|t| format!("0x{}", hex::encode(t))),
            topic_2: value.topic_2.map(|t| format!("0x{}", hex::encode(t))),
            topic_3: value.topic_3.map(|t| format!("0x{}", hex::encode(t))),
        })
    }
}

#[async_trait]
impl<P> ShadowRpcApiServer for ShadowRpc<P>
where
    P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
{
    #[doc = "Returns shadow logs."]
    // todo: move to common sqlite module
    async fn get_logs(&self, params: GetLogsParameters) -> RpcResult<Vec<GetLogsResult>> {
        let base_stmt = "
            SELECT
                address,
                block_hash,
                block_log_index,
                block_number,
                block_timestamp,
                data,
                removed,
                topic_0,
                topic_1,
                topic_2,
                topic_3,
                transaction_hash,
                transaction_index,
                transaction_log_index
            FROM shadow_logs
        ";

        let validated_param_objs = ValidatedQueryParams::new(&self.provider, params)?;

        let mut results: Vec<GetLogsResult> = vec![];
        for query_params in [validated_param_objs] {
            let sql = format!("{base_stmt} {query_params}");
            let raw_rows: Vec<RawGetLogsRow> = sqlx::query_as(&sql)
                .fetch_all(&self.sqlite_manager.pool)
                .await
                .map_err(|e| ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None))?;
            let intermediate_results = raw_rows
                .into_iter()
                .map(ShadowLog::try_from)
                .collect::<Result<Vec<ShadowLog>, ParseIntError>>()
                .map_err(|e| ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None))?;
            let mut result = intermediate_results
                .into_iter()
                .map(GetLogsResult::from)
                .collect::<Vec<GetLogsResult>>();
            results.append(&mut result);
        }

        Ok(results)
    }
}

impl ValidatedQueryParams {
    fn new(
        provider: &(impl BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static),
        params: GetLogsParameters,
    ) -> RpcResult<Self> {
        let address = params
            .address
            .into_iter()
            .map(|addr| {
                addr.parse::<Address>()
                    .map_err(|e| ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None))
            })
            .collect::<RpcResult<Vec<Address>>>()?
            .into_iter()
            .map(|a| a.to_string())
            .collect::<Vec<String>>();

        let (from_block, to_block) = match (params.block_hash, params.from_block, params.to_block) {
            (None, None, None) => {
                let num = match provider.block_by_number_or_tag(BlockNumberOrTag::Latest) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            "No block found for block number or tag: latest",
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                (num, num)
            }
            (None, None, Some(to_block)) => {
                let from = match provider.block_by_number_or_tag(BlockNumberOrTag::Latest) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            "No block found for block number or tag: latest",
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                let to_tag = BlockNumberOrTag::from_str(&to_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let to = if let BlockNumberOrTag::Number(n) = to_tag {
                    n
                } else {
                    match provider.block_by_number_or_tag(to_tag) {
                        Ok(Some(b)) => b.number,
                        Ok(None) => {
                            return Err(ErrorObject::owned::<()>(
                                -1,
                                format!("No block found for block number or tag: {to_tag}"),
                                None,
                            ))
                        }
                        Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                    }
                };
                (from, to)
            }
            (None, Some(from_block), None) => {
                let from_tag = BlockNumberOrTag::from_str(&from_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let from = if let BlockNumberOrTag::Number(n) = from_tag {
                    n
                } else {
                    match provider.block_by_number_or_tag(from_tag) {
                        Ok(Some(b)) => b.number,
                        Ok(None) => {
                            return Err(ErrorObject::owned::<()>(
                                -1,
                                format!("No block found for block number or tag: {from_tag}"),
                                None,
                            ))
                        }
                        Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                    }
                };
                let to = match provider.block_by_number_or_tag(BlockNumberOrTag::Latest) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            "No block found for block number or tag: latest",
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                (from, to)
            }
            (None, Some(from_block), Some(to_block)) => {
                let from_tag = BlockNumberOrTag::from_str(&from_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let from = if let BlockNumberOrTag::Number(n) = from_tag {
                    n
                } else {
                    match provider.block_by_number_or_tag(from_tag) {
                        Ok(Some(b)) => b.number,
                        Ok(None) => {
                            return Err(ErrorObject::owned::<()>(
                                -1,
                                format!("No block found for block number or tag: {from_tag}"),
                                None,
                            ))
                        }
                        Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                    }
                };
                let to_tag = BlockNumberOrTag::from_str(&to_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let to = if let BlockNumberOrTag::Number(n) = to_tag {
                    n
                } else {
                    match provider.block_by_number_or_tag(to_tag) {
                        Ok(Some(b)) => b.number,
                        Ok(None) => {
                            return Err(ErrorObject::owned::<()>(
                                -1,
                                format!("No block found for block number or tag: {to_tag}"),
                                None,
                            ))
                        }
                        Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                    }
                };

                (from, to)
            }
            (Some(block_hash), None, None) => {
                let num = match provider.block_by_hash(
                    FixedBytes::from_str(&block_hash)
                        .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?,
                ) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            format!("No block found for block hash: {block_hash}"),
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                (num, num)
            }
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) => return Err(ErrorObject::owned::<()>(
                -32001,
                "Parameters fromBlock and toBlock cannot be used if blockHash parameter is present",
                None,
            )),
        };

        let topics = if let Some(t_list) = params.topics {
            if t_list.len() > 4 {
                return Err(ErrorObject::owned::<()>(
                    32002,
                    "Only up to four topics are allowed",
                    None,
                ));
            } else {
                let mut topics: [Option<String>; 4] = [None, None, None, None];

                for (idx, topic) in t_list.into_iter().enumerate() {
                    topics[idx] = Some(topic);
                }

                topics
            }
        } else {
            [None, None, None, None]
        };

        Ok(ValidatedQueryParams { from_block, to_block, addresses: address, topics })
    }
}

impl std::fmt::Display for ValidatedQueryParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let address_clause = if !self.addresses.is_empty() {
            Some(format!(
                "address IN ({})",
                self.addresses
                    .iter()
                    .map(|addr| format!("X'{}'", &addr[2..]))
                    .collect::<Vec<String>>()
                    .join(", ")
            ))
        } else {
            None
        };

        let block_range_clause =
            Some(format!("block_number BETWEEN {} AND {}", self.from_block, self.to_block));

        let topic_0_clause = self.topics[0].as_ref().map(|t| format!("topic_0 = X'{}'", &t[2..]));

        let topic_1_clause = self.topics[1].as_ref().map(|t| format!("topic_1 = X'{}'", &t[2..]));

        let topic_2_clause = self.topics[2].as_ref().map(|t| format!("topic_2 = X'{}'", &t[2..]));

        let topic_3_clause = self.topics[3].as_ref().map(|t| format!("topic_3 = X'{}'", &t[2..]));

        let clauses = [
            address_clause,
            block_range_clause,
            topic_0_clause,
            topic_1_clause,
            topic_2_clause,
            topic_3_clause,
        ];

        let filtered_clauses = clauses.into_iter().flatten().collect::<Vec<String>>();

        if !filtered_clauses.is_empty() {
            write!(f, "WHERE {}", filtered_clauses.join(" AND "))
        } else {
            write!(f, "")
        }
    }
}

#[cfg(test)]
mod tests {
    use reth::providers::test_utils::MockEthProvider;
    use reth_primitives::{Block, Header};

    use super::ValidatedQueryParams;

    use super::GetLogsParameters;

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
            address: vec!["0x0000000000000000000000000000000000000000".to_string()],
            block_hash: Some(last_block_hash.to_string()),
            from_block: None,
            to_block: None,
            topics: None,
        };

        assert!(ValidatedQueryParams::new(&mock_provider, params_with_block_hash).is_ok());

        let params_with_defaults = GetLogsParameters {
            address: vec!["0x0000000000000000000000000000000000000000".to_string()],
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
            address: vec!["0x0000000000000000000000000000000000000000".to_string()],
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

        let params_with_invalid_address = GetLogsParameters {
            address: vec!["0x123".to_string()],
            block_hash: Some(first_block_hash.to_string()),
            from_block: Some(first_block_hash.to_string()),
            to_block: Some(last_block_hash.to_string()),
            topics: None,
        };
        assert!(ValidatedQueryParams::new(&mock_provider, params_with_invalid_address).is_err());

        let params_with_block_hash_and_range = GetLogsParameters {
            address: vec!["0x0000000000000000000000000000000000000000".to_string()],
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
