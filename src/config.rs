use crate::paths::AppPaths;
use serde_json::{Map, Value, json};
use std::fs;
use std::io;

pub const SAMPLE_CLIENT_ID: &str = "51f81489-12ee-4a9e-aaae-a2591f45987d";

pub fn default_config() -> Value {
    json!({
        "tenantId": "",
        "clientId": SAMPLE_CLIENT_ID,
        "environmentUrl": "",
        "resourceId": "",
        "mapping": {
            "timeEntryEntitySet": "msdyn_timeentries",
            "timeEntryIdField": "msdyn_timeentryid",
            "dateField": "msdyn_date",
            "dateOnly": false,
            "timezone": "UTC",
            "durationField": "msdyn_duration",
            "durationUnit": "minutes",
            "descriptionField": "msdyn_externaldescription",
            "entryStatusField": "msdyn_entrystatus",
            "projectLookupField": "msdyn_project",
            "projectLookupBind": "msdyn_project@odata.bind",
            "projectLookupValueField": "_msdyn_project_value",
            "taskLookupField": "msdyn_projecttask",
            "taskLookupBind": "msdyn_projectTask@odata.bind",
            "taskLookupValueField": "_msdyn_projecttask_value",
            "resourceLookupField": "msdyn_bookableresource",
            "resourceLookupBind": "msdyn_bookableresource@odata.bind",
            "resourceLookupValueField": "_msdyn_bookableresource_value",
            "projectEntitySet": "msdyn_projects",
            "projectIdField": "msdyn_projectid",
            "projectNameField": "msdyn_subject",
            "projectStateField": "statecode",
            "projectActiveValue": "0",
            "taskEntitySet": "msdyn_projecttasks",
            "taskIdField": "msdyn_projecttaskid",
            "taskNameField": "msdyn_subject",
            "taskProjectLookupField": "_msdyn_project_value",
            "restrictToMyProjects": true,
            "myProjectsEntitySet": "msdyn_projectteams",
            "myProjectsResourceValueField": "_msdyn_bookableresourceid_value",
            "myProjectsProjectValueField": "_msdyn_project_value",
            "myProjectsStateField": "statecode",
            "myProjectsStateValue": "0",
            "restrictToMyTasks": true,
            "myTasksEntitySet": "msdyn_resourceassignments",
            "myTasksResourceValueField": "_msdyn_bookableresourceid_value",
            "myTasksProjectValueField": "_msdyn_projectid_value"
        }
    })
}

/// Applies the same recursive defaulting behaviour as Python's `_deep_merge`.
pub fn deep_merge(base: &Value, override_value: &Value) -> Value {
    match (base, override_value) {
        (Value::Object(base), Value::Object(override_value)) => {
            let mut merged = base.clone();
            for (key, value) in override_value {
                let result = merged
                    .get(key)
                    .map(|existing| deep_merge(existing, value))
                    .unwrap_or_else(|| value.clone());
                merged.insert(key.clone(), result);
            }
            Value::Object(merged)
        }
        (_, value) => value.clone(),
    }
}

pub fn load(paths: &AppPaths) -> Result<Value, ConfigError> {
    paths.ensure_app_dir()?;
    if !paths.config_file.exists() {
        return Ok(default_config());
    }
    let raw = fs::read_to_string(&paths.config_file)?;
    let user_config = serde_json::from_str(&raw)?;
    Ok(migrate(deep_merge(&default_config(), &user_config)))
}

