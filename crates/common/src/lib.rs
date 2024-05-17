//! Common utilities and functions used throughout [`shadow-reth`].

mod hex;

// re-export the `hex` module
pub use hex::*;

/// Contains shared logic for interating with SQLite.
mod db;
pub use db::*;
