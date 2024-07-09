//! ShadowExEx is a reth [Execution Extension](https://www.paradigm.xyz/2024/05/reth-exex) which allows for
//! overriding bytecode at specific addresses with custom "shadow" bytecode.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod contracts;
mod db;
mod execution;

use std::path::PathBuf;

use contracts::ShadowContracts;
use execution::ShadowExecutor;
use eyre::{eyre, OptionExt, Result};
use futures::Future;
use reth_evm_ethereum::EthEvmConfig;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use reth_provider::{DatabaseProviderFactory, HistoricalStateProviderRef};
use reth_tracing::tracing::{debug, info};
use serde_json::Value;
use shadow_reth_common::{ShadowSqliteDb, ToLowerHex};
use tokio::sync::broadcast::Sender;

use crate::db::ShadowDatabase;

#[derive(Debug)]
/// The main ExEx struct, which handles loading and parsing shadow configuration,
/// as well as handling ExEx events from reth.
pub struct ShadowExEx {
    /// Stores the shadow contracts, a map of addresses to shadow (overridden) bytecode.
    contracts: ShadowContracts,
    /// The [`ShadowSqliteDb`] for the shadow database.
    sqlite_db: ShadowSqliteDb,

    indexed_block_hash_sender: Sender<String>,
}

impl ShadowExEx {
    /// Creates a new instance of the ShadowExEx. This will attempt to load
    /// the configuration from `shadow.json` in the current working directory.
    pub async fn new(db_path: PathBuf, indexed_block_hash_sender: Sender<String>) -> Result<Self> {
        // read config from `./shadow.json` as a serde_json::Value
        let config: Value =
            serde_json::from_str(&std::fs::read_to_string("shadow.json").map_err(|e| {
                eyre!("failed to locate `shadow.json` in the current working directory: {}", e)
            })?)
            .map_err(|e| eyre!("failed to parse `shadow.json`: {}", e))?;

        // parse shadow contracts from the config
        let contracts = ShadowContracts::try_from(config)?;

        // get the path to the shadow database
        let shadow_db_path = db_path.join("shadow.db");
        debug!("Path to shadow database: {}", shadow_db_path.display());

        // create a new ShadowSqliteDb for the shadow database
        let sqlite_db = ShadowSqliteDb::new(
            shadow_db_path.to_str().expect("Failed to convert shadow_db_path to string"),
        )
        .await?;

        Ok(Self { contracts, sqlite_db, indexed_block_hash_sender })
    }

    /// The initialization logic of the ExEx is just an async function.
    pub async fn init<Node: FullNodeComponents>(
        ctx: ExExContext<Node>,
        indexed_block_hash_sender: Sender<String>,
    ) -> Result<impl Future<Output = Result<()>>> {
        let db_path = ctx.data_dir.db();
        let this = Self::new(db_path, indexed_block_hash_sender).await?;

        info!("Initialized ShadowExEx with {} shadowed contracts", this.contracts.len());

        Ok(async move {
            this.exex(ctx).await?;
            Ok(())
        })
    }

    /// The exex
    async fn exex<Node: FullNodeComponents>(&self, mut ctx: ExExContext<Node>) -> Result<()> {
        while let Some(notification) = ctx.notifications.recv().await {
            match notification {
                ExExNotification::ChainCommitted { new: chain } => {
                    // Create a read-only database provider that we can use to get historical state
                    // at the start of the notification chain. i.e. the state at the first block in
                    // the notification, pre-execution.
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

                    // Construct a new `ShadowExecutor` with the default config and proper chain
                    // spec, using the `ShadowDatabase` as the state provider.
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
                    let shadow_logs = blocks
                        .into_iter()
                        .map(|block| executor.execute_one(block.clone().unseal()))
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .flat_map(|executed_block| executed_block.logs())
                        .filter(|log| {
                            self.contracts.is_shadowed(
                                &log.address.parse().expect("failed to parse log address"),
                            )
                        })
                        .collect::<Vec<_>>();

                    // Create a new task to send the shadow logs to the shadow database.
                    tokio::spawn({
                        let sqlite_db = self.sqlite_db.clone();
                        let indexed_block_hash_sender = self.indexed_block_hash_sender.clone();
                        async move {
                            let block_hashes =
                                shadow_logs.iter().fold(Vec::new(), |mut acc, log| {
                                    match acc.last() {
                                        None => acc.push(log.block_hash.clone()),
                                        Some(last) if last != &log.block_hash => {
                                            acc.push(log.block_hash.clone())
                                        }
                                        _ => {}
                                    }

                                    acc
                                });

                            let _ = sqlite_db.bulk_insert_into_shadow_log_table(shadow_logs).await;
                            for block_hash in block_hashes {
                                let _ = indexed_block_hash_sender.send(block_hash);
                            }
                        }
                    });

                    // We're done, so send a FinishedHeight event to the ExEx.
                    ctx.events.send(ExExEvent::FinishedHeight(chain.tip().number))?;
                }
                ExExNotification::ChainReverted { old: chain } => {
                    // The chain was reverted to a previous state, so we need to invalidate the
                    // blocks in the old chain
                    chain.blocks_iter().for_each(|block| {
                        let block = block.clone().unseal();
                        debug!(block = block.number, "Invalidating shadow logs");

                        // Create a new task to handle the block reorg in the shadow database.
                        tokio::spawn({
                            let sqlite_db = self.sqlite_db.clone();
                            let indexed_block_hash_sender = self.indexed_block_hash_sender.clone();
                            async move {
                                let block_hash = block.hash_slow();
                                let _ = sqlite_db.handle_block_reorg(block_hash).await;
                                let _ = indexed_block_hash_sender.send(block_hash.to_lower_hex());
                            }
                        });
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}
