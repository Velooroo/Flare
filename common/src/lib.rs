//! common — shared types, wire protocol and helpers used by both `flare` (CLI) and `flared` (daemon).
//!
//! Comment legend used across the workspace:
//!   INFO  — explains how something works
//!   TODO  — planned / not finished yet
//!   WARN  — works, but has a caveat, performance cost or security trade-off
//!   ERROR — known to be broken right now
//!   FIXME — a concrete bug that should be fixed
//!   HACK  — a workaround that deserves a better solution
//!   NOTE  — extra context, not a problem

pub mod network;
pub mod types;
pub mod utils;

// INFO: Glob re-exports keep call sites short (e.g. `common::PROTOCOL_VERSION`).
pub use network::*;
pub use types::*;
pub use utils::*;
