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
use shadow_reth_common::ShadowSqliteDb;

use crate::db::ShadowDatabase;

#[derive(Debug)]
/// The main ExEx struct, which handles loading and parsing shadow configuration,
/// as well as handling ExEx events from reth.
pub struct ShadowExEx {
    /// Stores the shadow contracts, a map of addresses to shadow (overridden) bytecode.
    contracts: ShadowContracts,
    /// The [`ShadowSqliteDb`] for the shadow database.
    sqlite_db: ShadowSqliteDb,
}

impl ShadowExEx {
    /// Creates a new instance of the ShadowExEx. This will attempt to load
    /// the configuration from `shadow.json` in the current working directory.
    pub async fn new(db_path: PathBuf) -> Result<Self> {
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

        Ok(Self { contracts, sqlite_db })
    }

    /// The initialization logic of the ExEx is just an async function.
    pub async fn init<Node: FullNodeComponents>(
        ctx: ExExContext<Node>,
    ) -> Result<impl Future<Output = Result<()>>> {
        let db_path = ctx.config.datadir().db();
        let this = Self::new(db_path).await?;

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

                    println!("Shadow logs: {:?}", shadow_logs);

                    // Create a new runtime to send the shadow logs to the shadow database.
                    tokio::spawn({
                        let sqlite_db = self.sqlite_db.clone();
                        async move {
                            let _ = sqlite_db.bulk_insert_into_shadow_log_table(shadow_logs).await;
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
                        let sqlite_db = self.sqlite_db.clone();

                        // Create a new runtime to handle the block reorg in the shadow database.
                        tokio::spawn({
                            async move {
                                let _ = sqlite_db.handle_block_reorg(block.hash_slow()).await;
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, pin::pin};

    use alloy_sol_types::SolEvent;
    use reth::{args::DatadirArgs, dirs::MaybePlatformPath, revm::db::BundleState};
    use reth_exex_test_utils::{test_exex_context, PollOnce};
    use reth_primitives::{
        address, Address, Block, Bytes, Header, Log, Receipt, Receipts, Transaction,
        TransactionSigned, TxKind, TxLegacy, TxType, U256,
    };
    use reth_provider::{BundleStateWithReceipts, Chain, DatabaseProviderFactory};
    use reth_revm::{
        db::{AccountStatus, BundleAccount},
        primitives::AccountInfo,
    };
    use reth_testing_utils::generators::sign_tx_with_random_key_pair;

    use crate::ShadowExEx;

    const WETH_ADDRESS: Address = address!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");

    fn generate_tx_and_receipt(
        from: Address,
        to: Address,
        input: Bytes,
        value: U256,
    ) -> eyre::Result<(TransactionSigned, Receipt)> {
        let tx = Transaction::Legacy(TxLegacy {
            to: TxKind::Call(to),
            input,
            value,
            gas_limit: u64::MAX,
            ..Default::default()
        });
        let receipt = Receipt {
            tx_type: TxType::Legacy,
            success: true,
            cumulative_gas_used: 0,
            logs: vec![],
            ..Default::default()
        };
        Ok((sign_tx_with_random_key_pair(&mut rand::thread_rng(), tx), receipt))
    }

    #[tokio::test]
    async fn test_exex() -> eyre::Result<()> {
        let (mut ctx, handle) = test_exex_context().await?;

        println!("{:#?}", ctx.components.provider.database_provider_ro());

        // copy ../../shadow.json.example to cwd
        std::fs::copy("../../shadow.json.example", "shadow.json")?;

        // init ShadowExEx
        let mut exex = pin!(ShadowExEx::init(ctx).await?);

        // get a random from address, and give it some ETH
        let from_address = Address::random();
        let to_address = Address::random();
        let from_balance = U256::from(1_000_000_000);

        // build account state
        let from_account_info = AccountInfo { balance: from_balance, ..Default::default() };
        let from_bundle_account = BundleAccount {
            info: Some(from_account_info.clone()),
            original_info: Some(from_account_info),
            storage: HashMap::new(),
            status: AccountStatus::LoadedNotExisting,
        };
        let bundle_state = HashMap::from([(from_address, from_bundle_account)]);

        // build a new WETH deposit transaction
        let (deposit_tx, deposit_tx_receipt) =
            generate_tx_and_receipt(from_address, WETH_ADDRESS, Bytes::default(), U256::from(0))?;
        let block = Block {
            header: Header { gas_limit: u64::MAX, ..Default::default() },
            body: vec![deposit_tx],
            ..Default::default()
        }
        .seal_slow()
        .seal_with_senders()
        .ok_or_else(|| eyre::eyre!("failed to recover senders"))?;

        let chain = Chain::new(
            vec![block.clone()],
            BundleStateWithReceipts::new(
                BundleState { state: bundle_state, ..Default::default() },
                Receipts::from_block_receipt(vec![deposit_tx_receipt]),
                block.number,
            ),
            None,
        );

        handle.send_notification_chain_committed(chain.clone()).await?;
        exex.poll_once().await;

        Ok(())
    }
}
