//! Rust implementation of the PSA Tool.
//!
//! Modules are introduced incrementally while retaining compatibility with the
//! existing application's files in `~/.psa-tool`.

pub mod auth;
pub mod commands;
pub mod config;
pub mod dataverse;
pub mod db;
pub mod interactive;
pub mod model;
pub mod paths;
pub mod project_search;
