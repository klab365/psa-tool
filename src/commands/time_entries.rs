//! Command handlers for local time-entry persistence.
//!
//! `medi-rs` keeps the CLI boundary separate from these use cases: commands
//! carry input data and the mediator injects the application paths needed by
//! the handlers.

use crate::{commands::AppContext, db, model::TimeEntry};
use medi_rs::{MediCommand, medi_handler, medi_module};

#[derive(MediCommand)]
#[medi_command(return_type = i64, error_type = String)]
pub struct CreateTimeEntry {
    pub entry: TimeEntry,
}

#[derive(MediCommand)]
#[medi_command(return_type = (), error_type = String)]
pub struct RemoveTimeEntry {
    pub id: i64,
}

#[derive(MediCommand)]
#[medi_command(return_type = Vec<TimeEntry>, error_type = String)]
pub struct ListTimeEntries {
    pub from: Option<String>,
    pub to: Option<String>,
    pub include_deleted: bool,
}

#[medi_handler]
async fn create_time_entry(context: AppContext, command: CreateTimeEntry) -> Result<i64, String> {
    let connection = db::open(&context.paths).map_err(|error| error.to_string())?;
    db::insert(&connection, &command.entry, None, "new").map_err(|error| error.to_string())
}

#[medi_handler]
async fn remove_time_entry(context: AppContext, command: RemoveTimeEntry) -> Result<(), String> {
    let connection = db::open(&context.paths).map_err(|error| error.to_string())?;
    db::mark_deleted(&connection, command.id)
        .map(|_| ())
        .map_err(|_| format!("Eintrag {} nicht gefunden", command.id))
}

#[medi_handler]
async fn list_time_entries(
    context: AppContext,
    command: ListTimeEntries,
) -> Result<Vec<TimeEntry>, String> {
    let connection = db::open(&context.paths).map_err(|error| error.to_string())?;
    db::list(
        &connection,
        command.from.as_deref(),
        command.to.as_deref(),
        command.include_deleted,
    )
    .map_err(|error| error.to_string())
}

medi_module! {
    manifest time_entries;
    commands {
        crate::commands::time_entries::CreateTimeEntry => crate::commands::time_entries::create_time_entry;
        crate::commands::time_entries::RemoveTimeEntry => crate::commands::time_entries::remove_time_entry;
        crate::commands::time_entries::ListTimeEntries => crate::commands::time_entries::list_time_entries;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> TimeEntry {
        TimeEntry {
            id: 0,
            work_date: "2026-09-01".into(),
            project_id: None,
            project_name: Some("Projekt A".into()),
            task_id: None,
            task_name: None,
            hours: 7.5,
            description: Some("Test".into()),
            remote_id: None,
            status: "new".into(),
            entry_status: None,
            error: None,
        }
    }

    #[tokio::test]
    async fn commands_persist_and_remove_an_entry() {
        let home = tempfile::tempdir().expect("temporary home");
        let mediator = crate::commands::AppMediator::new(AppContext::new(
            crate::paths::AppPaths::for_home(home.path().into()),
            serde_json::Value::Null,
        ));

        let id = mediator
            .send(CreateTimeEntry { entry: entry() })
            .await
            .expect("entry is created");
        let entries = mediator
            .send(ListTimeEntries {
                from: None,
                to: None,
                include_deleted: true,
            })
            .await
            .expect("entries are listed");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, id);

        mediator
            .send(RemoveTimeEntry { id })
            .await
            .expect("entry is marked as deleted");
        let entries = mediator
            .send(ListTimeEntries {
                from: None,
                to: None,
                include_deleted: false,
            })
            .await
            .expect("remaining entries are listed");
        assert!(entries.is_empty());
    }
}
