// SPDX-License-Identifier: MIT

//! Core consensus used in BFTE
//!
//! Notably this implementation is deterministic,
//! and side-effect free. The higher level code is
//! expected to implement p2p communication querrying
//! other consensus peers for information, that is
//! then passed to be handled by this crate.
//!
//! See [`tables`] module for an overview of
//! the data model it uses.
#![doc = include_str!("../DESIGN.md")]
#![allow(dead_code)]

pub mod consensus;

mod tables;
pub mod vote_set;
