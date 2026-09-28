use crate::{auth, paths::AppPaths};
use serde_json::Value;
#[derive(Debug, thiserror::Error)]
pub enum DataverseError {
    #[error(transparent)]
    Auth(#[from] auth::AuthError),
    #[error("HTTP-Fehler: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Dataverse-Fehler ({context}): [{status}] {message}")]
    Response {
        context: String,
        status: u16,
        message: String,
    },
}
pub struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}
impl Client {
    pub async fn new(paths: &AppPaths, c: &Value, force: bool) -> Result<Self, DataverseError> {
        let env = c["environmentUrl"]
            .as_str()
            .unwrap_or("")
            .trim_end_matches('/');
        if env.is_empty() {
            return Err(DataverseError::Auth(auth::AuthError::MissingEnvironment));
        }
        Ok(Self {
            http: reqwest::Client::new(),
            base: format!("{env}/api/data/v9.2"),
            token: auth::access_token(paths, c, force).await?,
        })
    }
    fn request(&self, m: reqwest::Method, path: &str, annotate: bool) -> reqwest::RequestBuilder {
        let prefer = if annotate {
            "return=representation, odata.include-annotations=\"OData.Community.Display.V1.FormattedValue\""
        } else {
            "return=representation"
        };
        self.http
            .request(m, format!("{}{}", self.base, path))
            .bearer_auth(&self.token)
            .header("Content-Type", "application/json; charset=utf-8")
            .header("Accept", "application/json")
            .header("OData-MaxVersion", "4.0")
            .header("OData-Version", "4.0")
            .header("Prefer", prefer)
    }
    async fn result(r: reqwest::Response, ctx: String) -> Result<Value, DataverseError> {
        let status = r.status();
        let text = r.text().await?;
        if status.is_client_error() || status.is_server_error() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                .unwrap_or(text);
            return Err(DataverseError::Response {
                context: ctx,
                status: status.as_u16(),
                message,
            });
        }
        if text.is_empty() {
            Ok(Value::Object(Default::default()))
        } else {
            Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
        }
    }
    pub async fn get(&self, path: &str, annotate: bool) -> Result<Value, DataverseError> {
        Self::result(
            self.request(reqwest::Method::GET, path, annotate)
                .send()
                .await?,
            format!("GET {path}"),
        )
        .await
    }
    pub async fn post(&self, set: &str, body: &Value) -> Result<Value, DataverseError> {
        Self::result(
            self.request(reqwest::Method::POST, &format!("/{set}"), false)
                .json(body)
                .send()
                .await?,
            format!("POST {set}"),
        )
        .await
    }
    pub async fn patch(&self, set: &str, id: &str, body: &Value) -> Result<Value, DataverseError> {
        Self::result(
            self.request(reqwest::Method::PATCH, &format!("/{set}({id})"), false)
                .json(body)
                .send()
                .await?,
            format!("PATCH {set}({id})"),
        )
        .await
    }
    pub async fn delete(&self, set: &str, id: &str) -> Result<(), DataverseError> {
        let r = self
            .request(reqwest::Method::DELETE, &format!("/{set}({id})"), false)
            .send()
            .await?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        Self::result(r, format!("DELETE {set}({id})")).await?;
        Ok(())
    }
}
