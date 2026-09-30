//! Dataverse lookups used by the interactive time-entry prompts.

use crate::{dataverse::Client, paths::AppPaths};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupItem {
    pub id: String,
    pub name: String,
}

impl std::fmt::Display for LookupItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name)
    }
}

fn value<'a>(config: &'a Value, path: &str) -> Result<&'a str, String> {
    config
        .pointer(path)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Konfiguration {path} fehlt."))
}

fn config_bool(value: &Value) -> bool {
    value.as_bool().unwrap_or_else(|| {
        value
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    })
}

fn escape(value: &str) -> String {
    value.replace('\'', "''")
}

async fn client(paths: &AppPaths, config: &Value) -> Result<Client, String> {
    Client::new(paths, config, false)
        .await
        .map_err(|error| error.to_string())
}

async fn my_project_ids(
    paths: &AppPaths,
    config: &Value,
    only_active: bool,
) -> Result<Option<Vec<String>>, String> {
    let resource_id = config["resourceId"].as_str().unwrap_or("");
    if resource_id.is_empty() {
        return Ok(None);
    }
    let entity_set = value(config, "/mapping/myProjectsEntitySet")?;
    let resource_field = value(config, "/mapping/myProjectsResourceValueField")?;
    let project_field = value(config, "/mapping/myProjectsProjectValueField")?;
    let mut filter = format!("{resource_field} eq {resource_id}");
    if only_active {
        let state_field = config
            .pointer("/mapping/myProjectsStateField")
            .and_then(Value::as_str)
            .unwrap_or("");
        let state_value = config
            .pointer("/mapping/myProjectsStateValue")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !state_field.is_empty() && !state_value.is_empty() {
            filter.push_str(&format!(" and {state_field} eq {state_value}"));
        }
    }
    let response = client(paths, config)
        .await?
        .get(
            &format!("/{entity_set}?$filter={filter}&$select={project_field}"),
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(
        response["value"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| row[project_field].as_str().map(str::to_owned))
            .collect(),
    ))
}

