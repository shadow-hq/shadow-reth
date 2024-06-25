use std::{num::ParseIntError, str::FromStr};

use jsonrpsee::{
    core::RpcResult,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
};
use reth_primitives::{hex, Address, BlockNumberOrTag, B256};
use reth_provider::{BlockNumReader, BlockReaderIdExt};
use shadow_reth_common::ShadowLog;
use sqlx::{Pool, Sqlite};

use crate::apis::{AddressRepresentation, GetLogsParameters, SubscribeParameters};

pub(crate) async fn exec_query(
    query_params: ValidatedQueryParams,
    pool: &Pool<Sqlite>,
) -> RpcResult<Vec<ShadowLog>> {
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
        FROM shadow_logs";
    let sql = format!("{base_stmt} {query_params}");
    let raw_rows: Vec<RawGetLogsRow> = sqlx::query_as(&sql)
        .fetch_all(pool)
        .await
        .map_err(|e| ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None))?;

    raw_rows
        .into_iter()
        .map(ShadowLog::try_from)
        .collect::<Result<Vec<ShadowLog>, ParseIntError>>()
        .map_err(|e| ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidatedBlockIdParam {
    /// Block hash from which logs will be filtered.
    BlockHash(String),
    /// Start and end of block range from which logs will be filtered.
    BlockRange(u64, u64),
}

/// Validated query parameter object. Instances are considered to be well-formed
/// and are used in query construction and execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedQueryParams {
    pub(crate) block_id: ValidatedBlockIdParam,
    /// Set of addresses from which logs will be filtered.
    pub(crate) addresses: Vec<String>,
    /// Set of log topics.
    pub(crate) topics: [Option<String>; 4],
}

