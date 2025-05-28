//! BFTE Node
//!
//! A BFTE node follows and possibly participates in extending
//! some consensus maintained as a blockchain, persisting necessary
//! data in a database.
//!
//! This crate drives [`bfte-consensus`] for actual consensus logic,
//! taking care of communication with other peers based on the consensus
//! state.
//!
//! See [`run_consensus`] for the core consensus round loop logic.
mod app_api;
mod connection_pool;
pub mod derive_secret_ext;
mod envs;
mod finality_vote_query_task;
mod handle;
mod invite;
mod join;
mod node;
mod pass;
mod peer_address;
pub(crate) mod rpc;
mod rpc_server;
mod run_consensus;
mod tables;
mod ui_api;

use std::time::Duration;

use backon::FibonacciBuilder;
pub use node::Node;

const LOG_TARGET: &str = "bfte::node";
const RPC_BACKOFF: FibonacciBuilder = FibonacciBuilder::new()
    .with_jitter()
    .without_max_times()
    .with_max_delay(Duration::from_secs(60));
