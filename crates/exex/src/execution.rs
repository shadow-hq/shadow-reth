use eyre::{OptionExt, Result};
use reth_evm_ethereum::EthEvmConfig;
use reth_node_api::{ConfigureEvm, ConfigureEvmEnv};
use reth_primitives::{revm::env::fill_tx_env, BlockWithSenders, ChainSpec, Header};
use reth_provider::StateProvider;
use reth_tracing::tracing::debug;
use revm::{
    db::{states::bundle_state::BundleRetention, State},
    DatabaseCommit, Evm, StateBuilder,
};
use revm_primitives::{CfgEnvWithHandlerCfg, EVMError, ExecutionResult, ResultAndState, U256};
use shadow_reth_common::ToLowerHex;
use std::sync::Arc;

use crate::db::ShadowDatabase;

#[derive(Debug)]
/// todo
pub(crate) struct ShadowExecutor<'a, DB: StateProvider> {
    evm: Evm<'a, (), State<ShadowDatabase<DB>>>,
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

    /// docs todo
    pub(crate) fn execute_one(&mut self, block: BlockWithSenders) -> Result<Vec<ExecutionResult>> {
        let transactions = block.into_transactions();
        let mut results = Vec::with_capacity(transactions.len());

        if !transactions.is_empty() {
            for transaction in transactions {
                let sender = transaction.recover_signer().ok_or_eyre(format!(
                    "invalid canonical transaction '{}', failed to recover signer",
                    transaction.hash.to_lower_hex()
                ))?;

                // Execute transaction.
                // Fill revm structure.
                fill_tx_env(self.evm.tx_mut(), &transaction, sender);

                let ResultAndState { result, state } = match self.evm.transact_preverified() {
                    Ok(result) => result,
                    Err(err) => {
                        match err {
                            EVMError::Transaction(err) => {
                                // if the transaction is invalid, we can skip it
                                debug!(%err, ?transaction, "Skipping invalid transaction");
                                continue;
                            }
                            err => {
                                // this is an error that we should treat as fatal for this attempt
                                eyre::bail!(err)
                            }
                        }
                    }
                };

                self.evm.db_mut().commit(state);

                // append transaction to the list of executed transactions
                results.push(result);
            }

            self.evm.db_mut().merge_transitions(BundleRetention::Reverts);
        }

        Ok(results)
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
