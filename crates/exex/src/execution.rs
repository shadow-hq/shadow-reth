use std::sync::Arc;

use eyre::Result;
use reth_evm_ethereum::EthEvmConfig;
use reth_node_api::{ConfigureEvm, ConfigureEvmEnv};
use reth_primitives::{
    revm::{config::revm_spec, env::fill_tx_env},
    Block, BlockWithSenders, ChainSpec, Head, Header, TransactionSigned,
};
use reth_provider::StateProvider;
use reth_revm::{
    db::{states::bundle_state::BundleRetention, State},
    primitives::{
        CfgEnvWithHandlerCfg, EVMError, ExecutionResult, HashMap, ResultAndState, B256, U256,
    },
    DatabaseCommit, Evm, EvmBuilder, StateBuilder,
};
use reth_tracing::tracing::{debug, error, info};
use shadow_reth_common::{ShadowLog, ToLowerHex};

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
    canonical_block_hash: B256,
    results: HashMap<TransactionSigned, ExecutionResult>,
}

impl ExecutedBlock {
    /// Returns [`ShadowLog`]s from the executed block.
    pub(crate) fn logs(&self) -> Vec<ShadowLog> {
        let mut block_log_index = 0;
        self.results
            .clone()
            .into_iter()
            .enumerate()
            .flat_map(|(transaction_index, (transaction, result))| {
                result.into_logs().into_iter().enumerate().map(
                    move |(transaction_log_index, log)| {
                        block_log_index += 1;
                        ShadowLog {
                            address: log.address.to_lower_hex(),
                            block_hash: self.canonical_block_hash.to_lower_hex(),
                            block_log_index,
                            block_number: self.block.number,
                            block_timestamp: self.block.timestamp,
                            transaction_index: transaction_index as u64,
                            transaction_hash: transaction.hash.to_lower_hex(),
                            transaction_log_index: transaction_log_index as u64,
                            removed: false,
                            data: Some(log.data.data.to_lower_hex()),
                            topic_0: log.topics().first().map(|t| t.to_lower_hex()),
                            topic_1: log.topics().get(1).map(|t| t.to_lower_hex()),
                            topic_2: log.topics().get(2).map(|t| t.to_lower_hex()),
                            topic_3: log.topics().get(3).map(|t| t.to_lower_hex()),
                        }
                    },
                )
            })
            .collect()
    }
}

impl<'a, DB: StateProvider> ShadowExecutor<'a, DB> {
    /// Creates a new instance of the ShadowExecutor
    pub(crate) fn new(db: ShadowDatabase<DB>) -> Self {
        let evm = EvmBuilder::default()
            .with_db(StateBuilder::new_with_database(db).with_bundle_update().build())
            .build();
        Self { evm }
    }

    #[allow(clippy::mutable_key_type)]
    /// Executes a single block (without verifying them) and returns their [`ExecutionResult`]s
    /// within a [`ExecutedBlock`].
    pub(crate) fn execute_one(
        &mut self,
        block: BlockWithSenders,
        chain: Arc<ChainSpec>,
    ) -> Result<ExecutedBlock> {
        // Configure the EVM for the block.
        configure_evm(&mut self.evm, chain, &block.block.header);

        // Calculate the canonical block hash, before making state-changing operations.
        let canonical_block_hash = block.block.hash_slow();

        // Extract the transactions from the block.
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

        Ok(ExecutedBlock { canonical_block_hash, block: block.block, results })
    }
}

/// Configure EVM with the given header and chain spec.
fn configure_evm<'a, DB: StateProvider>(
    evm: &mut Evm<'a, (), State<ShadowDatabase<DB>>>,
    chain: Arc<ChainSpec>,
    header: &Header,
) {
    let header_spec_id = revm_spec(
        &chain,
        Head::new(
            header.number,
            header.hash_slow(),
            header.difficulty,
            U256::ZERO,
            header.timestamp,
        ),
    );
    let mut cfg = CfgEnvWithHandlerCfg::new_with_spec_id(evm.cfg().clone(), header_spec_id);
    EthEvmConfig::fill_cfg_and_block_env(&mut cfg, evm.block_mut(), &chain, header, U256::ZERO);
    *evm.cfg_mut() = cfg.cfg_env;
}
