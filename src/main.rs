mod api;
mod config;

use anyhow::Result;
use clap::{Parser, Subcommand};

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

async fn add_deploy_key(client: &api::Client, repo: String, key: String, label: String) -> Result<()> {
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
        Commands::Create { slug, public, description } => create_repo(client, slug, public, description).await,
        Commands::Webhook { repo, url, events, description, inactive } => {
            create_webhook(client, repo, url, events, description, inactive).await
        }
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