/// Corrects known-invalid mapping values that older releases persisted.
///
/// `psa config set` writes the fully merged configuration back to disk, so
/// wrong defaults could end up stored as explicit overrides. These stored
/// values take precedence over the corrected defaults and must be fixed here.
fn migrate(config: Value) -> Value {
    let mut config = config;
    let Some(mapping) = config.get_mut("mapping").and_then(Value::as_object_mut) else {
        return config;
    };

    // `msdyn_resourceassignment` uses `msdyn_bookableresourceid` and
    // `msdyn_projectid` as lookups, unlike `msdyn_projectteam` which uses
    // `msdyn_bookableresourceid` and `msdyn_project`.
    if mapping
        .get("myTasksResourceValueField")
        .and_then(Value::as_str)
        == Some("_msdyn_bookableresource_value")
    {
        mapping.insert(
            "myTasksResourceValueField".into(),
            Value::String("_msdyn_bookableresourceid_value".into()),
        );
    }
    if mapping
        .get("myTasksProjectValueField")
        .and_then(Value::as_str)
        == Some("_msdyn_project_value")
    {
        mapping.insert(
            "myTasksProjectValueField".into(),
            Value::String("_msdyn_projectid_value".into()),
        );
    }

    config
}

pub fn save(paths: &AppPaths, config: &Value) -> Result<(), ConfigError> {
    paths.ensure_app_dir()?;
    let content = serde_json::to_string_pretty(config)?;
    fs::write(&paths.config_file, content)?;
    restrict_to_owner(&paths.config_file)?;
    Ok(())
}

/// Mirrors `psa config set`: command-line values are deliberately stored as strings.
pub fn set_value(config: &mut Value, dotted_key: &str, value: String) -> Result<(), ConfigError> {
    if dotted_key.is_empty() {
        return Err(ConfigError::InvalidKey);
    }

    let parts: Vec<_> = dotted_key.split('.').collect();
    let mut current = config.as_object_mut().ok_or(ConfigError::InvalidConfig)?;
    for part in &parts[..parts.len() - 1] {
        if !current.get(*part).is_some_and(Value::is_object) {
            current.insert((*part).to_owned(), Value::Object(Map::new()));
        }
        current = current
            .get_mut(*part)
            .and_then(Value::as_object_mut)
            .ok_or(ConfigError::InvalidConfig)?;
    }
    current.insert(parts[parts.len() - 1].to_owned(), Value::String(value));
    Ok(())
}

#[cfg(unix)]
fn restrict_to_owner(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &std::path::Path) -> io::Result<()> {
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read or write configuration: {0}")]
    Io(#[from] io::Error),
    #[error("configuration is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("configuration root must be an object")]
    InvalidConfig,
    #[error("configuration key must not be empty")]
    InvalidKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_retains_default_mapping_values() {
        let merged = deep_merge(
            &default_config(),
            &json!({"mapping": {"timezone": "Europe/Zurich"}}),
        );
        assert_eq!(merged["mapping"]["timezone"], "Europe/Zurich");
        assert_eq!(merged["mapping"]["durationUnit"], "minutes");
    }

    #[test]
    fn config_set_keeps_cli_value_as_string() {
        let mut config = default_config();
        set_value(&mut config, "mapping.dateOnly", "true".to_owned()).unwrap();
        assert_eq!(config["mapping"]["dateOnly"], "true");
    }

    #[test]
    fn migrate_fixes_persisted_my_tasks_lookup_fields() {
        let mut config = default_config();
        config["mapping"]["myTasksResourceValueField"] = json!("_msdyn_bookableresource_value");
        config["mapping"]["myTasksProjectValueField"] = json!("_msdyn_project_value");
        let migrated = migrate(config);
        assert_eq!(
            migrated["mapping"]["myTasksResourceValueField"],
            "_msdyn_bookableresourceid_value"
        );
        assert_eq!(
            migrated["mapping"]["myTasksProjectValueField"],
            "_msdyn_projectid_value"
        );
    }

    #[test]
    fn migrate_leaves_other_project_value_fields_untouched() {
        let config = default_config();
        let migrated = migrate(config);
        assert_eq!(
            migrated["mapping"]["myProjectsProjectValueField"],
            "_msdyn_project_value"
        );
        assert_eq!(
            migrated["mapping"]["taskProjectLookupField"],
            "_msdyn_project_value"
        );
    }
}