impl ValidatedQueryParams {
    fn validate_addresses(address: Option<AddressRepresentation>) -> RpcResult<Vec<String>> {
        let v = if let Some(addr_repr) = address {
            match addr_repr {
                AddressRepresentation::String(addr) => {
                    let parsed = addr
                        .parse::<Address>()
                        .map_err(|e| {
                            ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None)
                        })?
                        .to_string();
                    vec![parsed]
                }
                AddressRepresentation::ArrayOfStrings(array) => array
                    .into_iter()
                    .map(|addr| {
                        addr.parse::<Address>().map_err(|e| {
                            ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None)
                        })
                    })
                    .collect::<RpcResult<Vec<Address>>>()?
                    .into_iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<String>>(),
                AddressRepresentation::Bytes(bytes) => {
                    vec![Address::from_slice(&bytes).to_string()]
                }
            }
        } else {
            vec![]
        };

        Ok(v)
    }

    fn validate_topics(topics: Option<Vec<String>>) -> RpcResult<[Option<String>; 4]> {
        let v = if let Some(t_list) = topics {
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

        Ok(v)
    }

    fn validate_block_id(
        provider: &(impl BlockNumReader + BlockReaderIdExt),
        block_hash: Option<String>,
        from_block: Option<String>,
        to_block: Option<String>,
        resolve_block_hash: bool,
    ) -> RpcResult<ValidatedBlockIdParam> {
        let v = match (block_hash, from_block, to_block) {
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
                ValidatedBlockIdParam::BlockRange(num, num)
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
                ValidatedBlockIdParam::BlockRange(from, to)
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
                ValidatedBlockIdParam::BlockRange(from, to)
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

                ValidatedBlockIdParam::BlockRange(from, to)
            }
            (Some(block_hash), None, None) if resolve_block_hash => {
                let num = match provider.block_by_hash(
                    B256::from_str(&block_hash)
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
                ValidatedBlockIdParam::BlockRange(num, num)
            }
            (Some(block_hash), None, None) => ValidatedBlockIdParam::BlockHash(block_hash),
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) => return Err(ErrorObject::owned::<()>(
                -32001,
                "Parameters fromBlock and toBlock cannot be used if blockHash parameter is present",
                None,
            )),
        };

        Ok(v)
    }

    pub(crate) fn from_get_logs_parameters(
        provider: &(impl BlockNumReader + BlockReaderIdExt),
        params: GetLogsParameters,
    ) -> RpcResult<Self> {
        let addresses = Self::validate_addresses(params.address)?;
        let block_id = Self::validate_block_id(
            provider,
            params.block_hash,
            params.from_block,
            params.to_block,
            true,
        )?;
        let topics = Self::validate_topics(params.topics)?;

        Ok(ValidatedQueryParams { block_id, addresses, topics })
    }

    pub(crate) fn from_subscribe_parameters(
        provider: &(impl BlockNumReader + BlockReaderIdExt),
        params: SubscribeParameters,
        block_hash: String,
    ) -> RpcResult<Self> {
        let addresses = Self::validate_addresses(params.address)?;
        let topics = Self::validate_topics(params.topics)?;
        let block_id = Self::validate_block_id(provider, Some(block_hash), None, None, false)?;

        Ok(ValidatedQueryParams { block_id, addresses, topics })
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

        let block_range_clause = match &self.block_id {
            ValidatedBlockIdParam::BlockHash(block_hash) => {
                Some(format!("block_hash = X'{}'", &block_hash[2..]))
            }
            ValidatedBlockIdParam::BlockRange(from_block, to_block) => {
                Some(format!("block_number BETWEEN {} AND {}", from_block, to_block))
            }
        };

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
    use reth_primitives::{Address, Block, BlockHash, Header};
    use reth_provider::test_utils::MockEthProvider;

    use super::{ValidatedBlockIdParam, ValidatedQueryParams};
    use crate::apis::{AddressRepresentation, GetLogsParameters, SubscribeParameters};

    #[test]
    fn test_display() {
        let mock_provider = MockEthProvider::default();

        let first_block =
            Block { header: Header { number: 0, ..Default::default() }, ..Default::default() };
        let first_block_hash = first_block.hash_slow();

        let last_block =
            Block { header: Header { number: 10, ..Default::default() }, ..Default::default() };
        let last_block_hash = last_block.hash_slow();

        mock_provider
            .extend_blocks([(first_block_hash, first_block), (last_block_hash, last_block)]);

        let subscribe_params = SubscribeParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![Address::ZERO.to_string()])),
            topics: Some(vec!["0xfoo".to_string()]),
        };

        assert_eq!(
            format!(
                "{}",
                ValidatedQueryParams::from_subscribe_parameters(
                    &mock_provider,
                    subscribe_params,
                    BlockHash::ZERO.to_string(),
                )
                .unwrap()
            ),
            "WHERE address IN (X'0000000000000000000000000000000000000000') AND block_hash = X'0000000000000000000000000000000000000000000000000000000000000000' AND topic_0 = X'foo'"
        );

        let get_logs_params = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![Address::ZERO.to_string()])),
            block_hash: Some(last_block_hash.to_string()),
            from_block: None,
            to_block: None,
            topics: Some(vec!["0xfoo".to_string()]),
        };

        assert_eq!(
            format!(
                "{}",
                ValidatedQueryParams::from_get_logs_parameters(&mock_provider, get_logs_params,)
                    .unwrap()
            ),
            "WHERE address IN (X'0000000000000000000000000000000000000000') AND block_number BETWEEN 10 AND 10 AND topic_0 = X'foo'"
        );
    }

    #[test]
    fn test_from_subscribe_parameters() {
        let mock_provider = MockEthProvider::default();

        let first_block =
            Block { header: Header { number: 0, ..Default::default() }, ..Default::default() };
        let first_block_hash = first_block.hash_slow();

        let last_block =
            Block { header: Header { number: 10, ..Default::default() }, ..Default::default() };
        let last_block_hash = last_block.hash_slow();

        mock_provider
            .extend_blocks([(first_block_hash, first_block), (last_block_hash, last_block)]);

        let params = SubscribeParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![Address::ZERO.to_string()])),
            topics: None,
        };

        assert_eq!(
            ValidatedQueryParams::from_subscribe_parameters(
                &mock_provider,
                params,
                BlockHash::ZERO.to_string(),
            )
            .unwrap(),
            ValidatedQueryParams {
                addresses: vec![Address::ZERO.to_string()],
                block_id: ValidatedBlockIdParam::BlockHash(BlockHash::ZERO.to_string()),
                topics: [None, None, None, None]
            }
        )
    }

    #[test]
    fn test_from_get_logs_parameters() {
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

        assert!(ValidatedQueryParams::from_get_logs_parameters(
            &mock_provider,
            params_with_block_hash
        )
        .is_ok());

        let params_with_defaults = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0000000000000000000000000000000000000000".to_string(),
            ])),
            block_hash: None,
            from_block: None,
            to_block: None,
            topics: None,
        };

        let validated =
            ValidatedQueryParams::from_get_logs_parameters(&mock_provider, params_with_defaults);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x0000000000000000000000000000000000000000".to_string()],
                block_id: ValidatedBlockIdParam::BlockRange(10, 10),
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
        let validated =
            ValidatedQueryParams::from_get_logs_parameters(&mock_provider, params_with_block_tags);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x0000000000000000000000000000000000000000".to_string()],
                block_id: ValidatedBlockIdParam::BlockRange(0, 10),
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
        let validated = ValidatedQueryParams::from_get_logs_parameters(
            &mock_provider,
            params_with_non_array_address,
        );

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x0000000000000000000000000000000000000000".to_string()],
                block_id: ValidatedBlockIdParam::BlockRange(0, 10),
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
        let validated = ValidatedQueryParams::from_get_logs_parameters(
            &mock_provider,
            params_with_bytes_as_address,
        );

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string()],
                block_id: ValidatedBlockIdParam::BlockRange(0, 10),
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
        assert!(ValidatedQueryParams::from_get_logs_parameters(
            &mock_provider,
            params_with_invalid_address
        )
        .is_err());

        let params_with_block_hash_and_range = GetLogsParameters {
            address: Some(AddressRepresentation::ArrayOfStrings(vec![
                "0x0000000000000000000000000000000000000000".to_string(),
            ])),
            block_hash: Some(first_block_hash.to_string()),
            from_block: Some(first_block_hash.to_string()),
            to_block: Some(last_block_hash.to_string()),
            topics: None,
        };
        assert!(ValidatedQueryParams::from_get_logs_parameters(
            &mock_provider,
            params_with_block_hash_and_range
        )
        .is_err());
    }
}
