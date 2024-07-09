/// A shadow log entry.
#[derive(Debug, Clone)]
pub struct ShadowLog {
    /// Contract address from which the log originated.
    pub address: String,
    /// Hash of block from which the log originated.
    pub block_hash: String,
    /// Integer of the log index in the containing block.
    pub block_log_index: u64,
    /// Block number from which the log originated.
    pub block_number: u64,
    /// Timestamp of block from which the log originated.
    pub block_timestamp: u64,
    /// Integer of the transaction index position from which the log originated.
    pub transaction_index: u64,
    /// Hash of transaction from which the log originated.
    pub transaction_hash: String,
    /// Integer of the log index in the containing transaction.
    pub transaction_log_index: u64,
    /// Indicates whether the log has been removed from the canonical chain.
    pub removed: bool,
    /// Contains one or more 32-byte non-indexed arguments of the log.
    pub data: Option<String>,
    /// Topic 0.
    pub topic_0: Option<String>,
    /// Topic 1.
    pub topic_1: Option<String>,
    /// Topic 2.
    pub topic_2: Option<String>,
    /// Topic 3.
    pub topic_3: Option<String>,
}
