use eyre::Result;
use reth_evm_ethereum::EthEvmConfig;
use reth_node_api::{ConfigureEvm, ConfigureEvmEnv};
use reth_primitives::{
    revm::env::fill_tx_env, Block, BlockWithSenders, ChainSpec, Header, TransactionSigned,
};
use reth_provider::StateProvider;
use reth_tracing::tracing::{debug, error};
use revm::{
    db::{states::bundle_state::BundleRetention, State},
    DatabaseCommit, Evm, StateBuilder,
};
use revm_primitives::{
    CfgEnvWithHandlerCfg, EVMError, ExecutionResult, HashMap, ResultAndState, U256,
};
use shadow_reth_common::{ShadowLog, ToLowerHex};
use std::sync::Arc;

use crate::db::ShadowDatabase;

/// A block executor which shadows certain contracts, overriding their bytecode.
/// Uses the [`ShadowDatabase`] to shadow the contracts from the provided `shadow.json`.
#[derive(Debug)]
pub(crate) struct ShadowExecutor<'a, DB: StateProvider> {
    evm: Evm<'a, (), State<ShadowDatabase<DB>>>,
}

/// Holds the result of a block execution, as well as important
/// information about the block and transactions executed.
#[derive(Debug)]
pub(crate) struct ExecutedBlock {
    block: Block,
    results: HashMap<TransactionSigned, ExecutionResult>,
}

impl ExecutedBlock {
    /// Returns [`ShadowLog`]s from the executed block.
    pub(crate) fn logs(&self) -> Vec<ShadowLog> {
        self.results
            .clone()
            .into_iter()
            .enumerate()
            .flat_map(|(transaction_index, (transaction, result))| {
                result.into_logs().into_iter().map(move |log| ShadowLog {
                    address: log.address.to_lower_hex(),
                    block_hash: self.block.hash_slow().to_lower_hex(),
                    block_log_index: 0, // TODO: what do we do about log index fields?
                    block_number: self.block.number,
                    block_timestamp: self.block.timestamp,
                    transaction_index: transaction_index as u64,
                    transaction_hash: transaction.hash.to_lower_hex(),
                    transaction_log_index: 0, // TODO: what do we do about log index fields?
                    removed: false,
                    data: Some(log.data.data.to_lower_hex()),
                    topic_0: log.topics().first().map(|t| t.to_lower_hex()),
                    topic_1: log.topics().get(1).map(|t| t.to_lower_hex()),
                    topic_2: log.topics().get(2).map(|t| t.to_lower_hex()),
                    topic_3: log.topics().get(3).map(|t| t.to_lower_hex()),
                })
            })
            .collect()
    }
}

impl<'a, DB: StateProvider> ShadowExecutor<'a, DB> {
    /// Creates a new instance of the ShadowExecutor
    pub(crate) fn new(
        config: &'a EthEvmConfig,
        db: ShadowDatabase<DB>,
        chain: Arc<ChainSpec>,
        header: &Header,
    ) -> Self {
        let evm = configure_evm(config, db, chain, header);
        Self { evm }
    }

    #[allow(clippy::mutable_key_type)]
    /// Executes a single block (without verifying them) and returns their [`ExecutionResult`]s
    /// within a [`ExecutedBlock`].
    pub(crate) fn execute_one(&mut self, block: BlockWithSenders) -> Result<ExecutedBlock> {
        // Update the base fee per gas to 0 to avoid any gas fees.
        // This will allow us to execute shadow bytecode without running out of gas.
        let mut block = block;
        block.block.header.base_fee_per_gas = Some(0);

        let transactions = block.clone().into_transactions();
        let mut results = HashMap::with_capacity(transactions.len());

        if !transactions.is_empty() {
            for transaction in transactions {
                // Recover the sender of the transaction.
                let sender = match transaction.recover_signer() {
                    Some(sender) => sender,
                    None => {
                        debug!(?transaction, "Skipping transaction with invalid signature");
                        continue;
                    }
                };

                // Execute the transaction, do not verify it since we're shadowing certain contracts
                // which may not be valid.
                fill_tx_env(self.evm.tx_mut(), &transaction, sender);
                let ResultAndState { result, state } = match self.evm.transact_preverified() {
                    Ok(result) => result,
                    Err(err) => match err {
                        EVMError::Transaction(err) => {
                            debug!(%err, ?transaction, "Skipping invalid transaction");
                            continue;
                        }
                        err => {
                            error!(%err, ?transaction, "Fatal error during transaction execution");
                            continue;
                        }
                    },
                };

                // Commit the state changes to the shadowed database, and store the result of the
                // transaction.
                self.evm.db_mut().commit(state);
                results.insert(transaction, result);
            }

            // Merge the transitions into the shadowed database.
            self.evm.db_mut().merge_transitions(BundleRetention::Reverts);
        }

        Ok(ExecutedBlock { block: block.block, results })
    }
}

/// Configure EVM with the given database and header.
fn configure_evm<'a, DB: StateProvider>(
    config: &'a EthEvmConfig,
    db: ShadowDatabase<DB>,
    chain: Arc<ChainSpec>,
    header: &Header,
) -> Evm<'a, (), State<ShadowDatabase<DB>>> {
    let mut evm = config.evm(StateBuilder::new_with_database(db).with_bundle_update().build());
    let mut cfg = CfgEnvWithHandlerCfg::new_with_spec_id(evm.cfg().clone(), evm.spec_id());
    EthEvmConfig::fill_cfg_and_block_env(&mut cfg, evm.block_mut(), &chain, header, U256::ZERO);
    *evm.cfg_mut() = cfg.cfg_env;

    evm
}
