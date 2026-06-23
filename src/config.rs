use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    /// Bitbucket workspace (e.g., "mycompany")
    pub workspace: Option<String>,
    /// Bitbucket username
    pub username: Option<String>,
    /// API token (from bitbucket.org/account/settings/api-tokens/)
    pub api_token: Option<String>,
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bitbucket-cli")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load_config() -> Result<Config> {
    let path = config_path();
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        return Ok(serde_json::from_str(&content)?);
    }
    Ok(Config::default())
}

pub fn save_config(config: &Config) -> Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let path = config_path();
    fs::write(&path, serde_json::to_string_pretty(config)?)?;
    println!("Config saved to {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn with_temp_config(test: impl FnOnce()) {
        let guard = crate::TEST_ENV_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
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
    fn load_missing_config_returns_default() {
        with_temp_config(|| {
            let config = load_config().unwrap();

            assert!(config.workspace.is_none());
            assert!(config.username.is_none());
            assert!(config.api_token.is_none());
        });
    }

    #[test]
    fn save_and_load_config_round_trips_all_fields() {
        with_temp_config(|| {
            let config = Config {
                workspace: Some("workspace".to_string()),
                username: Some("user".to_string()),
                api_token: Some("token".to_string()),
            };

            save_config(&config).unwrap();
            let loaded = load_config().unwrap();

            assert_eq!(loaded.workspace.as_deref(), Some("workspace"));
            assert_eq!(loaded.username.as_deref(), Some("user"));
            assert_eq!(loaded.api_token.as_deref(), Some("token"));
        });
    }
}
