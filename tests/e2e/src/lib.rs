#![allow(clippy::unwrap_in_result)]

pub mod api;
pub mod config;
pub mod faucet;
pub mod logger;

/// Drop-in replacement for `#[tokio::test]` that adds per-test tracing setup.
/// See `midnight_node_e2e_macros::e2e_test`.
pub use midnight_node_e2e_macros::e2e_test;
