//! Synchronisation of local time entries with Dataverse.
//!
//! The handler performs the push/patch/delete loop and returns a structured
//! summary so that both the CLI and the web layer can render the outcome.

use crate::{commands::AppContext, db, mapping};
use medi_rs::{MediCommand, medi_handler, medi_module};

/// Outcome of a single entry during a sync run.
#[derive(Debug, Clone)]
pub struct SyncOutcome {
    pub label: String,
    /// Imperative verb for the dry-run output, e.g. `CREATE`.
    pub action: &'static str,
    pub ok: bool,
    pub error: Option<String>,
}

/// Aggregate result of a sync run.
#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub dry_run: bool,
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub failed: usize,
    pub outcomes: Vec<SyncOutcome>,
}

impl SyncSummary {
    pub fn is_empty(&self) -> bool {
        self.outcomes.is_empty()
    }

    /// Human-readable one-line summary shown after the run.
    pub fn message(&self) -> String {
        if self.dry_run {
            format!(
                "Dry-run fertig: {} erstellen, {} aktualisieren, {} löschen.",
                self.created, self.updated, self.deleted
            )
        } else if self.failed == 0 {
            format!(
                "Sync fertig: {} erstellt, {} aktualisiert, {} gelöscht.",
                self.created, self.updated, self.deleted
            )
        } else {
            format!(
                "Sync fertig: {} erstellt, {} aktualisiert, {} gelöscht, {} fehlgeschlagen.",
                self.created, self.updated, self.deleted, self.failed
            )
        }
    }
}

#[derive(MediCommand)]
#[medi_command(return_type = SyncSummary, error_type = String)]
pub struct SyncTimeEntries {
    pub dry_run: bool,
}

#[medi_handler]
async fn sync_time_entries(
    context: AppContext,
    command: SyncTimeEntries,
) -> Result<SyncSummary, String> {
    let connection = db::open(&context.paths).map_err(|error| error.to_string())?;
    let pending = db::pending(&connection).map_err(|error| error.to_string())?;
    let dry = command.dry_run;

    if pending.is_empty() {
        return Ok(SyncSummary {
            dry_run: dry,
            created: 0,
            updated: 0,
            deleted: 0,
            failed: 0,
            outcomes: Vec::new(),
        });
    }

    // A dry run must remain fully local: it must not require a token or make
    // an HTTP request.
    let client = if dry {
        None
    } else {
        Some(mapping::client(&context.paths, &context.config).await?)
    };
    let mapping = &context.config["mapping"];
    let mut created = 0;
    let mut updated = 0;
    let mut deleted = 0;
    let mut failed = 0;
    let mut outcomes = Vec::new();

    for entry in pending {
        let label = format!(
            "{} | {} | {}h",
            entry.work_date,
            entry.project_name.as_deref().unwrap_or("?"),
            entry.hours
        );
        let (kind, action) = match entry.status.as_str() {
            "deleted" => ("deleted", "DELETE"),
            "new" => ("created", "CREATE"),
            _ => ("updated", "UPDATE"),
        };
        let result = match entry.status.as_str() {
            "deleted" => {
                if dry {
                    Ok(())
                } else {
                    client
                        .as_ref()
                        .expect("Dataverse client for non-dry sync")
                        .delete(
                            mapping::value(mapping, "/timeEntryEntitySet"),
                            entry.remote_id.as_deref().unwrap_or(""),
                        )
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|_| db::delete(&connection, entry.id).map_err(|e| e.to_string()))
                }
            }
            "new" => {
                if dry {
                    Ok(())
                } else {
                    match client
                        .as_ref()
                        .expect("Dataverse client for non-dry sync")
                        .post(
                            mapping::value(mapping, "/timeEntryEntitySet"),
                            &mapping::entry_body(&entry, &context.config),
                        )
                        .await
                    {
                        Ok(response) => db::synced(
                            &connection,
                            entry.id,
                            response[mapping::value(mapping, "/timeEntryIdField")]
                                .as_str()
                                .unwrap_or(""),
                        )
                        .map_err(|error| error.to_string()),
                        Err(error) => Err(error.to_string()),
                    }
                }
            }
            _ => {
                if dry {
                    Ok(())
                } else {
                    client
                        .as_ref()
                        .expect("Dataverse client for non-dry sync")
                        .patch(
                            mapping::value(mapping, "/timeEntryEntitySet"),
                            entry.remote_id.as_deref().unwrap_or(""),
                            &mapping::entry_body(&entry, &context.config),
                        )
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|_| {
                            db::synced(
                                &connection,
                                entry.id,
                                entry.remote_id.as_deref().unwrap_or(""),
                            )
                            .map_err(|error| error.to_string())
                        })
                }
            }
        };
        match result {
            Ok(()) => {
                match kind {
                    "deleted" => deleted += 1,
                    "created" => created += 1,
                    _ => updated += 1,
                }
                outcomes.push(SyncOutcome {
                    label,
                    action,
                    ok: true,
                    error: None,
                });
            }
            Err(error) => {
                failed += 1;
                db::error(&connection, entry.id, &error).map_err(|e| e.to_string())?;
                outcomes.push(SyncOutcome {
                    label,
                    action,
                    ok: false,
                    error: Some(error),
                });
            }
        }
    }

    Ok(SyncSummary {
        dry_run: dry,
        created,
        updated,
        deleted,
        failed,
        outcomes,
    })
}

medi_module! {
    manifest sync_commands;
    commands {
        crate::commands::sync::SyncTimeEntries => crate::commands::sync::sync_time_entries;
    }
}
