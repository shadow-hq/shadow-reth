//! ShadowExEx is a reth [Execution Extension](https://www.paradigm.xyz/2024/05/reth-exex) which allows for
//! overriding bytecode at specific addresses with custom "shadow" bytecode.
mod contracts;
mod db;

use contracts::ShadowContracts;
use eyre::{eyre, OptionExt, Result};
use futures::Future;
use reth::providers::{
    providers::BundleStateProvider, DatabaseProviderFactory, HistoricalStateProviderRef,
};
use reth_evm::execute::{BatchExecutor, BlockExecutorProvider};
use reth_evm_ethereum::EthEvmConfig;
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthExecutorProvider;
use reth_tracing::tracing::info;
use serde_json::Value;

use crate::db::ShadowDatabase;

#[derive(Clone, Debug)]
/// The main ExEx struct, which handles loading and parsing shadow configuration,
/// as well as handling ExEx events from reth.
pub struct ShadowExEx {
    /// Stores the shadow contracts, a map of addresses to shadow (overridden) bytecode.
    contracts: ShadowContracts,
}

impl ShadowExEx {
    /// Creates a new instance of the ShadowExEx. This will attempt to load
    /// the configuration from `shadow.json` in the current working directory.
    pub fn new() -> Result<Self> {
        // read config from `./shadow.json` as a serde_json::Value
        let config: Value =
            serde_json::from_str(&std::fs::read_to_string("shadow.json").map_err(|e| {
                eyre!("failed to locate `shadow.json` in the current working directory: {}", e)
            })?)
            .map_err(|e| eyre!("failed to parse `shadow.json`: {}", e))?;

        // parse shadow contracts from the config
        let shadow_contracts = ShadowContracts::try_from(config)?;

        Ok(Self { contracts: shadow_contracts })
    }

    /// The initialization logic of the ExEx is just an async function.
    pub async fn init<Node: FullNodeComponents>(
        ctx: ExExContext<Node>,
    ) -> Result<impl Future<Output = Result<()>>> {
        let this = Self::new()?;

        info!("Initialized ShadowExEx with {} shadowed contracts", this.contracts.len());

        Ok(async move {
            this.exex(ctx).await?;
            Ok(())
        })
    }

    /// The exex
    async fn exex<Node: FullNodeComponents>(&self, mut ctx: ExExContext<Node>) -> Result<()> {
        while let Some(notification) = ctx.notifications.recv().await {
            if let Some(chain) = notification.committed_chain() {
                let evm_config = EthEvmConfig::default();
                let executor_provider =
                    EthExecutorProvider::new(ctx.config.chain.clone(), evm_config);

                let database_provider = ctx.provider().database_provider_ro()?;
                let provider = BundleStateProvider::new(
                    HistoricalStateProviderRef::new(
                        database_provider.tx_ref(),
                        chain.first().number.checked_sub(1).ok_or_eyre("block number underflow")?,
                        database_provider.static_file_provider().clone(),
                    ),
                    chain.state(),
                );
                let shadow_db = ShadowDatabase::new(&provider, self.contracts.clone());

                let mut executor = executor_provider.batch_executor(
                    shadow_db,
                    ctx.config.prune_config().map(|config| config.segments).unwrap_or_default(),
                );

                for block in chain.blocks_iter() {
                    let td = block.header().difficulty;
                    executor.execute_one((&block.clone().unseal(), td).into())?;
                }

                let output = executor.finalize();

                let same_state = chain.state() == &output.into();
                info!(
                    chain = ?chain.range(),
                    %same_state,
                    "Executed chain"
                );

                ctx.events.send(ExExEvent::FinishedHeight(chain.tip().number))?;
            }
        }
        Ok(())
    }
}
