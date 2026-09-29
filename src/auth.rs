use crate::paths::AppPaths;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io};

/// Cache format version. Bumped when the stored shape changes incompatibly.
const CACHE_VERSION: u8 = 1;

/// A cached access token is reused only while it is still valid for at least
/// this many seconds. The buffer absorbs clock skew and network latency.
const EXPIRY_BUFFER_SECS: i64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(
        "environmentUrl ist nicht gesetzt; zuerst 'psa config set environmentUrl <URL>' ausführen"
    )]
    MissingEnvironment,
    #[error("Anmeldung fehlgeschlagen: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Token-Cache konnte nicht verarbeitet werden: {0}")]
    Io(#[from] io::Error),
    #[error("OAuth-Fehler: {0}")]
    OAuth(String),
    #[error("Anmeldung konnte nicht erneuert werden – bitte erneut 'psa login' ausführen ({0})")]
    RefreshFailed(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct Cache {
    version: u8,
    refresh_token: String,
    /// Cached access token reused until shortly before `expires_at`. Both are
    /// optional so that token caches written by older versions still parse.
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    expires_at: Option<i64>,
}

fn authority(config: &Value) -> String {
    let t = config["tenantId"].as_str().unwrap_or("");
    format!(
        "https://login.microsoftonline.com/{}",
        if t.is_empty() { "organizations" } else { t }
    )
}

fn client(config: &Value) -> &str {
    config["clientId"].as_str().unwrap_or("")
}

fn scope(config: &Value) -> Result<String, AuthError> {
    let e = config["environmentUrl"]
        .as_str()
        .unwrap_or("")
        .trim_end_matches('/');
    if e.is_empty() {
        Err(AuthError::MissingEnvironment)
    } else {
        Ok(format!("{e}/.default offline_access"))
    }
}

async fn token_request(
    http: &reqwest::Client,
    url: String,
    form: &[(&str, &str)],
) -> Result<Value, AuthError> {
    // OAuth returns expected polling states such as authorization_pending as
    // HTTP 400 JSON responses, so decode the body before considering status.
    let response = http.post(url).form(form).send().await?;
    let status = response.status();
    let body = response.text().await?;
    let value: Value = serde_json::from_str(&body)
        .map_err(|_| AuthError::OAuth(format!("HTTP {status}: {body}")))?;
    if let Some(error) = value.get("error") {
        return Err(AuthError::OAuth(format!(
            "{}: {}",
            error,
            value.get("error_description").unwrap_or(error)
        )));
    }
    if !status.is_success() {
        return Err(AuthError::OAuth(format!("HTTP {status}: {body}")));
    }
    Ok(value)
}

fn read_cache(paths: &AppPaths) -> Option<Cache> {
    let raw = fs::read(&paths.token_cache_file).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Returns the cached access token when it is still valid beyond the buffer.
fn valid_cached_token(cache: &Cache, now: i64) -> Option<&str> {
    cache
        .expires_at
        .filter(|expires_at| *expires_at > now + EXPIRY_BUFFER_SECS)
        .and(cache.access_token.as_deref())
}

/// Builds the cache from a token response. `previous_refresh` is kept when the
/// identity provider does not rotate the refresh token on this response.
fn build_cache(response: &Value, previous_refresh: &str, now: i64) -> Cache {
    let refresh_token = response["refresh_token"]
        .as_str()
        .unwrap_or(previous_refresh)
        .to_owned();
    let access_token = response["access_token"].as_str().map(str::to_owned);
    let expires_at = response["expires_in"]
        .as_i64()
        .filter(|seconds| *seconds > 0)
        .map(|seconds| now + seconds);
    Cache {
        version: CACHE_VERSION,
        refresh_token,
        access_token,
        expires_at,
    }
}

fn save(paths: &AppPaths, cache: &Cache) -> Result<(), io::Error> {
    paths.ensure_app_dir()?;
    // Write to a temporary file first so a crash never leaves a truncated or
    // partially written token cache behind.
    let tmp = paths.token_cache_file.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(cache).expect("serializable"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    if paths.token_cache_file.exists() {
        fs::remove_file(&paths.token_cache_file)?;
    }
    fs::rename(&tmp, &paths.token_cache_file)?;
    Ok(())
}

pub async fn access_token(
    paths: &AppPaths,
    config: &Value,
    force: bool,
) -> Result<String, AuthError> {
    let http = reqwest::Client::new();
    let a = authority(config);
    let scope = scope(config)?;
    let now = Utc::now().timestamp();

    if !force && let Some(cache) = read_cache(paths) {
        // Reuse a still-valid access token without a network round trip.
        if let Some(token) = valid_cached_token(&cache, now) {
            return Ok(token.to_owned());
        }

        // Rotate the access token by exchanging the refresh token.
        let response = match token_request(
            &http,
            format!("{a}/oauth2/v2.0/token"),
            &[
                ("client_id", client(config)),
                ("grant_type", "refresh_token"),
                ("refresh_token", &cache.refresh_token),
                ("scope", &scope),
            ],
        )
        .await
        {
            Ok(response) => response,
            Err(AuthError::OAuth(error)) => return Err(AuthError::RefreshFailed(error)),
            Err(error) => return Err(error),
        };
        let rotated = build_cache(&response, &cache.refresh_token, now);
        let access_token = rotated
            .access_token
            .clone()
            .ok_or_else(|| AuthError::OAuth("kein access_token erhalten".into()))?;
        save(paths, &rotated)?;
        return Ok(access_token);
    }

    let device = token_request(
        &http,
        format!("{a}/oauth2/v2.0/devicecode"),
        &[("client_id", client(config)), ("scope", &scope)],
    )
    .await?;
    println!(
        "{}",
        device["message"]
            .as_str()
            .unwrap_or("Bitte Device-Code-Login abschließen.")
    );
    let code = device["device_code"]
        .as_str()
        .ok_or_else(|| AuthError::OAuth("kein device_code erhalten".into()))?
        .to_owned();
    let interval = device["interval"].as_u64().unwrap_or(5);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        let response = token_request(
            &http,
            format!("{a}/oauth2/v2.0/token"),
            &[
                ("client_id", client(config)),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &code),
            ],
        )
        .await;
        match response {
            Ok(response) => {
                let cache = build_cache(&response, "", Utc::now().timestamp());
                if cache.refresh_token.is_empty() {
                    return Err(AuthError::OAuth("kein refresh_token erhalten".into()));
                }
                let access_token = cache
                    .access_token
                    .clone()
                    .ok_or_else(|| AuthError::OAuth("kein access_token erhalten".into()))?;
                save(paths, &cache)?;
                return Ok(access_token);
            }
            Err(AuthError::OAuth(error))
                if error.contains("authorization_pending") || error.contains("slow_down") =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn logout(paths: &AppPaths) -> io::Result<()> {
    match fs::remove_file(&paths.token_cache_file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_cache_without_access_token_deserializes() {
        let cache: Cache = serde_json::from_str(r#"{"version":1,"refresh_token":"rt"}"#)
            .expect("legacy cache parses");
        assert_eq!(cache.refresh_token, "rt");
        assert!(cache.access_token.is_none());
        assert!(cache.expires_at.is_none());
    }

    #[test]
    fn valid_token_is_reused_before_buffer() {
        let now = 1_700_000_000;
        let cache = Cache {
            version: CACHE_VERSION,
            refresh_token: "rt".into(),
            access_token: Some("at".into()),
            expires_at: Some(now + EXPIRY_BUFFER_SECS + 1),
        };
        assert_eq!(valid_cached_token(&cache, now), Some("at"));
    }

    #[test]
    fn token_within_buffer_is_not_reused() {
        let now = 1_700_000_000;
        let cache = Cache {
            version: CACHE_VERSION,
            refresh_token: "rt".into(),
            access_token: Some("at".into()),
            expires_at: Some(now + EXPIRY_BUFFER_SECS - 1),
        };
        assert_eq!(valid_cached_token(&cache, now), None);
    }

    #[test]
    fn missing_expiry_is_not_reused() {
        let cache = Cache {
            version: CACHE_VERSION,
            refresh_token: "rt".into(),
            access_token: Some("at".into()),
            expires_at: None,
        };
        assert_eq!(valid_cached_token(&cache, 1_700_000_000), None);
    }

    #[test]
    fn build_cache_uses_rotated_refresh_token_and_expiry() {
        let now = 1_700_000_000;
        let response = json!({
            "access_token": "at",
            "refresh_token": "rt-new",
            "expires_in": 3600,
        });
        let cache = build_cache(&response, "rt-old", now);
        assert_eq!(cache.refresh_token, "rt-new");
        assert_eq!(cache.access_token.as_deref(), Some("at"));
        assert_eq!(cache.expires_at, Some(now + 3600));
    }

    #[test]
    fn build_cache_keeps_previous_refresh_token_when_not_rotated() {
        let now = 1_700_000_000;
        let response = json!({"access_token": "at", "expires_in": 3600});
        let cache = build_cache(&response, "rt-old", now);
        assert_eq!(cache.refresh_token, "rt-old");
    }
}
