use reth_primitives::{Bloom, B64};
use revm_primitives::{Address, Bytes, B256};

/// A trait for converting reth and revm primitives to lowercase hexadecimal strings.
pub trait ToLowerHex {
    /// Converts the value to a lowercase hexadecimal string.
    ///
    /// ```
    /// use reth_primitives::Address;
    ///
    /// let value = Address::ZERO;
    /// assert_eq!(value.to_lower_hex(), "0x0000000000000000000000000000000000000000");
    /// ```
    fn to_lower_hex(&self) -> String;
}

impl ToLowerHex for B256 {
    fn to_lower_hex(&self) -> String {
        format!("{:#032x}", self)
    }
}

impl ToLowerHex for Address {
    fn to_lower_hex(&self) -> String {
        format!("{:#020x}", self)
    }
}

impl ToLowerHex for B64 {
    fn to_lower_hex(&self) -> String {
        format!("{:#016x}", self)
    }
}

impl ToLowerHex for Bloom {
    fn to_lower_hex(&self) -> String {
        format!("{:#064x}", self)
    }
}

impl ToLowerHex for Bytes {
    fn to_lower_hex(&self) -> String {
        format!("{:#x}", self)
    }
}
