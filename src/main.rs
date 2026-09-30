use chrono::{DateTime, Datelike, Duration, Local, Utc};
use chrono_tz::Tz;
use clap::{Parser, Subcommand};
use comfy_table::{ColumnConstraint, ContentArrangement, Table, presets::UTF8_FULL};
use psa_tool::{
    commands::{
        AppContext, AppMediator,
        auth::{Login, Logout},
        config::{GetConfig, SetConfig, ShowConfig},
        time_entries::{
            CreateTimeEntry, GetTimeEntry, ListTimeEntries, RemoveTimeEntry, UpdateTimeEntry,
        },
    },
    config,
    dataverse::Client,
    dates, db, interactive,
    model::TimeEntry,
    paths::AppPaths,
};
use serde_json::{Value, json};
#[derive(Parser)]
#[command(
    name = "psa",
    version,
    about = "CLI zum Erfassen und Synchronisieren von Zeiteinträgen in Dynamics 365 Project Operations."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Bei Microsoft Entra ID anmelden.
    Login,
    /// Die gespeicherte Anmeldung verwerfen.
    Logout,
    /// Konfiguration anzeigen und ändern.
    Config {
        #[command(subcommand)]
        command: Config,
    },
    /// Einen Zeiteintrag erfassen.
    Add {
        /// Fehlende Werte abfragen sowie Projekt und Task suchen.
        #[arg(long)]
        interactive: bool,
        /// Datum des Eintrags (YYYY-MM-DD, 01.09., gestern, Montag, +2).
        #[arg(long)]
        date: Option<String>,
        /// Gearbeitete Stunden.
        #[arg(long)]
        hours: Option<f64>,
        /// Beschreibung des Eintrags.
        #[arg(long)]
        description: Option<String>,
        /// Dataverse-ID des Projekts.
        #[arg(long)]
        project_id: Option<String>,
        /// Projektname ohne Dataverse-Zugriff.
        #[arg(long)]
        project_name: Option<String>,
        /// Dataverse-ID des Tasks.
        #[arg(long)]
        task_id: Option<String>,
        /// Taskname ohne Dataverse-Zugriff.
        #[arg(long)]
        task_name: Option<String>,
    },
    /// Einen bestehenden Eintrag interaktiv bearbeiten.
    Edit {
        /// ID des Eintrags (siehe `psa list`).
        id: i64,
    },
    /// Einen Eintrag löschen (lokal oder in Dataverse).
    Remove {
        /// ID des Eintrags (siehe `psa list`).
        id: i64,
    },
    /// Alle Einträge einer Woche anzeigen.
    Week {
        /// Beliebiger Tag der Woche (Standard: heute).
        date: Option<String>,
    },
    /// Alle lokalen Einträge tabellarisch anzeigen.
    List,
    /// Einen der letzten Einträge als Vorlage für einen neuen Eintrag verwenden.
    History {
        /// Anzahl der angebotenen Vorlagen.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Einträge aus Dataverse abrufen.
    Pull {
        /// Einzelner Tag (Standard: aktuelle Woche).
        date: Option<String>,
        /// Startdatum des Zeitraums.
        #[arg(long = "from", value_name = "FROM")]
        from: Option<String>,
        /// Enddatum des Zeitraums.
        #[arg(long, value_name = "TO")]
        to: Option<String>,
    },
    /// Lokale Änderungen nach Dataverse übertragen.
    Sync {
        /// Nur anzeigen, was übertragen würde.
        #[arg(long)]
        dry_run: bool,
    },
    /// Dataverse-Metadaten untersuchen.
    Discover {
        #[command(subcommand)]
        command: Discover,
    },
}
#[derive(Subcommand)]
enum Config {
    /// Gesamte Konfiguration ausgeben.
    Show,
    /// Einzelnen Konfigurationswert ausgeben.
    Get {
        /// Punktgetrennter Schlüssel, z. B. `mapping.timezone`.
        key: String,
    },
    /// Einen Konfigurationswert setzen.
    Set {
        /// Punktgetrennter Schlüssel, z. B. `environmentUrl`.
        key: String,
        /// Neuer Wert.
        value: String,
    },
}
#[derive(Subcommand)]
enum Discover {
    /// Metadaten einer Entität anzeigen.
    Entity {
        /// Logischer Name, z. B. `msdyn_timeentry`.
        logical_name: String,
    },
    /// Entitäten nach Text durchsuchen.
    Find {
        /// Suchtext für den logischen Namen.
        text: String,
    },
    /// Die eigene Bookable-Resource-ID ermitteln.
    Myresource,
    /// Eigene Projekte auflisten.
    Myprojects,
    /// Bindenamen eines Lookup-Attributs ermitteln.
    Bindname {
        entity_logical_name: String,
        attribute_logical_name: String,
    },
}
fn value<'a>(c: &'a Value, k: &str) -> &'a str {
    c.pointer(k).and_then(Value::as_str).unwrap_or("")
}
fn week(date: Option<&str>) -> Result<(String, String), String> {
    let d = match date {
        Some(x) => dates::parse_date(x)?,
        None => Local::now().date_naive(),
    };
    let start = d - Duration::days((d.weekday().num_days_from_monday()) as i64);
    Ok((start.to_string(), (start + Duration::days(6)).to_string()))
}
fn status_label(status: &str) -> &str {
    match status {
        "new" => "offen",
        "modified" => "geändert",
        "deleted" => "gelöscht",
        "synced" => "synchronisiert",
        other => other,
    }
}
fn entries_table(entries: &[TimeEntry]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        // Fits the table to the terminal width when running in a tty;
        // falls back to the full content width when piped.
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            "ID", "Datum", "Std.", "Projekt", "Task", "Text", "Status",
        ]);
    for entry in entries {
        table.add_row(vec![
            format!("#{}", entry.id),
            entry.work_date.clone(),
            format!("{}h", entry.hours),
            entry.project_name.clone().unwrap_or_default(),
            entry.task_name.clone().unwrap_or_default(),
            entry.description.clone().unwrap_or_default(),
            status_label(&entry.status).to_owned(),
        ]);
    }
    // Keep the compact columns on a single line; project, task and text may
    // shrink and wrap so the table still fits narrow terminals.
    for index in [0, 1, 2] {
        if let Some(column) = table.column_mut(index) {
            column.set_constraint(ColumnConstraint::ContentWidth);
        }
    }
    table
}
fn config_bool(value: &Value) -> bool {
    value.as_bool().unwrap_or_else(|| {
        value
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    })
}
fn local_date(raw: &str, mapping: &Value) -> String {
    if config_bool(&mapping["dateOnly"]) {
        return raw.get(..10).unwrap_or(raw).to_owned();
    }
    let parsed = DateTime::parse_from_rfc3339(raw).map(|v| v.with_timezone(&Utc));
    let timezone = value(mapping, "/timezone")
        .parse::<Tz>()
        .unwrap_or(chrono_tz::UTC);
    parsed
        .map(|v| v.with_timezone(&timezone).date_naive().to_string())
        .unwrap_or_else(|_| raw.get(..10).unwrap_or(raw).to_owned())
}
fn entry_body(e: &TimeEntry, c: &Value) -> Value {
    let m = &c["mapping"];
    let mut b = serde_json::Map::new();
    let date = if config_bool(&m["dateOnly"]) {
        e.work_date.clone()
    } else {
        format!("{}T00:00:00Z", e.work_date)
    };
    b.insert(value(m, "/dateField").to_owned(), json!(date));
    if value(m, "/durationUnit") == "hours" {
        b.insert(value(m, "/durationField").to_owned(), json!(e.hours));
    } else {
        // `msdyn_duration` is an Edm.Int32. Keep the rounded minute value an
        // integer in JSON; serializing the intermediate f64 emits e.g. 360.0.
        let minutes = (e.hours * 60.).round() as i32;
        b.insert(value(m, "/durationField").to_owned(), json!(minutes));
    }
    b.insert(
        value(m, "/descriptionField").to_owned(),
        json!(e.description),
    );
    for (id, bind, set) in [
        (&e.project_id, "/projectLookupBind", "/projectEntitySet"),
        (&e.task_id, "/taskLookupBind", "/taskEntitySet"),
    ] {
        if let Some(id) = id {
            b.insert(
                value(m, bind).to_owned(),
                json!(format!("/{}({id})", value(m, set))),
            );
        }
    }
    if !value(c, "/resourceId").is_empty() {
        b.insert(
            value(m, "/resourceLookupBind").to_owned(),
            json!(format!("/bookableresources({})", value(c, "/resourceId"))),
        );
    }
    Value::Object(b)
}
async fn client(paths: &AppPaths, c: &Value) -> Result<Client, String> {
    Client::new(paths, c, false)
        .await
        .map_err(|e| e.to_string())
}
async fn pull(
    conn: &rusqlite::Connection,
    paths: &AppPaths,
    c: &Value,
    from: Option<String>,
    to: Option<String>,
    date: Option<String>,
) -> Result<(), String> {
    if value(c, "/resourceId").is_empty() {
        return Err(
            "resourceId ist nicht gesetzt. Zuerst 'psa discover myresource' ausführen.".into(),
        );
    }
    let (from, to) = if from.is_none() && to.is_none() {
        week(date.as_deref())?
    } else {
        (from.unwrap_or_default(), to.unwrap_or_default())
    };
    let m = &c["mapping"];
    let filter = format!(
        "{} eq {} and {} ge {}T00:00:00Z and {} le {}T23:59:59Z",
        value(m, "/resourceLookupValueField"),
        value(c, "/resourceId"),
        value(m, "/dateField"),
        from,
        value(m, "/dateField"),
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
    .map(|x| value(m, x))
    .join(",");
    let data = client(paths, c)
        .await?
        .get(
            &format!(
                "/{}?$filter={filter}&$select={select}&$orderby={} asc",
                value(m, "/timeEntryEntitySet"),
                value(m, "/dateField")
            ),
            true,
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut created = 0;
    let mut updated = 0;
    for r in data["value"].as_array().into_iter().flatten() {
        let remote = r[value(m, "/timeEntryIdField")].as_str().unwrap_or("");
        let raw = r[value(m, "/dateField")].as_str().unwrap_or("");
        let wd = local_date(raw, m);
        let e = TimeEntry {
            id: 0,
            work_date: wd,
            project_id: r[value(m, "/projectLookupValueField")]
                .as_str()
                .map(str::to_owned),
            project_name: r[format!(
                "{}@OData.Community.Display.V1.FormattedValue",
                value(m, "/projectLookupValueField")
            )]
            .as_str()
            .map(str::to_owned),
            task_id: r[value(m, "/taskLookupValueField")]
                .as_str()
                .map(str::to_owned),
            task_name: r[format!(
                "{}@OData.Community.Display.V1.FormattedValue",
                value(m, "/taskLookupValueField")
            )]
            .as_str()
            .map(str::to_owned),
            hours: r[value(m, "/durationField")].as_f64().unwrap_or(0.)
                / if value(m, "/durationUnit") == "hours" {
                    1.
                } else {
                    60.
                },
            description: r[value(m, "/descriptionField")].as_str().map(str::to_owned),
            remote_id: Some(remote.into()),
            status: "synced".into(),
            entry_status: r[format!(
                "{}@OData.Community.Display.V1.FormattedValue",
                value(m, "/entryStatusField")
            )]
            .as_str()
            .map(str::to_owned),
            error: None,
        };
        match db::by_remote_id(conn, remote).map_err(|e| e.to_string())? {
            None => {
                db::insert(conn, &e, Some(remote), "synced").map_err(|e| e.to_string())?;
                created += 1
            }
            Some(local) if local.status == "synced" => {
                db::update_synced(conn, local.id, &e).map_err(|e| e.to_string())?;
                updated += 1
            }
            Some(local) => eprintln!(
                "⚠ Konflikt bei Eintrag #{}: lokaler Status '{}'",
                local.id, local.status
            ),
        }
    }
    println!("Pull fertig ({from} – {to}): {created} neu, {updated} aktualisiert.");
    // A remote record missing from this range was deleted in Dataverse. Keep
    // unsynchronised local work intact, exactly as the Python implementation.
    for local in db::list(conn, Some(&from), Some(&to), false).map_err(|e| e.to_string())? {
        if local.status == "synced"
            && local.remote_id.is_some()
            && !data["value"].as_array().is_some_and(|rows| {
                rows.iter().any(|row| {
                    row[value(m, "/timeEntryIdField")].as_str() == local.remote_id.as_deref()
                })
            })
        {
            db::delete(conn, local.id).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
async fn sync(
    conn: &rusqlite::Connection,
    paths: &AppPaths,
    c: &Value,
    dry: bool,
) -> Result<(), String> {
    let pending = db::pending(conn).map_err(|e| e.to_string())?;
    if pending.is_empty() {
        println!("Nichts zu synchronisieren – alles aktuell.");
        return Ok(());
    }
    // A dry run must remain fully local: it must not require a token or make
    // an HTTP request.
    let dv = if dry {
        None
    } else {
        Some(client(paths, c).await?)
    };
    let m = &c["mapping"];
    for e in pending {
        let label = format!(
            "{} | {} | {}h",
            e.work_date,
            e.project_name.as_deref().unwrap_or("?"),
            e.hours
        );
        let result = match e.status.as_str() {
            "deleted" => {
                if dry {
                    println!("[dry-run] DELETE {label}");
                    Ok(())
                } else {
                    dv.as_ref()
                        .expect("Dataverse client for non-dry sync")
                        .delete(
                            value(m, "/timeEntryEntitySet"),
                            e.remote_id.as_deref().unwrap_or(""),
                        )
                        .await
                        .map_err(|x| x.to_string())
                        .and_then(|_| db::delete(conn, e.id).map_err(|x| x.to_string()))
                }
            }
            "new" => {
                if dry {
                    println!("[dry-run] CREATE {label}");
                    Ok(())
                } else {
                    match dv
                        .as_ref()
                        .expect("Dataverse client for non-dry sync")
                        .post(value(m, "/timeEntryEntitySet"), &entry_body(&e, c))
                        .await
                    {
                        Ok(r) => db::synced(
                            conn,
                            e.id,
                            r[value(m, "/timeEntryIdField")].as_str().unwrap_or(""),
                        )
                        .map_err(|x| x.to_string()),
                        Err(x) => Err(x.to_string()),
                    }
                }
            }
            _ => {
                if dry {
                    println!("[dry-run] UPDATE {label}");
                    Ok(())
                } else {
                    dv.as_ref()
                        .expect("Dataverse client for non-dry sync")
                        .patch(
                            value(m, "/timeEntryEntitySet"),
                            e.remote_id.as_deref().unwrap_or(""),
                            &entry_body(&e, c),
                        )
                        .await
                        .map_err(|x| x.to_string())
                        .and_then(|_| {
                            db::synced(conn, e.id, e.remote_id.as_deref().unwrap_or(""))
                                .map_err(|x| x.to_string())
                        })
                }
            }
        };
        if let Err(x) = result {
            db::error(conn, e.id, &x).map_err(|z| z.to_string())?;
            eprintln!("✘ Fehler bei {label}: {x}");
        } else {
            println!("✓ Synchronisiert: {label}");
        }
    }
    Ok(())
}
#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
async fn run(cli: Cli) -> Result<(), String> {
    let p = AppPaths::discover().map_err(|e| e.to_string())?;
    let c = config::load(&p).map_err(|e| e.to_string())?;
    let mediator = AppMediator::new(AppContext::new(p.clone(), c.clone()));
    match cli.command {
        Command::Config {
            command: Config::Show,
        } => println!("{}", mediator.send(ShowConfig).await?),
        Command::Config {
            command: Config::Get { key },
        } => println!("{}", mediator.send(GetConfig { key }).await?),
        Command::Config {
            command: Config::Set { key, value },
        } => println!("{}", mediator.send(SetConfig { key, value }).await?),
        Command::Logout => println!("{}", mediator.send(Logout).await?),
        Command::Login => println!("{}", mediator.send(Login).await?),
        Command::List => {
            let entries = mediator
                .send(ListTimeEntries {
                    from: None,
                    to: None,
                    include_deleted: true,
                })
                .await?;
            if entries.is_empty() {
                println!("Keine Einträge vorhanden.");
            } else {
                println!("{}", entries_table(&entries));
            }
        }
        Command::History { limit } => {
            let entries = mediator
                .send(ListTimeEntries {
                    from: None,
                    to: None,
                    include_deleted: false,
                })
                .await?;
            let templates = entries.into_iter().take(limit).collect();
            let entry = interactive::reuse_time_entry(templates)?;
            let id = mediator.send(CreateTimeEntry { entry }).await?;
            println!("Eintrag #{id} aus Verlauf erfasst.")
        }
        Command::Week { date } => {
            let (a, b) = week(date.as_deref())?;
            let es = mediator
                .send(ListTimeEntries {
                    from: Some(a.clone()),
                    to: Some(b.clone()),
                    include_deleted: false,
                })
                .await?;
            println!("Woche {a} – {b}");
            if es.is_empty() {
                println!("  Keine Einträge.");
            } else {
                println!("{}", entries_table(&es));
            }
            let total: f64 = es.iter().map(|e| e.hours).sum();
            println!("  Summe: {total}h")
        }
        Command::Remove { id } => {
            mediator.send(RemoveTimeEntry { id }).await?;
            println!("Eintrag #{id} zum Löschen vorgemerkt.")
        }
        Command::Add {
            interactive: force_interactive,
            date,
            hours,
            description,
            project_id,
            project_name,
            task_id,
            task_name,
        } => {
            let choose_history = date.is_none()
                && hours.is_none()
                && description.is_none()
                && project_id.is_none()
                && project_name.is_none()
                && task_id.is_none()
                && task_name.is_none();
            if choose_history {
                let entries = mediator
                    .send(ListTimeEntries {
                        from: None,
                        to: None,
                        include_deleted: false,
                    })
                    .await?;
                if let Some(entry) =
                    interactive::choose_add_mode(entries.into_iter().take(10).collect())?
                {
                    let id = mediator.send(CreateTimeEntry { entry }).await?;
                    println!("Eintrag #{id} aus Verlauf erfasst.");
                    return Ok(());
                }
            }
            let interactive = force_interactive || hours.is_none() || description.is_none();
            let entry = interactive::complete_time_entry(
                &p,
                &c,
                interactive::AddInput {
                    interactive,
                    date,
                    hours,
                    description,
                    project_id,
                    project_name,
                    task_id,
                    task_name,
                },
            )
            .await?;
            let id = mediator.send(CreateTimeEntry { entry }).await?;
            println!("Eintrag #{id} erfasst.")
        }
        Command::Edit { id } => {
            let entry = mediator.send(GetTimeEntry { id }).await?;
            let entry = interactive::edit_time_entry(&p, &c, entry).await?;
            mediator.send(UpdateTimeEntry { id, entry }).await?;
            println!("Eintrag #{id} aktualisiert.");
        }
        Command::Pull { date, from, to } => {
            let d = db::open(&p).map_err(|e| e.to_string())?;
            pull(&d, &p, &c, from, to, date).await?
        }
        Command::Sync { dry_run } => {
            let d = db::open(&p).map_err(|e| e.to_string())?;
            sync(&d, &p, &c, dry_run).await?
        }
        Command::Discover { command } => discover(&p, &c, command).await?,
    };
    Ok(())
}
async fn discover(p: &AppPaths, c: &Value, x: Discover) -> Result<(), String> {
    let d = client(p, c).await?;
    let path = match x {
        Discover::Entity { logical_name } => format!(
            "/EntityDefinitions(LogicalName='{logical_name}')?$select=LogicalName,EntitySetName,PrimaryIdAttribute,PrimaryNameAttribute"
        ),
        Discover::Find { text } => format!(
            "/EntityDefinitions?$select=LogicalName,EntitySetName,DisplayName&$filter=contains(LogicalName,'{}')",
            text.replace('\'', "''")
        ),
        Discover::Myresource => "/WhoAmI".into(),
        Discover::Myprojects => return Err("discover myprojects ist noch nicht verfügbar.".into()),
        Discover::Bindname {
            entity_logical_name,
            attribute_logical_name,
        } => format!(
            "/EntityDefinitions(LogicalName='{entity_logical_name}')/Attributes(LogicalName='{attribute_logical_name}')/Microsoft.Dynamics.CRM.LookupAttributeMetadata?$select=SchemaName,LogicalName"
        ),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&d.get(&path, false).await.map_err(|e| e.to_string())?)
            .unwrap()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_config_boolean_and_timezone_match_python_config() {
        let mapping = json!({"dateOnly": "false", "timezone": "Europe/Zurich"});
        assert!(!config_bool(&mapping["dateOnly"]));
        assert_eq!(local_date("2026-08-31T22:00:00Z", &mapping), "2026-09-01");
    }

    #[test]
    fn entry_body_serializes_minute_durations_as_integers() {
        let entry = TimeEntry {
            id: 1,
            work_date: "2026-09-29".into(),
            project_id: None,
            project_name: None,
            task_id: None,
            task_name: None,
            hours: 6.0,
            description: None,
            remote_id: None,
            status: "pending".into(),
            entry_status: None,
            error: None,
        };

        let body = entry_body(&entry, &config::default_config());

        assert_eq!(body["msdyn_duration"].as_i64(), Some(360));
    }
}
