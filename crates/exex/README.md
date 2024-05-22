# shadow-exex

The Shadow Execution Extension (ExEx) is a [Reth execution extension](https://www.paradigm.xyz/2024/05/reth-exex) that generates information about shadowed contracts and persists it in a SQLite database.

## How does it work?

At a high level, the Shadow ExEx works as follows:

### Chain Committed

When blocks are committed to the chain, reth emits `ExExNotification::ChainCommitted` for each transaction in the block. This notification contains the entire chain state, along with helpful block and transaction information such as `SealedBlockWithSenders`. `ShadowExEx` needs to re-execute each transaction in the block using `ShadowDatabase` (which implements `revm::Database`). To do this, we use `ShadowExecutor`, a simple block executor using revm, which will execute each transaction in a given block and commits changes to `ShadowDatabase`. When block execution is complete, shadow logs can be recovered from the `ExecutedBlock`, and stored in the SQLite database.

#### ShadowDatabase

`ShadowDatabase` is a simple implementation of `revm::Database` that stores the state of shadow contracts in a SQLite database. It is used by `ShadowExecutor` to serve as a `revm::Database` implementation, which also handles shadowing contract bytecode where applicable.

<details>
<summary>Expand code</summary>

```rust
impl<DB: StateProvider> DatabaseRef for ShadowDatabase<DB> {
    /// Retrieves basic account information for a given address.
    ///
    /// Returns `Ok` with `Some(AccountInfo)` if the account exists,
    /// `None` if it doesn't, or an error if encountered.
    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(self.db.basic_account(address)?.map(|account| AccountInfo {
            balance: account.balance,
            nonce: account.nonce,
            code_hash: self
                .shadow
                .code_hash(&address) // Check if the address is a shadow contract, and use that code hash
                .unwrap_or_else(|| account.bytecode_hash.unwrap_or(KECCAK_EMPTY)),
            code: self.shadow.code(&address),
        }))
    }

    /// Retrieves the bytecode associated with a given code hash.
    ///
    /// Returns `Ok` with the bytecode if found, or the default bytecode otherwise.
    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        Ok(self.shadow.code_by_hash(&code_hash).unwrap_or_else(|| {
            self.bytecode_by_hash(code_hash).ok().flatten().unwrap_or_default().0
        }))
    }

    ...
}
```
</details>

#### ShadowExecutor

`ShadowExecutor` is a simple block executor using revm, which will execute each transaction in a given block and commit changes to `ShadowDatabase`. It is used by `ShadowExEx` to re-execute transactions in a block and store shadow logs in the SQLite database. When executing a block, the `base_fee_per_gas` for the block is set to `0`, allowing shadow contracts to perform arbitrary computations without worrying about gas costs.

`ShadowExecutor` uses `transact_preverified` to execute transactions in a block, since:

1. The block has already been verified by the chain, so we don't need to re-verify it.
2. We're modifying the state of the chain overall. Gas usage, event emission, etc. will change, and may cause the state root to differ from the canonical chain. This would cause the executor to fail if we used `transact`.

### Chain Reverted

When a reorg occurs, reth emits `ExExNotification::ChainReverted`, with the chain of blocks (and their state) that were reverted and are no longer part of canonical mainnet state. `ShadowExEx` handles these notifications by marking the logs as removed in the SQLite database:

```rust
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
```
