# shadow-rpc

The Shadow RPC extension allows for custom implementations of methods that return information about shadow events.

## Overview

The `ShadowExEx` Reth execution extension generates information about shadowed contracts and persists it in a SQLite database. The `ShadowRpc` RPC extension allows you to define custom methods for retrieving this information. 

## How does it work?
The RPC is driven by the following type and trait:

```rust
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use shadow_reth_common::SqliteManager;

#[derive(Debug)]
pub struct ShadowRpc<Provider> {
    provider: Provider,
    sqlite_manager: SqliteManager,
}

#[rpc(server, namespace = "shadow")]
pub trait ShadowRpcApi {
    /// Returns shadow logs.
    #[method(name = "getLogs")]
    async fn get_logs(&self, params: GetLogsParameters) -> RpcResult<Vec<GetLogsResult>>;
}
```

The `#[rpc]` macro generates a `Server` trait prepended with the name of the extension trait and the core logic for the extension trait methods should be implemented as part of this generated trait. For organization, the implementation for each RPC API method are split up into their own modules and included as part of the `apis` module.

## Extending the custom namespace

Currently, the Shadow custom RPC extentsion only implements `shadow_getLogs`, which allows you to retrieve Shadow Events emitted by your shadow contracts. However, you can extend the namespace by doing the following:

1. Adjust the `ShadowRpcApi` trait to include the function signature for your desired method along with the desired return type wrapped in an `RpcResult`. You should then decorate the signature with the `#[method(name = ...)]` macro which will add the named method to the RPC namespace. For example, if you wanted to add an equivalent method for `eth_getFilterLogs that returns Shadow Events, you could adjust the trait in the following way:

```rust
#[rpc(server, namespace = "shadow")]
pub trait ShadowRpcApi {
    /// Returns shadow logs.
    #[method(name = "getLogs")]
    async fn get_logs(&self, params: GetLogsParameters) -> RpcResult<Vec<GetLogsResult>>;
    #[method(name = "getFilterLogs")]
    fn get_filter_logs(&self, params: ...) -> RpcResult<...>
}
```

2. Add a new submodule to the `apis` module that contains the core logic for the newly added RPC method.

```rust
impl ShadowRpcApiServer for ShadowRpc
{
    #[doc = "Returns an array of all shadow logs matching filter with given id."]
    fn get_filter_logs(&self, params: ...) -> RpcResult<...> { ... }
}
```

### Notes

Here are some helpful notes when implementing a new method for the custom shadow RPC extension.

#### Async trait methods

You may find it necessary to use async functionality in your custom methods. If so, you should mark the function as `async` in the trait definition and then decorate the `impl` block with the `#[async_trait]` macro. You should then be able to freely `await` in your method implementation as you see fit.

#### Errors

The `RpcResult` type wraps your intended return type in the following type: `std::result::Result<T, jsonrpsee_types::ErrorObjectOwned>`; this will ensure that RPC method errors are properly propagated and returned to the client.

#### Providers

Your method may need to request certain information about the blockchain, e.g. grabbing the corresponding block for a particular hash; a blockchain provider should be used to retrieve that information. To that end, a generic `Provider` type is accessible on the `ShadowRpc` type and can be accessed through `&self.provider` inside your method implementation, provided that you've included `&self` as a parameter in your method signature. Additionally, you may need to extend the implementation of `ShadowRpc` in order to include specific traits that provide the necessary functionality for your new method. Available provider traits can be found in [the documentation for Reth](https://paradigmxyz.github.io/reth/docs/reth/providers/index.html#traits). Be aware that if you are using the provider on a full Reth node to create a `ShadowRpc` instance, a large number of these provider traits are implemented for you already.
