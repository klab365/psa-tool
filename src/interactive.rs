//! Terminal prompts for completing partially specified CLI commands.

use crate::{
    model::TimeEntry,
    paths::AppPaths,
    project_search::{self, LookupItem},
};
use chrono::{Local, NaiveDate};
use inquire::{Autocomplete, CustomUserError, Select, Text};
use serde_json::Value;

fn prompt(label: &str, default: Option<&str>) -> Result<String, String> {
    let question = match default {
        Some(value) => Text::new(label).with_default(value),
        None => Text::new(label),
    };
    question.prompt().map_err(|error| error.to_string())
}

fn prompt_date(default: Option<String>) -> Result<String, String> {
    loop {
        let date = prompt("Datum (YYYY-MM-DD):", default.as_deref())?;
        if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_ok() {
            return Ok(date);
        }
        eprintln!("Ungültiges Datum (YYYY-MM-DD erwartet).");
    }
}

fn prompt_hours(default: Option<f64>) -> Result<f64, String> {
    loop {
        let default = default.map(|value| value.to_string());
        let hours = prompt("Stunden:", default.as_deref())?;
        match hours.parse::<f64>() {
            Ok(value) if value > 0.0 => return Ok(value),
            _ => eprintln!("Stunden müssen eine positive Zahl sein."),
        }
    }
}

#[derive(Clone)]
struct LookupAutocomplete(Vec<LookupItem>);

impl Autocomplete for LookupAutocomplete {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let input = input.to_lowercase();
        Ok(self
            .0
            .iter()
            .filter(|item| item.name.to_lowercase().contains(&input))
            .take(15)
            .map(ToString::to_string)
            .collect())
    }

    fn get_completion(
        &mut self,
        _input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Option<String>, CustomUserError> {
        Ok(highlighted_suggestion)
    }
}

fn prompt_lookup(label: &str, choices: Vec<LookupItem>) -> Result<String, String> {
    Text::new(label)
        .with_autocomplete(LookupAutocomplete(choices))
        .prompt()
        .map_err(|error| error.to_string())
}

fn selected_or_choose(
    query: String,
    initial: Vec<LookupItem>,
    results: Vec<LookupItem>,
) -> Result<Option<LookupItem>, String> {
    if query.is_empty() {
        return Ok(None);
    }
    if let Some(item) = initial.into_iter().find(|item| item.to_string() == query) {
        return Ok(Some(item));
    }
    if results.is_empty() {
        return Err(format!("Kein Treffer gefunden für '{query}'."));
    }
    Select::new("Treffer wählen:", results)
        .prompt()
        .map(Some)
        .map_err(|error| error.to_string())
}

async fn choose_project(paths: &AppPaths, config: &Value) -> Result<Option<LookupItem>, String> {
    let initial = project_search::projects(paths, config, "").await?;
    let query = prompt_lookup("Projekt suchen (leer = kein Projekt):", initial.clone())?;
    let results = if query.is_empty() || initial.iter().any(|item| item.to_string() == query) {
        Vec::new()
    } else {
        project_search::projects(paths, config, &query).await?
    };
    selected_or_choose(query, initial, results)
}

async fn choose_task(
    paths: &AppPaths,
    config: &Value,
    project_id: Option<&str>,
) -> Result<Option<LookupItem>, String> {
    let initial = project_search::tasks(paths, config, project_id, "").await?;
    let query = prompt_lookup("Task suchen (leer = kein Task):", initial.clone())?;
    let results = if query.is_empty() || initial.iter().any(|item| item.to_string() == query) {
        Vec::new()
    } else {
        project_search::tasks(paths, config, project_id, &query).await?
    };
    selected_or_choose(query, initial, results)
}

fn copy_template(template: TimeEntry) -> Result<TimeEntry, String> {
    let work_date = prompt_date(Some(template.work_date.clone()))?;
    let hours = prompt_hours(Some(template.hours))?;
    let description = prompt("Beschreibung:", template.description.as_deref())?;
    Ok(TimeEntry {
        id: 0,
        work_date,
        hours,
        description: Some(description),
        remote_id: None,
        status: "new".into(),
        entry_status: None,
        error: None,
        ..template
    })
}

/// Selects a previous entry and creates a new, unsynchronised copy with only
/// date, hours and description changed by the user.
pub fn reuse_time_entry(entries: Vec<TimeEntry>) -> Result<TimeEntry, String> {
    if entries.is_empty() {
        return Err("Im Verlauf sind keine Einträge vorhanden.".into());
    }
    let template = Select::new("Eintrag aus Verlauf wählen:", entries)
        .prompt()
        .map_err(|error| error.to_string())?;
    copy_template(template)
}

enum AddMode {
    New,
    History(Box<TimeEntry>),
}

impl std::fmt::Display for AddMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::New => write!(formatter, "Neuen Eintrag erfassen"),
            Self::History(entry) => write!(formatter, "Aus Verlauf: {entry}"),
        }
    }
}

/// Lets a bare `psa add` start from either a blank form or a recent entry.
pub fn choose_add_mode(entries: Vec<TimeEntry>) -> Result<Option<TimeEntry>, String> {
    let choices = std::iter::once(AddMode::New)
        .chain(
            entries
                .into_iter()
                .map(|entry| AddMode::History(Box::new(entry))),
        )
        .collect();
    match Select::new("Was möchtest du erfassen?", choices)
        .prompt()
        .map_err(|error| error.to_string())?
    {
        AddMode::New => Ok(None),
        AddMode::History(entry) => copy_template(*entry).map(Some),
    }
}

pub struct AddInput {
    pub interactive: bool,
    pub date: Option<String>,
    pub hours: Option<f64>,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub task_id: Option<String>,
    pub task_name: Option<String>,
}

/// Prompts only for values absent from `psa add` flags when interactive mode is enabled.
pub async fn complete_time_entry(
    paths: &AppPaths,
    config: &Value,
    input: AddInput,
) -> Result<TimeEntry, String> {
    let AddInput {
        interactive,
        date,
        hours,
        description,
        project_id,
        project_name,
        task_id,
        task_name,
    } = input;
    let project = if interactive && project_id.is_none() && project_name.is_none() {
        choose_project(paths, config).await?
    } else {
        None
    };
    let project_id = project
        .as_ref()
        .map(|project| project.id.clone())
        .or(project_id);
    let project_name = project
        .as_ref()
        .map(|project| project.name.clone())
        .or(project_name);

    let task = if interactive && task_id.is_none() && task_name.is_none() {
        choose_task(paths, config, project_id.as_deref()).await?
    } else {
        None
    };
    let task_id = task.as_ref().map(|task| task.id.clone()).or(task_id);
    let task_name = task.as_ref().map(|task| task.name.clone()).or(task_name);

    let work_date = match date {
        Some(date) => {
            NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .map_err(|_| "Ungültiges Datum (YYYY-MM-DD erwartet).".to_owned())?;
            date
        }
        None if interactive => prompt_date(Some(Local::now().date_naive().to_string()))?,
        None => Local::now().date_naive().to_string(),
    };
    let hours = match hours {
        Some(hours) if hours > 0.0 => hours,
        Some(_) => return Err("Stunden müssen eine positive Zahl sein.".into()),
        None if interactive => prompt_hours(Some(8.0))?,
        None => return Err("--hours angeben oder --interactive verwenden.".into()),
    };
    let description = match description {
        Some(description) => description,
        None if interactive => prompt("Beschreibung:", None)?,
        None => return Err("--description angeben oder --interactive verwenden.".into()),
    };

    Ok(TimeEntry {
        id: 0,
        work_date,
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
    })
}
