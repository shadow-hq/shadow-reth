//! ShadowExEx is a reth [Execution Extension](https://www.paradigm.xyz/2024/05/reth-exex) which allows for
//! overriding bytecode at specific addresses with custom "shadow" bytecode.
mod contracts;
mod db;
mod execution;

use contracts::ShadowContracts;
use execution::ShadowExecutor;
use eyre::{eyre, OptionExt, Result};
use futures::Future;
use reth::providers::{DatabaseProviderFactory, HistoricalStateProviderRef};
use reth_evm_ethereum::EthEvmConfig;
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;
use reth_tracing::tracing::info;
use revm_primitives::Log;
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
                // Create a read-only database provider that we can use to get historical state
                // at the start of the notification chain. i.e. the state at the first block in the
                // notification, pre-execution.
                let database_provider = ctx.provider().database_provider_ro()?;
                let provider = HistoricalStateProviderRef::new(
                    database_provider.tx_ref(),
                    chain.first().number,
                    database_provider.static_file_provider().clone(),
                );

                // Use the database provider to create a [`ShadowDatabase`]. This is a
                // [`reth_revm::Database`] implementation that will override the
                // bytecode of contracts at specific addresses with custom shadow bytecode, as
                // defined in `shadow.json`.
                let db = ShadowDatabase::new(provider, self.contracts.clone());

                let blocks = chain.blocks_iter().collect::<Vec<_>>();

                // Construct a new `ShadowExecutor` with the default config and proper chain spec,
                // using the `ShadowDatabase` as the state provider.
                let evm_config = EthEvmConfig::default();
                let mut executor = ShadowExecutor::new(
                    &evm_config,
                    db,
                    ctx.config.chain.clone(),
                    blocks
                        .first()
                        .map(|b| b.header())
                        .ok_or_eyre("No blocks found in ExEx notification")?,
                );

                // Execute the blocks in the chain, collecting logs from shadowed contracts.
                let logs = blocks
                    .into_iter()
                    .map(|block| executor.execute_one(block.clone().unseal()))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .flat_map(|result| result.into_logs())
                    .filter(|log| self.contracts.is_shadowed(&log.address))
                    .collect::<Vec<Log>>();

                println!("Logs: {:?}", logs);

                ctx.events.send(ExExEvent::FinishedHeight(chain.tip().number))?;
            }
        }
        Ok(())
    }
}
