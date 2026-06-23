mod api;
mod config;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[cfg(test)]
static TEST_ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

#[derive(Parser)]
#[command(name = "bitbucket")]
#[command(about = "CLI tool for Bitbucket Cloud API")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure workspace and credentials
    Config {
        /// Bitbucket workspace slug
        #[arg(short, long)]
        workspace: Option<String>,
        /// Bitbucket username
        #[arg(short, long)]
        username: Option<String>,
        /// API token (from bitbucket.org/account/settings/api-tokens/)
        #[arg(short, long)]
        token: Option<String>,
    },
    /// Get current user info
    User,
    /// List repositories in the workspace
    Repos {
        /// Page number
        #[arg(short, long)]
        page: Option<u32>,
    },
    /// Get repository details
    Repo {
        /// Repository slug
        slug: String,
    },
    /// List pull requests
    Prs {
        /// Repository slug
        repo: String,
        /// PR state: OPEN, MERGED, DECLINED, SUPERSEDED
        #[arg(short, long)]
        state: Option<String>,
    },
    /// Get pull request details
    Pr {
        /// Repository slug
        repo: String,
        /// Pull request ID
        id: u32,
    },
    /// List pipelines
    Pipelines {
        /// Repository slug
        repo: String,
    },
    /// Get pipeline details
    Pipeline {
        /// Repository slug
        repo: String,
        /// Pipeline UUID
        uuid: String,
    },
    /// List branches
    Branches {
        /// Repository slug
        repo: String,
    },
    /// Create a new repository
    Create {
        /// Repository slug (name)
        slug: String,
        /// Make repository public
        #[arg(long)]
        public: bool,
        /// Repository description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// List webhooks
    Webhooks {
        /// Repository slug
        repo: String,
    },
    /// Create a webhook
    Webhook {
        /// Repository slug
        repo: String,
        /// Webhook URL
        url: String,
        /// Events to trigger on (comma-separated: repo:push,pullrequest:created,etc)
        #[arg(short, long, default_value = "repo:push")]
        events: String,
        /// Webhook description
        #[arg(short, long)]
        description: Option<String>,
        /// Create as inactive
        #[arg(long)]
        inactive: bool,
    },
    /// List deploy keys
    DeployKeys {
        /// Repository slug
        repo: String,
    },
    /// Add a deploy key
    DeployKey {
        /// Repository slug
        repo: String,
        /// SSH public key
        key: String,
        /// Label for the key
        #[arg(short, long)]
        label: String,
    },
}

fn get_client() -> Result<api::Client> {
    let cfg = config::load_config()?;

    let workspace = cfg.workspace.ok_or_else(|| {
        anyhow::anyhow!("Workspace not configured. Run 'bitbucket config -w <workspace>' first")
    })?;
    let username = cfg.username.ok_or_else(|| {
        anyhow::anyhow!("Username not configured. Run 'bitbucket config -u <username>' first")
    })?;
    let api_token = cfg.api_token.ok_or_else(|| {
        anyhow::anyhow!("API token not configured. Run 'bitbucket config -t <token>' first")
    })?;

    api::Client::new(&workspace, &username, &api_token)
}

fn run_config(
    workspace: Option<String>,
    username: Option<String>,
    token: Option<String>,
) -> Result<()> {
    let mut cfg = config::load_config().unwrap_or_default();

    if workspace.is_none() && username.is_none() && token.is_none() {
        println!("Current config:");
        println!(
            "  Workspace: {}",
            cfg.workspace.as_deref().unwrap_or("(not set)")
        );
        println!(
            "  Username:  {}",
            cfg.username.as_deref().unwrap_or("(not set)")
        );
        println!(
            "  Token:     {}",
            if cfg.api_token.is_some() {
                "(set)"
            } else {
                "(not set)"
            }
        );
        return Ok(());
    }

    cfg.workspace = workspace.or(cfg.workspace);
    cfg.username = username.or(cfg.username);
    cfg.api_token = token.or(cfg.api_token);
    config::save_config(&cfg)?;
    Ok(())
}

async fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

async fn get_user(client: &api::Client) -> Result<()> {
    print_json(client.get_user().await?).await
}

async fn list_repos(client: &api::Client, page: Option<u32>) -> Result<()> {
    print_json(client.list_repositories(page).await?).await
}

async fn get_repo(client: &api::Client, slug: String) -> Result<()> {
    print_json(client.get_repository(&slug).await?).await
}

async fn list_prs(client: &api::Client, repo: String, state: Option<String>) -> Result<()> {
    print_json(client.list_pull_requests(&repo, state.as_deref()).await?).await
}

async fn get_pr(client: &api::Client, repo: String, id: u32) -> Result<()> {
    print_json(client.get_pull_request(&repo, id).await?).await
}

async fn list_pipelines(client: &api::Client, repo: String) -> Result<()> {
    print_json(client.list_pipelines(&repo).await?).await
}

async fn get_pipeline(client: &api::Client, repo: String, uuid: String) -> Result<()> {
    print_json(client.get_pipeline(&repo, &uuid).await?).await
}

