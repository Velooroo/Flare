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
pub mod rpc;
pub mod security;
pub mod types;
pub mod utils;

// INFO: Re-export the RPC engine so Flare crates can define services with
//       `common::monolith::api! { … }` without adding their own dependency.
pub use monolith;

// INFO: Glob re-exports keep call sites short (e.g. `common::DeployReply`).
pub use network::*;
pub use types::*;
pub use utils::*;

// INFO: The daemon's control API, declared **once**. The daemon implements the
//       generated `control::Handler`; the CLI uses the generated
//       `control::Client`. Both sides therefore share one wire contract, and the
//       transport is Flare's TLS (`common::rpc`) — there is no plain TCP path.
monolith::api! {
    service control {
        fn register_token(
            token_hash: String,
            existing_token: Option<String>,
            pairing_code: Option<String>,
        ) -> RegisterReply;

        fn deploy(
            token: Option<String>,
            repo: String,
            forge: String,
            branch: Option<String>,
            auth_user: Option<String>,
            auth_password: Option<String>,
        ) -> DeployReply;

        fn manage(token: Option<String>, app: String, action: String) -> ManageReply;

        fn health(token: Option<String>, app: String) -> HealthReply;
    }
}
