//! One module per CLI command group. Each module talks to the daemon directly
//! (or writes local config) — there is no shared state between them.

pub mod apps;
pub mod auth;
pub mod deploy;
pub mod devices;
pub mod discovery;
