use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io};

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
}
#[derive(Serialize, Deserialize)]
struct Cache {
    version: u8,
    refresh_token: String,
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
pub async fn access_token(
    paths: &AppPaths,
    config: &Value,
    force: bool,
) -> Result<String, AuthError> {
    let http = reqwest::Client::new();
    let a = authority(config);
    let scope = scope(config)?;
    if !force {
        if let Ok(raw) = fs::read(&paths.token_cache_file) {
            if let Ok(c) = serde_json::from_slice::<Cache>(&raw) {
                let v = token_request(
                    &http,
                    format!("{a}/oauth2/v2.0/token"),
                    &[
                        ("client_id", client(config)),
                        ("grant_type", "refresh_token"),
                        ("refresh_token", &c.refresh_token),
                        ("scope", &scope),
                    ],
                )
                .await?;
                save(
                    paths,
                    v.get("refresh_token")
                        .and_then(Value::as_str)
                        .unwrap_or(&c.refresh_token),
                )?;
                return v["access_token"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| AuthError::OAuth("kein access_token erhalten".into()));
            }
        }
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
        let v = token_request(
            &http,
            format!("{a}/oauth2/v2.0/token"),
            &[
                ("client_id", client(config)),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &code),
            ],
        )
        .await;
        match v {
            Ok(v) => {
                let refresh = v["refresh_token"]
                    .as_str()
                    .ok_or_else(|| AuthError::OAuth("kein refresh_token erhalten".into()))?;
                save(paths, refresh)?;
                return v["access_token"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| AuthError::OAuth("kein access_token erhalten".into()));
            }
            Err(AuthError::OAuth(e))
                if e.contains("authorization_pending") || e.contains("slow_down") =>
            {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}
fn save(paths: &AppPaths, refresh: &str) -> Result<(), io::Error> {
    paths.ensure_app_dir()?;
    fs::write(
        &paths.token_cache_file,
        serde_json::to_vec(&Cache {
            version: 1,
            refresh_token: refresh.to_owned(),
        })
        .expect("serializable"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&paths.token_cache_file, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
pub fn logout(paths: &AppPaths) -> io::Result<()> {
    match fs::remove_file(&paths.token_cache_file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