/// Liefert die Projekt-IDs, in denen die konfigurierte resourceId mindestens
/// eine Projektaufgabe (Resource Assignment) hat. Ohne resourceId wird None
/// zurueckgegeben, damit der Aufrufer keine Projekte anbietet.
async fn my_task_project_ids(
    paths: &AppPaths,
    config: &Value,
) -> Result<Option<Vec<String>>, String> {
    let resource_id = config["resourceId"].as_str().unwrap_or("");
    if resource_id.is_empty() {
        return Ok(None);
    }
    let entity_set = value(config, "/mapping/myTasksEntitySet")?;
    let resource_field = value(config, "/mapping/myTasksResourceValueField")?;
    let project_field = value(config, "/mapping/myTasksProjectValueField")?;
    let response = client(paths, config)
        .await?
        .get(
            &format!(
                "/{entity_set}?$filter={resource_field} eq {resource_id}&$select={project_field}"
            ),
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut ids: Vec<String> = response["value"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row[project_field].as_str().map(str::to_owned))
        .collect();
    ids.sort();
    ids.dedup();
    Ok(Some(ids))
}

/// Lists the projects the current resource is a member of, without the
/// `$top` limit used by the interactive picker.
pub async fn my_projects(paths: &AppPaths, config: &Value) -> Result<Vec<LookupItem>, String> {
    let Some(ids) = my_project_ids(paths, config, false).await? else {
        return Err(
            "resourceId ist nicht gesetzt. Zuerst 'psa discover myresource' ausführen.".into(),
        );
    };
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let entity_set = value(config, "/mapping/projectEntitySet")?;
    let id_field = value(config, "/mapping/projectIdField")?;
    let name_field = value(config, "/mapping/projectNameField")?;
    let filter = format!(
        "({})",
        ids.iter()
            .map(|id| format!("{id_field} eq {id}"))
            .collect::<Vec<_>>()
            .join(" or ")
    );
    lookup(
        paths,
        config,
        &format!("/{entity_set}?$filter={filter}&$select={id_field},{name_field}&$orderby={name_field} asc"),
        id_field,
        name_field,
    )
    .await
}

/// Finds at most 50 projects. Restricted configurations never expose projects
/// whose team membership could not be established.
pub async fn projects(
    paths: &AppPaths,
    config: &Value,
    query: &str,
) -> Result<Vec<LookupItem>, String> {
    let entity_set = value(config, "/mapping/projectEntitySet")?;
    let id_field = value(config, "/mapping/projectIdField")?;
    let name_field = value(config, "/mapping/projectNameField")?;
    let mut filters = Vec::new();
    if !query.is_empty() {
        filters.push(format!("contains({name_field},'{}')", escape(query)));
    }
    // Nur aktive Projekte anbieten: geschlossene/beendete Projekte sind für
    // die Zeiterfassung nicht mehr relevant.
    if let (Some(state_field), Some(active_value)) = (
        config["mapping"]["projectStateField"]
            .as_str()
            .filter(|value| !value.is_empty()),
        config["mapping"]["projectActiveValue"]
            .as_str()
            .filter(|value| !value.is_empty()),
    ) {
        filters.push(format!("{state_field} eq {active_value}"));
    }
    let restrict_projects = config_bool(&config["mapping"]["restrictToMyProjects"]);
    let restrict_tasks = config_bool(&config["mapping"]["restrictToMyTasks"]);
    // Team-Mitgliedschaft und Aufgaben-Zuordnung sind unabhängig und werden
    // parallel abgefragt, um die Latenz des Projekt-Pickers zu reduzieren.
    let (team_ids, task_ids) = tokio::join!(
        async {
            if restrict_projects {
                my_project_ids(paths, config, true).await
            } else {
                Ok(None)
            }
        },
        async {
            if restrict_tasks {
                my_task_project_ids(paths, config).await
            } else {
                Ok(None)
            }
        },
    );
    let team_ids = team_ids?;
    let task_ids = task_ids?;

    if restrict_projects {
        let Some(ids) = team_ids.filter(|ids| !ids.is_empty()) else {
            return Ok(Vec::new());
        };
        filters.push(format!(
            "({})",
            ids.iter()
                .map(|id| format!("{id_field} eq {id}"))
                .collect::<Vec<_>>()
                .join(" or ")
        ));
    }
    // Nur Projekte anbieten, in denen die resourceId eine Projektaufgabe hat.
    if restrict_tasks {
        let Some(ids) = task_ids.filter(|ids| !ids.is_empty()) else {
            return Ok(Vec::new());
        };
        filters.push(format!(
            "({})",
            ids.iter()
                .map(|id| format!("{id_field} eq {id}"))
                .collect::<Vec<_>>()
                .join(" or ")
        ));
    }
    let filter = if filters.is_empty() {
        String::new()
    } else {
        format!("$filter={}&", filters.join(" and "))
    };
    lookup(
        paths,
        config,
        &format!("/{entity_set}?{filter}$select={id_field},{name_field}&$top=50&$orderby={name_field} asc"),
        id_field,
        name_field,
    )
    .await
}

pub async fn tasks(
    paths: &AppPaths,
    config: &Value,
    project_id: Option<&str>,
    query: &str,
) -> Result<Vec<LookupItem>, String> {
    let entity_set = value(config, "/mapping/taskEntitySet")?;
    let id_field = value(config, "/mapping/taskIdField")?;
    let name_field = value(config, "/mapping/taskNameField")?;
    let mut filters = Vec::new();
    if !query.is_empty() {
        filters.push(format!("contains({name_field},'{}')", escape(query)));
    }
    if let Some(project_id) = project_id.filter(|id| !id.is_empty()) {
        filters.push(format!(
            "{} eq {project_id}",
            value(config, "/mapping/taskProjectLookupField")?
        ));
    }
    let filter = if filters.is_empty() {
        String::new()
    } else {
        format!("$filter={}&", filters.join(" and "))
    };
    lookup(
        paths,
        config,
        &format!("/{entity_set}?{filter}$select={id_field},{name_field}&$top=50&$orderby={name_field} asc"),
        id_field,
        name_field,
    )
    .await
}

async fn lookup(
    paths: &AppPaths,
    config: &Value,
    path: &str,
    id_field: &str,
    name_field: &str,
) -> Result<Vec<LookupItem>, String> {
    let response = client(paths, config)
        .await?
        .get(path, false)
        .await
        .map_err(|error| error.to_string())?;
    Ok(response["value"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some(LookupItem {
                id: row[id_field].as_str()?.to_owned(),
                name: row[name_field].as_str()?.to_owned(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{LookupItem, escape};

    #[test]
    fn escapes_odata_strings() {
        assert_eq!(escape("O'Brien"), "O''Brien");
    }

    #[test]
    fn lookup_item_display_omits_id() {
        let item = LookupItem {
            id: "abc-123".into(),
            name: "Projekt A".into(),
        };
        assert_eq!(item.to_string(), "Projekt A");
    }
}