async fn list_branches(client: &api::Client, repo: String) -> Result<()> {
    print_json(client.list_branches(&repo).await?).await
}

async fn list_webhooks(client: &api::Client, repo: String) -> Result<()> {
    print_json(client.list_webhooks(&repo).await?).await
}

async fn list_deploy_keys(client: &api::Client, repo: String) -> Result<()> {
    print_json(client.list_deploy_keys(&repo).await?).await
}

async fn create_repo(
    client: &api::Client,
    slug: String,
    public: bool,
    description: Option<String>,
) -> Result<()> {
    let repo = client
        .create_repository(&slug, !public, description.as_deref())
        .await?;
    println!(
        "Created: {}",
        repo["links"]["html"]["href"].as_str().unwrap_or("")
    );
    Ok(())
}

async fn create_webhook(
    client: &api::Client,
    repo: String,
    url: String,
    events: String,
    description: Option<String>,
    inactive: bool,
) -> Result<()> {
    let events: Vec<&str> = events.split(',').collect();
    let webhook = client
        .create_webhook(&repo, &url, &events, description.as_deref(), !inactive)
        .await?;
    println!(
        "Created webhook: {}",
        webhook["uuid"].as_str().unwrap_or("")
    );
    Ok(())
}

async fn add_deploy_key(
    client: &api::Client,
    repo: String,
    key: String,
    label: String,
) -> Result<()> {
    let result = client.add_deploy_key(&repo, &key, &label).await?;
    println!("Added deploy key: {}", result["id"].as_u64().unwrap_or(0));
    Ok(())
}

