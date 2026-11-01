//! Decision rules for Organize for Me.
//!
//! This crate decides what should happen to a file. It never touches the disk,
//! which is why every rule can be tested without real files.

#![forbid(unsafe_code)]

pub mod guard;
pub mod judge;
pub mod kind;
pub mod ledger;
pub mod memory;
pub mod name;
