use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AddressRepresentation {
    ArrayOfStrings(Vec<String>),
    Bytes([u8; 20]),
    String(String),
}