async fn run_command(client: &api::Client, command: Commands) -> Result<()> {
    match command {
        Commands::User => get_user(client).await,
        Commands::Repos { page } => list_repos(client, page).await,
        Commands::Repo { slug } => get_repo(client, slug).await,
        Commands::Prs { repo, state } => list_prs(client, repo, state).await,
        Commands::Pr { repo, id } => get_pr(client, repo, id).await,
        Commands::Pipelines { repo } => list_pipelines(client, repo).await,
        Commands::Pipeline { repo, uuid } => get_pipeline(client, repo, uuid).await,
        Commands::Branches { repo } => list_branches(client, repo).await,
        Commands::Webhooks { repo } => list_webhooks(client, repo).await,
        Commands::DeployKeys { repo } => list_deploy_keys(client, repo).await,
        Commands::Create {
            slug,
            public,
            description,
        } => create_repo(client, slug, public, description).await,
        Commands::Webhook {
            repo,
            url,
            events,
            description,
            inactive,
        } => create_webhook(client, repo, url, events, description, inactive).await,
        Commands::DeployKey { repo, key, label } => add_deploy_key(client, repo, key, label).await,
        Commands::Config { .. } => unreachable!(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Commands::Config {
        workspace,
        username,
        token,
    } = cli.command
    {
        return run_config(workspace, username, token);
    }

    let client = get_client()?;
    run_command(&client, cli.command).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
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
                        let request_line = request.lines().next().unwrap_or_default();
                        let body = response_body(request_line);
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

        fn client(&self) -> api::Client {
            api::Client::with_base_url("workspace", "user", "token", self.base_url.clone()).unwrap()
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
                    "/repositories/workspace?page=3",
                    r#"{"values":[{"slug":"repo"}]}"#,
                ),
                (
                    "/repositories/workspace/repo",
                    r#"{"slug":"repo","links":{"html":{"href":"https://bitbucket/repo"}}}"#,
                ),
                (
                    "/repositories/workspace/repo/pullrequests?state=OPEN",
                    r#"{"values":[{"id":1}]}"#,
                ),
                ("/repositories/workspace/repo/pullrequests/1", r#"{"id":1}"#),
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
                ("/repositories/workspace/repo/hooks", r#"{"uuid":"hook"}"#),
                ("/repositories/workspace/repo/deploy-keys", r#"{"id":42}"#),
            ],
        )
    }

    fn write_response_for_path(path: &str) -> Option<&'static str> {
        find_response(
            path,
            &[(
                "/repositories/workspace/new-repo",
                r#"{"links":{"html":{"href":"https://bitbucket/new-repo"}}}"#,
            )],
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

    fn with_temp_config(test: impl FnOnce()) {
        let guard = crate::TEST_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap();
        let dir = tempdir().unwrap();

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", dir.path());
        }

        test();

        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        drop(guard);
    }

    #[test]
    fn parses_all_command_shapes() {
        assert!(matches!(
            Cli::try_parse_from(["bitbucket", "user"]).unwrap().command,
            Commands::User
        ));
        assert!(matches!(
            Cli::try_parse_from(["bitbucket", "repos", "--page", "3"])
                .unwrap()
                .command,
            Commands::Repos { page: Some(3) }
        ));
        assert!(matches!(
            Cli::try_parse_from(["bitbucket", "repo", "repo"])
                .unwrap()
                .command,
            Commands::Repo { slug } if slug == "repo"
        ));
        assert!(matches!(
            Cli::try_parse_from(["bitbucket", "prs", "repo", "--state", "MERGED"])
                .unwrap()
                .command,
            Commands::Prs { repo, state: Some(state) } if repo == "repo" && state == "MERGED"
        ));
        assert!(matches!(
            Cli::try_parse_from(["bitbucket", "deploy-key", "repo", "ssh-rsa AAA", "--label", "ci"])
                .unwrap()
                .command,
            Commands::DeployKey { repo, key, label } if repo == "repo" && key == "ssh-rsa AAA" && label == "ci"
        ));
    }

    #[test]
    fn config_command_merges_new_values_with_existing_config() {
        with_temp_config(|| {
            run_config(
                Some("workspace".to_string()),
                Some("user".to_string()),
                Some("token".to_string()),
            )
            .unwrap();
            run_config(Some("new-workspace".to_string()), None, None).unwrap();
            run_config(None, None, None).unwrap();

            let config = config::load_config().unwrap();
            assert_eq!(config.workspace.as_deref(), Some("new-workspace"));
            assert_eq!(config.username.as_deref(), Some("user"));
            assert_eq!(config.api_token.as_deref(), Some("token"));
        });
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_command_dispatches_read_operations() {
        let server = MockServer::start().await;
        let client = server.client();

        run_basic_read_commands(&client).await;
        run_pull_request_commands(&client).await;
        run_pipeline_and_branch_commands(&client).await;
        run_repo_extra_read_commands(&client).await;
        assert_read_requests(&server.requests());
    }

    async fn run_basic_read_commands(client: &api::Client) {
        run_command(client, Commands::User).await.unwrap();
        run_command(client, Commands::Repos { page: Some(3) })
            .await
            .unwrap();
        run_command(
            client,
            Commands::Repo {
                slug: "repo".to_string(),
            },
        )
        .await
        .unwrap();
    }

    async fn run_repo_extra_read_commands(client: &api::Client) {
        run_command(
            client,
            Commands::Webhooks {
                repo: "repo".to_string(),
            },
        )
        .await
        .unwrap();
        run_command(
            client,
            Commands::DeployKeys {
                repo: "repo".to_string(),
            },
        )
        .await
        .unwrap();
    }

    fn assert_read_requests(requests: &[String]) {
        assert!(
            requests
                .iter()
                .any(|request| request.starts_with("GET /user "))
        );
        assert!(
            requests
                .iter()
                .any(|request| request.contains("/pullrequests/1"))
        );
        assert!(
            requests
                .iter()
                .any(|request| request.contains("/refs/branches"))
        );
    }

    async fn run_pull_request_commands(client: &api::Client) {
        run_command(
            client,
            Commands::Prs {
                repo: "repo".to_string(),
                state: None,
            },
        )
        .await
        .unwrap();
        run_command(
            client,
            Commands::Pr {
                repo: "repo".to_string(),
                id: 1,
            },
        )
        .await
        .unwrap();
    }

    async fn run_pipeline_and_branch_commands(client: &api::Client) {
        run_command(
            client,
            Commands::Pipelines {
                repo: "repo".to_string(),
            },
        )
        .await
        .unwrap();
        run_command(
            client,
            Commands::Pipeline {
                repo: "repo".to_string(),
                uuid: "pipe".to_string(),
            },
        )
        .await
        .unwrap();
        run_command(
            client,
            Commands::Branches {
                repo: "repo".to_string(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_command_dispatches_write_operations() {
        let server = MockServer::start().await;
        let client = server.client();

        run_create_command(&client).await;
        run_webhook_command(&client).await;
        run_deploy_key_command(&client).await;
        assert_write_requests(&server.requests());
    }

    async fn run_create_command(client: &api::Client) {
        run_command(
            client,
            Commands::Create {
                slug: "new-repo".to_string(),
                public: true,
                description: Some("description".to_string()),
            },
        )
        .await
        .unwrap();
    }

    async fn run_webhook_command(client: &api::Client) {
        run_command(
            client,
            Commands::Webhook {
                repo: "repo".to_string(),
                url: "https://example.com/hook".to_string(),
                events: "repo:push,pullrequest:created".to_string(),
                description: Some("hook".to_string()),
                inactive: false,
            },
        )
        .await
        .unwrap();
    }

    async fn run_deploy_key_command(client: &api::Client) {
        run_command(
            client,
            Commands::DeployKey {
                repo: "repo".to_string(),
                key: "ssh-rsa AAA".to_string(),
                label: "ci".to_string(),
            },
        )
        .await
        .unwrap();
    }

    fn assert_write_requests(requests: &[String]) {
        assert!(requests.iter().any(|request| {
            request.starts_with("POST /repositories/workspace/new-repo ")
                && request.contains(r#""is_private":false"#)
        }));
        assert!(requests.iter().any(|request| {
            request.starts_with("POST /repositories/workspace/repo/hooks ")
                && request.contains(r#""events":["repo:push","pullrequest:created"]"#)
        }));
        assert!(requests.iter().any(|request| {
            request.starts_with("POST /repositories/workspace/repo/deploy-keys ")
                && request.contains(r#""key":"ssh-rsa AAA""#)
        }));
    }
}
