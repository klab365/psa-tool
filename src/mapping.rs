//! Shared Dataverse field mapping used by sync, pull and the web layer.
//!
//! These helpers translate between the local [`TimeEntry`] model and the
//! Dataverse JSON representation driven by the configured `mapping` section.

use crate::{dataverse::Client, model::TimeEntry, paths::AppPaths};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};

/// Reads a dotted config value as a string, defaulting to an empty string.
pub fn value<'a>(config: &'a Value, path: &str) -> &'a str {
    config.pointer(path).and_then(Value::as_str).unwrap_or("")
}

/// Interprets a config value as a boolean, accepting JSON booleans as well as
/// the strings `"true"`/`"false"` written by `psa config set`.
pub fn config_bool(value: &Value) -> bool {
    value.as_bool().unwrap_or_else(|| {
        value
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    })
}

/// Converts a Dataverse date value to the local `work_date` string.
pub fn local_date(raw: &str, mapping: &Value) -> String {
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

/// Builds the Dataverse request body for a [`TimeEntry`] from the config.
pub fn entry_body(entry: &TimeEntry, config: &Value) -> Value {
    let m = &config["mapping"];
    let mut body = serde_json::Map::new();
    let date = if config_bool(&m["dateOnly"]) {
        entry.work_date.clone()
    } else {
        format!("{}T00:00:00Z", entry.work_date)
    };
    body.insert(value(m, "/dateField").to_owned(), json!(date));
    if value(m, "/durationUnit") == "hours" {
        body.insert(value(m, "/durationField").to_owned(), json!(entry.hours));
    } else {
        // `msdyn_duration` is an Edm.Int32. Keep the rounded minute value an
        // integer in JSON; serializing the intermediate f64 emits e.g. 360.0.
        let minutes = (entry.hours * 60.).round() as i32;
        body.insert(value(m, "/durationField").to_owned(), json!(minutes));
    }
    body.insert(
        value(m, "/descriptionField").to_owned(),
        json!(entry.description),
    );
    for (id, bind, set) in [
        (&entry.project_id, "/projectLookupBind", "/projectEntitySet"),
        (&entry.task_id, "/taskLookupBind", "/taskEntitySet"),
    ] {
        if let Some(id) = id {
            body.insert(
                value(m, bind).to_owned(),
                json!(format!("/{}({id})", value(m, set))),
            );
        }
    }
    if !value(config, "/resourceId").is_empty() {
        body.insert(
            value(m, "/resourceLookupBind").to_owned(),
            json!(format!(
                "/bookableresources({})",
                value(config, "/resourceId")
            )),
        );
    }
    Value::Object(body)
}

/// Creates a Dataverse client using the stored (non-interactive) token.
pub async fn client(paths: &AppPaths, config: &Value) -> Result<Client, String> {
    Client::new(paths, config, false)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

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
