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

async fn run_command(client: &api::Client, command: Commands) -> Result<()> {
    let json = match command {
        Commands::User => client.get_user().await?,
        Commands::Repos { page } => client.list_repositories(page).await?,
        Commands::Repo { slug } => client.get_repository(&slug).await?,
        Commands::Prs { repo, state } => client.list_pull_requests(&repo, state.as_deref()).await?,
        Commands::Pr { repo, id } => client.get_pull_request(&repo, id).await?,
        Commands::Pipelines { repo } => client.list_pipelines(&repo).await?,
        Commands::Pipeline { repo, uuid } => client.get_pipeline(&repo, &uuid).await?,
        Commands::Branches { repo } => client.list_branches(&repo).await?,
        Commands::Webhooks { repo } => client.list_webhooks(&repo).await?,
        Commands::DeployKeys { repo } => client.list_deploy_keys(&repo).await?,
        Commands::Create {
            slug,
            public,
            description,
        } => {
            let repo = client
                .create_repository(&slug, !public, description.as_deref())
                .await?;
            println!(
                "Created: {}",
                repo["links"]["html"]["href"].as_str().unwrap_or("")
            );
            return Ok(());
        }
        Commands::Webhook {
            repo,
            url,
            events,
            description,
            inactive,
        } => {
            let events: Vec<&str> = events.split(',').collect();
            let webhook = client
                .create_webhook(&repo, &url, &events, description.as_deref(), !inactive)
                .await?;
            println!(
                "Created webhook: {}",
                webhook["uuid"].as_str().unwrap_or("")
            );
            return Ok(());
        }
        Commands::DeployKey { repo, key, label } => {
            let result = client.add_deploy_key(&repo, &key, &label).await?;
            println!("Added deploy key: {}", result["id"].as_u64().unwrap_or(0));
            return Ok(());
        }
        Commands::Config { .. } => unreachable!(),
    };
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
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
