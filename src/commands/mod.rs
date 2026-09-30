//! Application use cases grouped by feature.
//!
//! Each child module owns its command types and `medi-rs` handlers. The CLI
//! adapts parsed arguments to these commands and does not access persistence
//! directly.

use crate::paths::AppPaths;
use medi_rs::{medi_module, mediator};
use serde_json::Value;

pub mod auth;
pub mod config;
pub mod discover;
pub mod doctor;
pub mod pull;
pub mod sync;
pub mod time_entries;

use auth::auth_commands as auth_manifest;
use config::config_commands as config_manifest;
use discover::discover_commands as discover_manifest;
use doctor::doctor_commands as doctor_manifest;
use pull::pull_commands as pull_manifest;
use sync::sync_commands as sync_manifest;
use time_entries::time_entries as time_entries_manifest;

/// Dependencies shared by all command handlers for one CLI invocation.
#[derive(Clone)]
pub struct AppContext {
    pub paths: AppPaths,
    pub config: Value,
}

impl AppContext {
    pub fn new(paths: AppPaths, config: Value) -> Self {
        Self { paths, config }
    }
}

medi_module! {
    manifest application_context;
    resources { AppContext; }
}

mediator! {
    pub struct AppMediator {
        event_queue_capacity: 1;
        event_workers: 1;
        modules: [
            application_context,
            auth_manifest,
            config_manifest,
            discover_manifest,
            doctor_manifest,
            pull_manifest,
            sync_manifest,
            time_entries_manifest
        ];
    }
}
