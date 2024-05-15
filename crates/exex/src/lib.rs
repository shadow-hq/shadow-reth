//! ShadowExEx is a reth [Execution Extension](https://www.paradigm.xyz/2024/05/reth-exex) which allows for
//! overriding bytecode at specific addresses with custom "shadow" bytecode.

use std::str::FromStr;

use eyre::{eyre, Result};
use futures::Future;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use revm_primitives::{Address, Bytecode, Bytes, HashMap};
use serde_json::Value;
use shadow_reth_common::ToLowerHex;
use tracing::info;

#[derive(Clone, Debug)]
/// The main ExEx struct, which handles loading and parsing shadow configuration,
/// as well as handling ExEx events from reth.
pub struct ShadowExEx {
    /// A map of addresses to shadow bytecode, which will be used when replaying
    /// committed transactions.
    shadow_contracts: HashMap<Address, Bytecode>,
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

        // parse the config into a HashMap<Address, Bytecode>
        let shadow_contracts = config
            .as_object()
            .ok_or_else(|| eyre!("`shadow.json` must be an object"))?
            .iter()
            .map(|(address, bytecode)| {
                let address = Address::from_str(address).map_err(|e| {
                    eyre!("shadow configuration invalid at {address}: invalid address: {e}",)
                })?;
                let bytecode = Bytecode::new_raw(
                    Bytes::from_str(bytecode.as_str().ok_or_else(|| {
                        eyre!(
                            "shadow configuration invalid at {}: bytecode must be a string",
                            address.to_lower_hex()
                        )
                    })?)
                    .map_err(|e| {
                        eyre!(
                            "shadow configuration invalid at {}: invalid bytecode: {e}",
                            address.to_lower_hex()
                        )
                    })?,
                );
                Ok((address, bytecode))
            })
            .collect::<Result<HashMap<Address, Bytecode>>>()?;

        Ok(Self { shadow_contracts })
    }

    /// The initialization logic of the ExEx is just an async function.
    pub async fn init<Node: FullNodeComponents>(
        ctx: ExExContext<Node>,
    ) -> Result<impl Future<Output = Result<()>>> {
        let this = Self::new()?;

        Ok(async move {
            this.exex(ctx).await?;
            Ok(())
        })
    }

    /// The exex
    async fn exex<Node: FullNodeComponents>(&self, mut ctx: ExExContext<Node>) -> Result<()> {
        while let Some(notification) = ctx.notifications.recv().await {
            match &notification {
                ExExNotification::ChainCommitted { new } => {
                    info!(committed_chain = ?new.range(), "Received commit");
                }
                ExExNotification::ChainReorged { old, new } => {
                    info!(from_chain = ?old.range(), to_chain = ?new.range(), "Received reorg");
                }
                ExExNotification::ChainReverted { old } => {
                    info!(reverted_chain = ?old.range(), "Received revert");
                }
            };

            if let Some(committed_chain) = notification.committed_chain() {
                ctx.events.send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
            }
        }
        Ok(())
    }
}
