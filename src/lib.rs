//! Rust implementation of the PSA Tool.
//!
//! Modules are introduced incrementally while retaining compatibility with the
//! existing application's files in `~/.psa-tool`.

// The `default_config` JSON literal grows with each mapping option and exceeds
// the default macro recursion limit; raise it so `serde_json::json!` expands.
#![recursion_limit = "256"]

pub mod auth;
pub mod commands;
pub mod config;
pub mod dataverse;
pub mod dates;
pub mod db;
pub mod interactive;
pub mod model;
pub mod paths;
pub mod project_search;
