use reth_primitives::hex;
use serde::{Deserialize, Serialize};
use shadow_reth_common::ShadowLog;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AddressRepresentation {
    ArrayOfStrings(Vec<String>),
    Bytes([u8; 20]),
    String(String),
}

/// Inner result type for `shadow_getLogs` and `shadow_subscribe` RPC responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcLog {
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

impl From<ShadowLog> for RpcLog {
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
