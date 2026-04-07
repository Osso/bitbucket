use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;

const BASE_URL: &str = "https://api.bitbucket.org/2.0";

pub struct Client {
    client: reqwest::Client,
    workspace: String,
}

impl Client {
    pub fn new(workspace: &str, username: &str, api_token: &str) -> Result<Self> {
        use base64::Engine;
        // Bitbucket API tokens use Basic auth with username:token
        let credentials = format!("{}:{}", username, api_token);
        let auth_b64 = base64::engine::general_purpose::STANDARD.encode(credentials);
        let auth_value = format!("Basic {}", auth_b64);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).context("Invalid auth header")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            workspace: workspace.to_string(),
        })
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let url = format!("{}{}", BASE_URL, path);
        let mut req = self.client.request(method, &url);
        if let Some(body) = body {
            req = req.json(body);
        }
        let response = req.send().await.context("Request failed")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        response.json().await.context("Failed to parse JSON")
    }

    async fn get(&self, path: &str) -> Result<Value> {
        self.request(reqwest::Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.request(reqwest::Method::POST, path, Some(body)).await
    }

    pub async fn get_user(&self) -> Result<Value> {
        self.get("/user").await
    }

    pub async fn list_repositories(&self, page: Option<u32>) -> Result<Value> {
        let page = page.unwrap_or(1);
        self.get(&format!("/repositories/{}?page={}", self.workspace, page))
            .await
    }

    pub async fn get_repository(&self, repo_slug: &str) -> Result<Value> {
        self.get(&format!("/repositories/{}/{}", self.workspace, repo_slug))
            .await
    }

    pub async fn list_pull_requests(&self, repo_slug: &str, state: Option<&str>) -> Result<Value> {
        let state = state.unwrap_or("OPEN");
        self.get(&format!(
            "/repositories/{}/{}/pullrequests?state={}",
            self.workspace, repo_slug, state
        ))
        .await
    }

    pub async fn get_pull_request(&self, repo_slug: &str, pr_id: u32) -> Result<Value> {
        self.get(&format!(
            "/repositories/{}/{}/pullrequests/{}",
            self.workspace, repo_slug, pr_id
        ))
        .await
    }

    pub async fn list_pipelines(&self, repo_slug: &str) -> Result<Value> {
        self.get(&format!(
            "/repositories/{}/{}/pipelines/?sort=-created_on",
            self.workspace, repo_slug
        ))
        .await
    }

    pub async fn get_pipeline(&self, repo_slug: &str, pipeline_uuid: &str) -> Result<Value> {
        self.get(&format!(
            "/repositories/{}/{}/pipelines/{}",
            self.workspace, repo_slug, pipeline_uuid
        ))
        .await
    }

    pub async fn list_branches(&self, repo_slug: &str) -> Result<Value> {
        self.get(&format!(
            "/repositories/{}/{}/refs/branches",
            self.workspace, repo_slug
        ))
        .await
    }

    pub async fn create_repository(
        &self,
        slug: &str,
        is_private: bool,
        description: Option<&str>,
    ) -> Result<Value> {
        let mut body = serde_json::json!({
            "scm": "git",
            "is_private": is_private,
        });
        if let Some(desc) = description {
            body["description"] = serde_json::Value::String(desc.to_string());
        }
        self.post(&format!("/repositories/{}/{}", self.workspace, slug), &body)
            .await
    }

    pub async fn list_webhooks(&self, repo_slug: &str) -> Result<Value> {
        self.get(&format!(
            "/repositories/{}/{}/hooks",
            self.workspace, repo_slug
        ))
        .await
    }

    pub async fn create_webhook(
        &self,
        repo_slug: &str,
        url: &str,
        events: &[&str],
        description: Option<&str>,
        active: bool,
    ) -> Result<Value> {
        let body = serde_json::json!({
            "url": url,
            "events": events,
            "description": description.unwrap_or(""),
            "active": active,
        });
        self.post(
            &format!("/repositories/{}/{}/hooks", self.workspace, repo_slug),
            &body,
        )
        .await
    }

    pub async fn list_deploy_keys(&self, repo_slug: &str) -> Result<Value> {
        self.get(&format!(
            "/repositories/{}/{}/deploy-keys",
            self.workspace, repo_slug
        ))
        .await
    }

    pub async fn add_deploy_key(&self, repo_slug: &str, key: &str, label: &str) -> Result<Value> {
        let body = serde_json::json!({
            "key": key,
            "label": label,
        });
        self.post(
            &format!("/repositories/{}/{}/deploy-keys", self.workspace, repo_slug),
            &body,
        )
        .await
    }
}
