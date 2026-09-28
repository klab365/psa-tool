use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use chrono_tz::Tz;
use clap::{Parser, Subcommand};
use psa_tool::{
    commands::{
        AppContext, AppMediator,
        auth::{Login, Logout},
        config::{GetConfig, SetConfig, ShowConfig},
        time_entries::{CreateTimeEntry, ListTimeEntries, RemoveTimeEntry},
    },
    config,
    dataverse::Client,
    db,
    model::TimeEntry,
    paths::AppPaths,
};
use serde_json::{Value, json};
#[derive(Parser)]
#[command(
    name = "psa-rust",
    about = "CLI zum Erfassen und Synchronisieren von Zeiteinträgen in Dynamics 365 Project Operations."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Login,
    Logout,
    Config {
        #[command(subcommand)]
        command: Config,
    },
    Add {
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        hours: Option<f64>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        project_id: Option<String>,
        #[arg(long)]
        project_name: Option<String>,
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long)]
        task_name: Option<String>,
    },
    Edit {
        id: i64,
    },
    Remove {
        id: i64,
    },
    Week {
        date: Option<String>,
    },
    List,
    Pull {
        date: Option<String>,
        #[arg(long = "from")]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },
    Sync {
        #[arg(long)]
        dry_run: bool,
    },
    Discover {
        #[command(subcommand)]
        command: Discover,
    },
}
#[derive(Subcommand)]
enum Config {
    Show,
    Get { key: String },
    Set { key: String, value: String },
}
#[derive(Subcommand)]
enum Discover {
    Entity {
        logical_name: String,
    },
    Find {
        text: String,
    },
    Myresource,
    Myprojects,
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
        Some(x) => NaiveDate::parse_from_str(x, "%Y-%m-%d")
            .map_err(|_| "Ungültiges Datum (YYYY-MM-DD erwartet)".to_string())?,
        None => Local::now().date_naive(),
    };
    let start = d - Duration::days((d.weekday().num_days_from_monday()) as i64);
    Ok((start.to_string(), (start + Duration::days(6)).to_string()))
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
    let dur = if value(m, "/durationUnit") == "hours" {
        e.hours
    } else {
        (e.hours * 60.).round()
    };
    b.insert(value(m, "/durationField").to_owned(), json!(dur));
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
            eprintln!("✘ Fehler bei {label}: {x}")
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
            for e in entries {
                println!(
                    "#{} {} {}h {} {} [{}]",
                    e.id,
                    e.work_date,
                    e.hours,
                    e.project_name.unwrap_or_default(),
                    e.task_name.unwrap_or_default(),
                    e.status
                )
            }
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
            let total: f64 = es.iter().map(|e| e.hours).sum();
            for e in es {
                println!(
                    "  #{} {} {}h {} [{}]",
                    e.id,
                    e.work_date,
                    e.hours,
                    e.project_name.unwrap_or_default(),
                    e.status
                )
            }
            println!("  Summe: {total}h")
        }
        Command::Remove { id } => {
            mediator.send(RemoveTimeEntry { id }).await?;
            println!("Eintrag #{id} zum Löschen vorgemerkt.")
        }
        Command::Add {
            date,
            hours,
            description,
            project_id,
            project_name,
            task_id,
            task_name,
        } => {
            let (Some(hours), Some(description)) = (hours, description) else {
                return Err(
                    "Rust add ist nicht-interaktiv: --hours und --description angeben.".into(),
                );
            };
            let e = TimeEntry {
                id: 0,
                work_date: date.unwrap_or_else(|| Local::now().date_naive().to_string()),
                project_id,
                project_name,
                task_id,
                task_name,
                hours,
                description: Some(description),
                remote_id: None,
                status: "new".into(),
                entry_status: None,
                error: None,
            };
            let id = mediator.send(CreateTimeEntry { entry: e }).await?;
            println!("Eintrag #{id} erfasst.")
        }
        Command::Edit { id } => {
            return Err(format!(
                "Interaktives edit für Eintrag #{id} ist noch nicht verfügbar."
            ));
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
}
