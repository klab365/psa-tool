use clap::{Parser, Subcommand};
use comfy_table::{ColumnConstraint, ContentArrangement, Table, presets::UTF8_FULL};
use psa_tool::{
    commands::{
        AppContext, AppMediator,
        auth::{Login, Logout},
        config::{GetConfig, SetConfig, ShowConfig},
        discover::{
            DiscoverBindname, DiscoverEntity, DiscoverFind, DiscoverMyProjects, DiscoverMyresource,
        },
        doctor::Doctor,
        pull::PullTimeEntries,
        sync::SyncTimeEntries,
        time_entries::{
            CreateTimeEntry, GetTimeEntry, ListTimeEntries, RemoveTimeEntry, RestoreTimeEntry,
            UpdateTimeEntry,
        },
    },
    config, dates, interactive,
    model::TimeEntry,
    paths::AppPaths,
};

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
    /// Einen Eintrag löschen (lokal oder in Dataverse) oder wiederherstellen.
    Remove {
        /// ID des Eintrags (siehe `psa list`).
        id: i64,
        /// Gelöschten Eintrag wiederherstellen statt löschen.
        #[arg(long)]
        undo: bool,
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
    /// Konfiguration, Anmeldung und Datenbank prüfen.
    Doctor,
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

fn status_label(entry: &TimeEntry) -> String {
    let base = match entry.status.as_str() {
        "new" => "offen",
        "modified" => "geändert",
        "deleted" => "gelöscht",
        "synced" => "synchronisiert",
        other => other,
    };
    match entry
        .entry_status
        .as_deref()
        .filter(|status| !status.is_empty())
    {
        Some(status) => format!("{base} · {status}"),
        None => base.to_owned(),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut shortened: String = text.chars().take(max.saturating_sub(1)).collect();
        shortened.push('…');
        shortened
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
            "ID", "Datum", "Std.", "Projekt", "Task", "Text", "Status", "Fehler",
        ]);
    for entry in entries {
        table.add_row(vec![
            format!("#{}", entry.id),
            entry.work_date.clone(),
            format!("{}h", entry.hours),
            entry.project_name.clone().unwrap_or_default(),
            entry.task_name.clone().unwrap_or_default(),
            entry.description.clone().unwrap_or_default(),
            status_label(entry),
            truncate(entry.error.as_deref().unwrap_or(""), 40),
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli).await {
        eprintln!("{error}");
        std::process::exit(1)
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let paths = AppPaths::discover().map_err(|error| error.to_string())?;
    let config = config::load(&paths).map_err(|error| error.to_string())?;
    let mediator = AppMediator::new(AppContext::new(paths.clone(), config.clone()));

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
            let (from, to) = dates::week(date.as_deref())?;
            let entries = mediator
                .send(ListTimeEntries {
                    from: Some(from.clone()),
                    to: Some(to.clone()),
                    include_deleted: false,
                })
                .await?;
            println!("Woche {from} – {to}");
            if entries.is_empty() {
                println!("  Keine Einträge.");
            } else {
                println!("{}", entries_table(&entries));
            }
            let total: f64 = entries.iter().map(|entry| entry.hours).sum();
            println!("  Summe: {total}h")
        }
        Command::Remove { id, undo } => {
            if undo {
                mediator.send(RestoreTimeEntry { id }).await?;
                println!("Eintrag #{id} wiederhergestellt.")
            } else {
                mediator.send(RemoveTimeEntry { id }).await?;
                println!("Eintrag #{id} zum Löschen vorgemerkt.")
            }
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
                &paths,
                &config,
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
            let entry = interactive::edit_time_entry(&paths, &config, entry).await?;
            mediator.send(UpdateTimeEntry { id, entry }).await?;
            println!("Eintrag #{id} aktualisiert.");
        }
        Command::Pull { date, from, to } => {
            let summary = mediator.send(PullTimeEntries { from, to, date }).await?;
            println!(
                "Pull fertig ({} – {}): {} neu, {} aktualisiert.",
                summary.from, summary.to, summary.created, summary.updated
            );
        }
        Command::Sync { dry_run } => {
            let summary = mediator.send(SyncTimeEntries { dry_run }).await?;
            if summary.is_empty() {
                println!("Nichts zu synchronisieren – alles aktuell.");
            } else {
                for outcome in &summary.outcomes {
                    if summary.dry_run {
                        println!("[dry-run] {} {}", outcome.action, outcome.label);
                    } else if outcome.ok {
                        println!("✓ Synchronisiert: {}", outcome.label);
                    } else {
                        eprintln!(
                            "✘ Fehler bei {}: {}",
                            outcome.label,
                            outcome.error.as_deref().unwrap_or("")
                        );
                    }
                }
                println!("{}", summary.message());
            }
        }
        Command::Doctor => {
            let report = mediator.send(Doctor).await?;
            for check in &report.checks {
                let suffix = if check.ok || check.hint.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", check.hint)
                };
                println!(
                    "{} {}{suffix}",
                    if check.ok { "✓" } else { "✘" },
                    check.label
                );
            }
            if report.healthy {
                println!("Alles in Ordnung.");
            } else {
                return Err("Einige Prüfungen sind fehlgeschlagen – siehe Ausgabe oben.".into());
            }
        }
        Command::Discover { command } => {
            let output = match command {
                Discover::Myprojects => mediator.send(DiscoverMyProjects).await?,
                Discover::Entity { logical_name } => {
                    mediator.send(DiscoverEntity { logical_name }).await?
                }
                Discover::Find { text } => mediator.send(DiscoverFind { text }).await?,
                Discover::Myresource => mediator.send(DiscoverMyresource).await?,
                Discover::Bindname {
                    entity_logical_name,
                    attribute_logical_name,
                } => {
                    mediator
                        .send(DiscoverBindname {
                            entity_logical_name,
                            attribute_logical_name,
                        })
                        .await?
                }
            };
            println!("{output}");
        }
    }

    Ok(())
}
