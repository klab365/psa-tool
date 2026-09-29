//! Terminal prompts for completing partially specified CLI commands.

use crate::{
    dates,
    model::TimeEntry,
    paths::AppPaths,
    project_search::{self, LookupItem},
};
use chrono::{Datelike, Local, NaiveDate, Weekday};
use inquire::{Autocomplete, Confirm, CustomUserError, Select, Text};
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
        let date = prompt(
            "Datum (YYYY-MM-DD, 01.09., gestern, Montag, +2):",
            default.as_deref(),
        )?;
        match dates::parse_date(&date) {
            Ok(parsed) => return Ok(parsed.to_string()),
            Err(message) => eprintln!("{message}"),
        }
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
    // A single search hit is unambiguous, so skip the extra selection step.
    if results.len() == 1 {
        return Ok(results.into_iter().next());
    }
    Select::new("Treffer wählen:", results)
        .prompt()
        .map(Some)
        .map_err(|error| error.to_string())
}

async fn choose_project(paths: &AppPaths, config: &Value) -> Result<Option<LookupItem>, String> {
    let initial = project_search::projects(paths, config, "").await?;
    let query = prompt_lookup("Projekt suchen (leer = kein Projekt):", initial.clone())?
        .trim()
        .to_owned();
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
    let query = prompt_lookup("Task suchen (leer = kein Task):", initial.clone())?
        .trim()
        .to_owned();
    let results = if query.is_empty() || initial.iter().any(|item| item.to_string() == query) {
        Vec::new()
    } else {
        project_search::tasks(paths, config, project_id, &query).await?
    };
    selected_or_choose(query, initial, results)
}

fn copy_template(template: TimeEntry) -> Result<TimeEntry, String> {
    // A template is normally reused for the current day, so the date defaults
    // to today while hours, project, task and description come from the entry.
    let work_date = prompt_date(Some(Local::now().date_naive().to_string()))?;
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

fn weekday_short(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Mo",
        Weekday::Tue => "Di",
        Weekday::Wed => "Mi",
        Weekday::Thu => "Do",
        Weekday::Fri => "Fr",
        Weekday::Sat => "Sa",
        Weekday::Sun => "So",
    }
}

fn history_date(work_date: &str, today: NaiveDate) -> String {
    let Ok(date) = NaiveDate::parse_from_str(work_date, "%Y-%m-%d") else {
        return work_date.to_owned();
    };
    match (today - date).num_days() {
        0 => "Heute".to_owned(),
        1 => "Gestern".to_owned(),
        _ => format!(
            "{} {}",
            weekday_short(date.weekday()),
            date.format("%d.%m.")
        ),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut truncated: String = text.chars().take(max.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

/// Compact, aligned one-line label used by the history selection lists.
fn history_label(entry: &TimeEntry, today: NaiveDate) -> String {
    let mut context = entry
        .project_name
        .clone()
        .unwrap_or_else(|| "Ohne Projekt".to_owned());
    if let Some(task) = entry.task_name.as_deref().filter(|task| !task.is_empty()) {
        context.push_str(" › ");
        context.push_str(task);
    }
    format!(
        "#{:<3} {:<10} {:>4.1}h  {:<32} {}",
        entry.id,
        history_date(&entry.work_date, today),
        entry.hours,
        truncate(&context, 32),
        truncate(entry.description.as_deref().unwrap_or(""), 48),
    )
}

/// A history entry rendered with [`history_label`] so that both the visible
/// option and inquire's built-in filtering use the same text.
struct HistoryChoice(TimeEntry);

impl std::fmt::Display for HistoryChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&history_label(&self.0, Local::now().date_naive()))
    }
}

/// Selects a previous entry and creates a new, unsynchronised copy with only
/// date, hours and description changed by the user.
pub fn reuse_time_entry(entries: Vec<TimeEntry>) -> Result<TimeEntry, String> {
    if entries.is_empty() {
        return Err("Im Verlauf sind keine Einträge vorhanden.".into());
    }
    let choices = entries.into_iter().map(HistoryChoice).collect();
    let template = Select::new("Eintrag aus Verlauf wählen:", choices)
        .with_page_size(15)
        .with_help_message("Tippen zum Filtern · ↑/↓ wählen · Enter übernehmen")
        .prompt()
        .map_err(|error| error.to_string())?;
    copy_template(template.0)
}

enum AddMode {
    New,
    History(Box<TimeEntry>),
}

impl std::fmt::Display for AddMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::New => write!(formatter, "Neuen Eintrag erfassen"),
            Self::History(entry) => {
                formatter.write_str(&history_label(entry, Local::now().date_naive()))
            }
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
        .with_page_size(15)
        .with_help_message("Tippen zum Filtern · ↑/↓ wählen · Enter übernehmen")
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

    // Local fields first so the core entry is captured before the optional
    // Dataverse lookups, which may be slower or require a login.
    let work_date = match date {
        Some(date) => dates::parse_date(&date)?.to_string(),
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

/// Interactively edits an existing entry. Date, hours and description are
/// prefilled with the current values; project and task are only touched after
/// an explicit confirmation so that editing works without Dataverse access.
pub async fn edit_time_entry(
    paths: &AppPaths,
    config: &Value,
    entry: TimeEntry,
) -> Result<TimeEntry, String> {
    let work_date = prompt_date(Some(entry.work_date.clone()))?;
    let hours = prompt_hours(Some(entry.hours))?;
    let description = prompt("Beschreibung:", entry.description.as_deref())?;
    let change_lookups = Confirm::new("Projekt oder Task ändern?")
        .with_default(false)
        .prompt()
        .map_err(|error| error.to_string())?;
    let (project_id, project_name, task_id, task_name) = if change_lookups {
        let project = choose_project(paths, config).await?;
        let project_id = project.as_ref().map(|item| item.id.clone());
        let project_name = project.as_ref().map(|item| item.name.clone());
        let task = choose_task(paths, config, project_id.as_deref()).await?;
        (
            project_id,
            project_name,
            task.as_ref().map(|item| item.id.clone()),
            task.as_ref().map(|item| item.name.clone()),
        )
    } else {
        (
            entry.project_id,
            entry.project_name,
            entry.task_id,
            entry.task_name,
        )
    };
    Ok(TimeEntry {
        id: entry.id,
        work_date,
        project_id,
        project_name,
        task_id,
        task_name,
        hours,
        description: Some(description),
        remote_id: entry.remote_id,
        status: entry.status,
        entry_status: entry.entry_status,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(work_date: &str, description: &str) -> TimeEntry {
        TimeEntry {
            id: 7,
            work_date: work_date.into(),
            project_id: None,
            project_name: Some("Projekt A".into()),
            task_id: None,
            task_name: Some("Task A".into()),
            hours: 7.5,
            description: Some(description.into()),
            remote_id: None,
            status: "new".into(),
            entry_status: None,
            error: None,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 3).unwrap()
    }

    #[test]
    fn history_label_uses_relative_dates_and_alignment() {
        let label = history_label(&entry("2026-09-03", "Review"), today());
        assert!(label.starts_with("#7   Heute"), "{label}");
        assert!(label.contains("7.5h"));
        assert!(label.contains("Projekt A › Task A"));
        assert!(label.ends_with("Review"));

        let older = history_label(&entry("2026-08-31", "Review"), today());
        assert!(older.contains("Mo 31.08."), "{older}");
    }

    #[test]
    fn history_label_truncates_long_descriptions() {
        let label = history_label(&entry("2026-09-03", &"x".repeat(80)), today());
        assert!(label.ends_with('…'));
    }
}
