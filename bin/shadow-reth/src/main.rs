//! Shadow Reth: An open-source reth node with support for shadow bytecode.
//!
//! Works by using [`shadow-reth-exex`] to replay canonical transactions with shadow bytecode,
//! and [`shadow-reth-rpc`] to provide an RPC interface for interacting with shadow data.

use eyre::Result;
use reth::{
    providers::test_utils::TestCanonStateSubscriptions,
    rpc::builder::{
        constants::DEFAULT_HTTP_RPC_PORT, RethRpcModule, RpcModuleBuilder, RpcServerConfig,
        TransportRpcModuleConfig,
    },
    tasks::TokioTaskExecutor,
};
use reth_node_ethereum::{EthEvmConfig, EthereumNode};
use shadow_reth_exex::ShadowExEx;
use shadow_reth_rpc::{ShadowRpc, ShadowRpcApiServer};
use tracing::info;

fn main() -> Result<()> {
    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("ShadowExEx", ShadowExEx::init)
            .launch()
            .await?;

        let db_path_obj = handle.node.data_dir.data_dir().join("shadow.db");

        let rpc_builder = RpcModuleBuilder::default()
            .with_provider(handle.node.provider.clone())
            .with_noop_pool()
            .with_noop_network()
            .with_executor(TokioTaskExecutor::default())
            .with_evm_config(EthEvmConfig::default())
            .with_events(TestCanonStateSubscriptions::default());

        let config = TransportRpcModuleConfig::default().with_http([RethRpcModule::Eth]);
        let mut server = rpc_builder.build(config);

        let shadow_rpc =
            ShadowRpc::new(handle.node.provider.clone(), db_path_obj.to_str().unwrap())
                .await
                .unwrap();
        server.merge_configured(shadow_rpc.into_rpc()).unwrap();
        info!("RPC server extended with ShadowRPC API");

        let bind_addr = ["127.0.0.1", &DEFAULT_HTTP_RPC_PORT.to_string()].join(":");
        let server_args =
            RpcServerConfig::http(Default::default()).with_http_address(bind_addr.parse().unwrap());
        let rpc_handle = server_args.start(server);
        info!("RPC server started, url={bind_addr}");

        let (_, _) = tokio::join!(handle.wait_for_node_exit(), rpc_handle);

        Ok(())
    })
}
