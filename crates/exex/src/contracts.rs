use std::str::FromStr;

use eyre::{eyre, Result};
use revm_primitives::{Address, Bytecode, Bytes, HashMap, B256};
use serde_json::Value;
use shadow_reth_common::ToLowerHex;

#[derive(Clone, Debug)]

/// A map of addresses to shadow bytecode, which will be used when replaying
/// committed transactions.
pub(crate) struct ShadowContracts {
    contracts: HashMap<Address, Bytecode>,
    code_hashes: HashMap<Address, B256>,
}

impl TryFrom<Value> for ShadowContracts {
    type Error = eyre::Error;

    fn try_from(value: Value) -> Result<Self> {
        let contracts = value
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
        let code_hashes =
            contracts.iter().map(|(address, bytecode)| (*address, bytecode.hash_slow())).collect();

        Ok(ShadowContracts { contracts, code_hashes })
    }
}

impl ShadowContracts {
    /// Returns the number of shadow contracts.
    pub(crate) fn len(&self) -> usize {
        self.contracts.len()
    }

    /// Returns the shadow bytecode for the given address, if it exists.
    pub(crate) fn code(&self, address: &Address) -> Option<Bytecode> {
        self.contracts.get(address).cloned()
    }

    /// Get the code hash for a shadow contract at a given address.
    pub(crate) fn code_hash(&self, address: &Address) -> Option<B256> {
        self.code_hashes.get(address).copied()
    }

    /// Retrieves the shadow bytecode associated with a given code hash,
    /// if it exists.
    pub(crate) fn code_by_hash(&self, code_hash: &B256) -> Option<Bytecode> {
        self.code_hashes.iter().find_map(
            |(address, hash)| {
                if hash == code_hash {
                    self.code(address)
                } else {
                    None
                }
            },
        )
    }
}
