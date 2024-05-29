use reth_primitives::{Address, Bloom, Bytes, B256, B64};

/// A trait for converting primitives to lowercase hexadecimal strings.
pub trait ToLowerHex {
    /// Converts the value to a lowercase hexadecimal string.
    ///
    /// ```
    /// use reth_primitives::Address;
    /// use shadow_reth_common::ToLowerHex;
    ///
    /// let value = Address::ZERO;
    /// assert_eq!(value.to_lower_hex(), "0x0000000000000000000000000000000000000000");
    /// ```
    fn to_lower_hex(&self) -> String;
}

impl ToLowerHex for B256 {
    fn to_lower_hex(&self) -> String {
        format!("{self:#x}")
    }
}

impl ToLowerHex for Address {
    fn to_lower_hex(&self) -> String {
        format!("{self:#x}")
    }
}

impl ToLowerHex for B64 {
    fn to_lower_hex(&self) -> String {
        format!("{self:#x}")
    }
}

impl ToLowerHex for Bloom {
    fn to_lower_hex(&self) -> String {
        format!("{self:#x}")
    }
}

impl ToLowerHex for Bytes {
    fn to_lower_hex(&self) -> String {
        format!("{self:#x}")
    }
}
