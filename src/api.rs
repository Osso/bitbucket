use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;

const BASE_URL: &str = "https://api.bitbucket.org/2.0";

pub struct Client {
    client: reqwest::Client,
    workspace: String,
    base_url: String,
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
            base_url: BASE_URL.to_string(),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_base_url(
        workspace: &str,
        username: &str,
        api_token: &str,
        base_url: String,
    ) -> Result<Self> {
        let mut client = Self::new(workspace, username, api_token)?;
        client.base_url = base_url;
        Ok(client)
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct MockServer {
        base_url: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl MockServer {
        async fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let server_requests = Arc::clone(&requests);

            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        break;
                    };
                    let requests = Arc::clone(&server_requests);
                    tokio::spawn(async move {
                        let mut buffer = vec![0; 8192];
                        let bytes_read = stream.read(&mut buffer).await.unwrap();
                        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                        let request_line = request.lines().next().unwrap_or_default().to_string();
                        let body = response_body(&request_line);
                        requests.lock().unwrap().push(request.to_string());
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    });
                }
            });

            Self {
                base_url: format!("http://{}", display_addr(addr)),
                requests,
            }
        }

        fn client(&self) -> Client {
            Client::with_base_url("workspace", "user", "token", self.base_url.clone()).unwrap()
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    fn display_addr(addr: SocketAddr) -> String {
        format!("{}:{}", addr.ip(), addr.port())
    }

    fn response_body(request_line: &str) -> &'static str {
        let path = request_line.split_whitespace().nth(1).unwrap_or_default();
        response_for_path(path).unwrap_or(r#"{"ok":true}"#)
    }

    fn response_for_path(path: &str) -> Option<&'static str> {
        read_response_for_path(path).or_else(|| write_response_for_path(path))
    }

    fn read_response_for_path(path: &str) -> Option<&'static str> {
        repo_response_for_path(path).or_else(|| pipeline_response_for_path(path))
    }

    fn repo_response_for_path(path: &str) -> Option<&'static str> {
        find_response(
            path,
            &[
                ("/user", r#"{"display_name":"Test User"}"#),
                (
                    "/repositories/workspace?page=2",
                    r#"{"values":[{"slug":"repo"}]}"#,
                ),
                ("/repositories/workspace?page=1", r#"{"values":[]}"#),
                (
                    "/repositories/workspace/repo",
                    r#"{"slug":"repo","links":{"html":{"href":"https://bitbucket/repo"}}}"#,
                ),
                (
                    "/repositories/workspace/repo/pullrequests?state=MERGED",
                    r#"{"values":[{"id":7}]}"#,
                ),
                (
                    "/repositories/workspace/repo/pullrequests?state=OPEN",
                    r#"{"values":[]}"#,
                ),
                ("/repositories/workspace/repo/pullrequests/7", r#"{"id":7}"#),
            ],
        )
    }

    fn pipeline_response_for_path(path: &str) -> Option<&'static str> {
        find_response(
            path,
            &[
                (
                    "/repositories/workspace/repo/pipelines/?sort=-created_on",
                    r#"{"values":[{"uuid":"pipe"}]}"#,
                ),
                (
                    "/repositories/workspace/repo/pipelines/pipe",
                    r#"{"uuid":"pipe"}"#,
                ),
                (
                    "/repositories/workspace/repo/refs/branches",
                    r#"{"values":[{"name":"main"}]}"#,
                ),
            ],
        )
    }

    fn write_response_for_path(path: &str) -> Option<&'static str> {
        find_response(
            path,
            &[
                (
                    "/repositories/workspace/new-repo",
                    r#"{"links":{"html":{"href":"https://bitbucket/new-repo"}}}"#,
                ),
                ("/repositories/workspace/repo/hooks", r#"{"uuid":"hook"}"#),
                ("/repositories/workspace/repo/deploy-keys", r#"{"id":42}"#),
            ],
        )
    }

    fn find_response(
        path: &str,
        responses: &[(&'static str, &'static str)],
    ) -> Option<&'static str> {
        responses
            .iter()
            .find_map(|(candidate, response)| (*candidate == path).then_some(*response))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn client_get_methods_call_expected_paths() {
        let server = MockServer::start().await;
        let client = server.client();

        assert_user_repo_and_pr_methods(&client).await;
        assert_pipeline_and_branch_methods(&client).await;
        assert_get_request_metadata(&server.requests());
    }

    async fn assert_user_repo_and_pr_methods(client: &Client) {
        assert_eq!(
            client.get_user().await.unwrap()["display_name"],
            "Test User"
        );
        assert_eq!(
            client.list_repositories(Some(2)).await.unwrap()["values"][0]["slug"],
            "repo"
        );
        assert_eq!(
            client.list_repositories(None).await.unwrap()["values"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(client.get_repository("repo").await.unwrap()["slug"], "repo");
        assert_eq!(
            client
                .list_pull_requests("repo", Some("MERGED"))
                .await
                .unwrap()["values"][0]["id"],
            7
        );
        assert_eq!(
            client.list_pull_requests("repo", None).await.unwrap()["values"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(client.get_pull_request("repo", 7).await.unwrap()["id"], 7);
    }

    async fn assert_pipeline_and_branch_methods(client: &Client) {
        assert_eq!(
            client.list_pipelines("repo").await.unwrap()["values"][0]["uuid"],
            "pipe"
        );
        assert_eq!(
            client.get_pipeline("repo", "pipe").await.unwrap()["uuid"],
            "pipe"
        );
        assert_eq!(
            client.list_branches("repo").await.unwrap()["values"][0]["name"],
            "main"
        );
    }

    fn assert_get_request_metadata(requests: &[String]) {
        assert!(
            requests
                .iter()
                .any(|request| request.starts_with("GET /user "))
        );
        assert!(
            requests
                .iter()
                .any(|request| request.starts_with("GET /repositories/workspace?page=2 "))
        );
        assert!(
            requests
                .iter()
                .any(|request| request.contains("authorization: Basic dXNlcjp0b2tlbg=="))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn client_post_methods_send_expected_json_bodies() {
        let server = MockServer::start().await;
        let client = server.client();

        assert_post_method_results(&client).await;
        assert_post_request_bodies(&server.requests());
    }

    async fn assert_post_method_results(client: &Client) {
        let repo = client
            .create_repository("new-repo", false, Some("description"))
            .await
            .unwrap();
        let webhook = client
            .create_webhook(
                "repo",
                "https://example.com/hook",
                &["repo:push", "pullrequest:created"],
                None,
                true,
            )
            .await
            .unwrap();
        let key = client
            .add_deploy_key("repo", "ssh-rsa AAA", "deploy")
            .await
            .unwrap();

        assert_eq!(repo["links"]["html"]["href"], "https://bitbucket/new-repo");
        assert_eq!(webhook["uuid"], "hook");
        assert_eq!(key["id"], 42);
    }

    fn assert_post_request_bodies(requests: &[String]) {
        assert!(requests.iter().any(|request| {
            request.starts_with("POST /repositories/workspace/new-repo ")
                && request.contains(r#""is_private":false"#)
                && request.contains(r#""description":"description""#)
        }));
        assert!(requests.iter().any(|request| {
            request.starts_with("POST /repositories/workspace/repo/hooks ")
                && request.contains(r#""url":"https://example.com/hook""#)
                && request.contains(r#""active":true"#)
        }));
        assert!(requests.iter().any(|request| {
            request.starts_with("POST /repositories/workspace/repo/deploy-keys ")
                && request.contains(r#""label":"deploy""#)
        }));
    }
}
