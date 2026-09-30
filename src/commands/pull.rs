//! Pulling time entries from Dataverse into the local database.

use crate::{commands::AppContext, dates, db, mapping, model::TimeEntry};
use medi_rs::{MediCommand, medi_handler, medi_module};

/// Aggregate result of a pull run.
#[derive(Debug, Clone)]
pub struct PullSummary {
    pub from: String,
    pub to: String,
    pub created: usize,
    pub updated: usize,
}

#[derive(MediCommand)]
#[medi_command(return_type = PullSummary, error_type = String)]
pub struct PullTimeEntries {
    pub from: Option<String>,
    pub to: Option<String>,
    pub date: Option<String>,
}

#[medi_handler]
async fn pull_time_entries(
    context: AppContext,
    command: PullTimeEntries,
) -> Result<PullSummary, String> {
    let config = &context.config;
    if mapping::value(config, "/resourceId").is_empty() {
        return Err(
            "resourceId ist nicht gesetzt. Zuerst 'psa discover myresource' ausführen.".into(),
        );
    }
    let (from, to) = if command.from.is_none() && command.to.is_none() {
        dates::week(command.date.as_deref())?
    } else {
        (
            command.from.unwrap_or_default(),
            command.to.unwrap_or_default(),
        )
    };
    let mapping = &config["mapping"];
    let filter = format!(
        "{} eq {} and {} ge {}T00:00:00Z and {} le {}T23:59:59Z",
        mapping::value(mapping, "/resourceLookupValueField"),
        mapping::value(config, "/resourceId"),
        mapping::value(mapping, "/dateField"),
        from,
        mapping::value(mapping, "/dateField"),
        to
    );
    let select = [
        "/timeEntryIdField",
        "/dateField",
        "/durationField",
        "/descriptionField",
        "/projectLookupValueField",
        "/taskLookupValueField",
        "/entryStatusField",
    ]
    .map(|path| mapping::value(mapping, path))
    .join(",");
    let data = mapping::client(&context.paths, config)
        .await?
        .get(
            &format!(
                "/{}?$filter={filter}&$select={select}&$orderby={} asc",
                mapping::value(mapping, "/timeEntryEntitySet"),
                mapping::value(mapping, "/dateField")
            ),
            true,
        )
        .await
        .map_err(|error| error.to_string())?;

    let connection = db::open(&context.paths).map_err(|error| error.to_string())?;
    let mut created = 0;
    let mut updated = 0;
    for row in data["value"].as_array().into_iter().flatten() {
        let remote = row[mapping::value(mapping, "/timeEntryIdField")]
            .as_str()
            .unwrap_or("");
        let raw = row[mapping::value(mapping, "/dateField")]
            .as_str()
            .unwrap_or("");
        let work_date = mapping::local_date(raw, mapping);
        let entry = TimeEntry {
            id: 0,
            work_date,
            project_id: row[mapping::value(mapping, "/projectLookupValueField")]
                .as_str()
                .map(str::to_owned),
            project_name: row[format!(
                "{}@OData.Community.Display.V1.FormattedValue",
                mapping::value(mapping, "/projectLookupValueField")
            )]
            .as_str()
            .map(str::to_owned),
            task_id: row[mapping::value(mapping, "/taskLookupValueField")]
                .as_str()
                .map(str::to_owned),
            task_name: row[format!(
                "{}@OData.Community.Display.V1.FormattedValue",
                mapping::value(mapping, "/taskLookupValueField")
            )]
            .as_str()
            .map(str::to_owned),
            hours: row[mapping::value(mapping, "/durationField")]
                .as_f64()
                .unwrap_or(0.)
                / if mapping::value(mapping, "/durationUnit") == "hours" {
                    1.
                } else {
                    60.
                },
            description: row[mapping::value(mapping, "/descriptionField")]
                .as_str()
                .map(str::to_owned),
            remote_id: Some(remote.into()),
            status: "synced".into(),
            entry_status: row[format!(
                "{}@OData.Community.Display.V1.FormattedValue",
                mapping::value(mapping, "/entryStatusField")
            )]
            .as_str()
            .map(str::to_owned),
            error: None,
        };
        match db::by_remote_id(&connection, remote).map_err(|error| error.to_string())? {
            None => {
                db::insert(&connection, &entry, Some(remote), "synced")
                    .map_err(|error| error.to_string())?;
                created += 1
            }
            Some(local) if local.status == "synced" => {
                db::update_synced(&connection, local.id, &entry)
                    .map_err(|error| error.to_string())?;
                updated += 1
            }
            Some(local) => eprintln!(
                "⚠ Konflikt bei Eintrag #{}: lokaler Status '{}'",
                local.id, local.status
            ),
        }
    }

    // A remote record missing from this range was deleted in Dataverse. Keep
    // unsynchronised local work intact, exactly as the Python implementation.
    for local in
        db::list(&connection, Some(&from), Some(&to), false).map_err(|error| error.to_string())?
    {
        if local.status == "synced"
            && local.remote_id.is_some()
            && !data["value"].as_array().is_some_and(|rows| {
                rows.iter().any(|row| {
                    row[mapping::value(mapping, "/timeEntryIdField")].as_str()
                        == local.remote_id.as_deref()
                })
            })
        {
            db::delete(&connection, local.id).map_err(|error| error.to_string())?;
        }
    }

    Ok(PullSummary {
        from,
        to,
        created,
        updated,
    })
}

medi_module! {
    manifest pull_commands;
    commands {
        crate::commands::pull::PullTimeEntries => crate::commands::pull::pull_time_entries;
    }
}
