use std::ops::{Deref, DerefMut};

use reth_primitives::{Address, B256, KECCAK_EMPTY, U256};
use reth_provider::{ProviderError, StateProvider};
use reth_revm::{
    db::DatabaseRef,
    primitives::{AccountInfo, Bytecode},
    Database,
};

use crate::contracts::ShadowContracts;

/// Wrapper around [`StateProviderDatabase`] that implements the revm database trait
/// and also overrides certain methods, such as `basic` and `code_by_hash`, wherever
/// they touch a shadow contract in [`ShadowContracts`]
#[derive(Debug, Clone)]
pub(crate) struct ShadowDatabase<DB> {
    db: DB,
    shadow: ShadowContracts,
}

impl<DB> ShadowDatabase<DB> {
    /// Create new State with generic StateProvider.
    pub(crate) const fn new(db: DB, shadow: ShadowContracts) -> Self {
        Self { db, shadow }
    }
}

impl<DB> Deref for ShadowDatabase<DB> {
    type Target = DB;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl<DB> DerefMut for ShadowDatabase<DB> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.db
    }
}

impl<DB: StateProvider> Database for ShadowDatabase<DB> {
    type Error = ProviderError;

    /// Retrieves basic account information for a given address.
    ///
    /// Returns `Ok` with `Some(AccountInfo)` if the account exists,
    /// `None` if it doesn't, or an error if encountered.
    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        DatabaseRef::basic_ref(self, address)
    }

    /// Retrieves the bytecode associated with a given code hash.
    ///
    /// Returns `Ok` with the bytecode if found, or the default bytecode otherwise.
    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        DatabaseRef::code_by_hash_ref(self, code_hash)
    }

    /// Retrieves the storage value at a specific index for a given address.
    ///
    /// Returns `Ok` with the storage value, or the default value if not found.
    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        DatabaseRef::storage_ref(self, address, index)
    }

    /// Retrieves the block hash for a given block number.
    ///
    /// Returns `Ok` with the block hash if found, or the default hash otherwise.
    /// Note: It safely casts the `number` to `u64`.
    fn block_hash(&mut self, number: U256) -> Result<B256, Self::Error> {
        DatabaseRef::block_hash_ref(self, number)
    }
}

impl<DB: StateProvider> DatabaseRef for ShadowDatabase<DB> {
    type Error = <Self as Database>::Error;

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

    /// Retrieves the storage value at a specific index for a given address.
    ///
    /// Returns `Ok` with the storage value, or the default value if not found.
    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        Ok(self.db.storage(address, B256::new(index.to_be_bytes()))?.unwrap_or_default())
    }

    /// Retrieves the block hash for a given block number.
    ///
    /// Returns `Ok` with the block hash if found, or the default hash otherwise.
    fn block_hash_ref(&self, number: U256) -> Result<B256, Self::Error> {
        // Attempt to convert U256 to u64
        let block_number = match number.try_into() {
            Ok(value) => value,
            Err(_) => return Err(Self::Error::BlockNumberOverflow(number)),
        };

        // Get the block hash or default hash
        Ok(self.db.block_hash(block_number)?.unwrap_or_default())
    }
}
