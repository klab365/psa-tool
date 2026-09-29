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

#[derive(MediCommand)]
#[medi_command(return_type = TimeEntry, error_type = String)]
pub struct GetTimeEntry {
    pub id: i64,
}

#[derive(MediCommand)]
#[medi_command(return_type = (), error_type = String)]
pub struct UpdateTimeEntry {
    pub id: i64,
    pub entry: TimeEntry,
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

#[medi_handler]
async fn get_time_entry(context: AppContext, command: GetTimeEntry) -> Result<TimeEntry, String> {
    let connection = db::open(&context.paths).map_err(|error| error.to_string())?;
    db::get(&connection, command.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Eintrag {} nicht gefunden", command.id))
}

#[medi_handler]
async fn update_time_entry(context: AppContext, command: UpdateTimeEntry) -> Result<(), String> {
    let connection = db::open(&context.paths).map_err(|error| error.to_string())?;
    db::update_local(&connection, command.id, &command.entry).map_err(|error| error.to_string())
}

medi_module! {
    manifest time_entries;
    commands {
        crate::commands::time_entries::CreateTimeEntry => crate::commands::time_entries::create_time_entry;
        crate::commands::time_entries::RemoveTimeEntry => crate::commands::time_entries::remove_time_entry;
        crate::commands::time_entries::ListTimeEntries => crate::commands::time_entries::list_time_entries;
        crate::commands::time_entries::GetTimeEntry => crate::commands::time_entries::get_time_entry;
        crate::commands::time_entries::UpdateTimeEntry => crate::commands::time_entries::update_time_entry;
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

    #[tokio::test]
    async fn edit_command_reads_and_updates_an_entry() {
        let home = tempfile::tempdir().expect("temporary home");
        let mediator = crate::commands::AppMediator::new(AppContext::new(
            crate::paths::AppPaths::for_home(home.path().into()),
            serde_json::Value::Null,
        ));
        let id = mediator
            .send(CreateTimeEntry { entry: entry() })
            .await
            .expect("entry is created");

        let mut fetched = mediator
            .send(GetTimeEntry { id })
            .await
            .expect("entry is loaded");
        fetched.hours = 4.0;
        fetched.description = Some("geaendert".into());
        mediator
            .send(UpdateTimeEntry { id, entry: fetched })
            .await
            .expect("entry is updated");

        let entries = mediator
            .send(ListTimeEntries {
                from: None,
                to: None,
                include_deleted: true,
            })
            .await
            .expect("entries are listed");
        assert_eq!(entries[0].hours, 4.0);
        assert_eq!(entries[0].description.as_deref(), Some("geaendert"));
        assert_eq!(entries[0].status, "new");
        assert!(mediator.send(GetTimeEntry { id: 999 }).await.is_err());
    }

    #[tokio::test]
    async fn list_returns_newest_entries_first() {
        let home = tempfile::tempdir().expect("temporary home");
        let mediator = crate::commands::AppMediator::new(AppContext::new(
            crate::paths::AppPaths::for_home(home.path().into()),
            serde_json::Value::Null,
        ));
        let mut older = entry();
        older.work_date = "2026-09-01".into();
        let mut newer = entry();
        newer.work_date = "2026-09-03".into();
        mediator
            .send(CreateTimeEntry { entry: older })
            .await
            .expect("older entry is created");
        mediator
            .send(CreateTimeEntry { entry: newer })
            .await
            .expect("newer entry is created");

        let entries = mediator
            .send(ListTimeEntries {
                from: None,
                to: None,
                include_deleted: true,
            })
            .await
            .expect("entries are listed");
        let dates: Vec<_> = entries
            .iter()
            .map(|entry| entry.work_date.as_str())
            .collect();
        assert_eq!(dates, ["2026-09-03", "2026-09-01"]);
    }
}
