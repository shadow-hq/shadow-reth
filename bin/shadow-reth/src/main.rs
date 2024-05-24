//! Shadow Reth: An open-source reth node with support for shadow bytecode.
//!
//! Works by using [`shadow-reth-exex`] to replay canonical transactions with shadow bytecode,
//! and [`shadow-reth-rpc`] to provide an RPC interface for interacting with shadow data.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use eyre::Result;
use reth_node_ethereum::EthereumNode;
use shadow_reth_common::ShadowLog;
use shadow_reth_exex::ShadowExEx;
use shadow_reth_rpc::ShadowRpc;

fn main() -> Result<()> {
    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let db_path_obj = builder.data_dir().db().join("shadow.db");
        let (tx, rx) = tokio::sync::broadcast::channel::<ShadowLog>(1000);

        // Start reth w/ the shadow exex.
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("ShadowExEx", move |ctx| async { ShadowExEx::init(ctx, tx).await })
            .extend_rpc_modules(move |ctx| ShadowRpc::init(ctx, db_path_obj, rx))
            .launch()
            .await?;

        // Wait for the node to exit.
        handle.wait_for_node_exit().await?;

        Ok(())
    })
}
