use reth_primitives::{Address, Bloom, Bytes, B256, B64};
use std::fmt::Write;

/// A trait for converting primitives to lowercase hexadecimal strings.
pub trait ToLowerHex {
    /// Converts the value to a lowercase hexadecimal string.
    fn to_lower_hex(&self) -> String;
}

impl ToLowerHex for B256 {
    fn to_lower_hex(&self) -> String {
        let mut s = String::with_capacity(66); // 2 for "0x", 64 for the hash
        write!(s, "{self:#x}").unwrap();
        s
    }
}

impl ToLowerHex for Address {
    fn to_lower_hex(&self) -> String {
        let mut s = String::with_capacity(42); // 2 for "0x", 40 for the address
        write!(s, "{self:#x}").unwrap();
        s
    }
}

impl ToLowerHex for B64 {
    fn to_lower_hex(&self) -> String {
        let mut s = String::with_capacity(18); // 2 for "0x", 16 for the value
        write!(s, "{self:#x}").unwrap();
        s
    }
}

impl ToLowerHex for Bloom {
    fn to_lower_hex(&self) -> String {
        let mut s = String::with_capacity(514); // 2 for "0x", 512 for the Bloom filter
        write!(s, "{self:#x}").unwrap();
        s
    }
}

impl ToLowerHex for Bytes {
    fn to_lower_hex(&self) -> String {
        let mut s = String::with_capacity(66); // Adjusted based on expected byte length + 2 for "0x"
        write!(s, "{self:#x}").unwrap();
        s
    }
}
