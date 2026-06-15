use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub download_dir: PathBuf,
    #[serde(default = "default_use_color")]
    pub use_color: bool,
    pub editor: Option<String>,
    /// Max search results shown per source (AUR / official). 0 = unlimited.
    #[serde(default = "default_search_limit")]
    pub search_limit: usize,
    /// Run reflector to update mirrorlist before system/AUR updates.
    #[serde(default = "default_update_mirrors")]
    pub update_mirrors: bool,
    /// Short flag for search (default: -Q). Long form --search always works.
    #[serde(default = "default_cmd_search")]
    pub cmd_search: String,
    /// Short flag for install/update-aur (default: -S). Long forms always work.
    #[serde(default = "default_cmd_install")]
    pub cmd_install: String,
    /// Short flag for full system update (default: -Syu). Long form --update-all always works.
    #[serde(default = "default_cmd_update_all")]
    pub cmd_update_all: String,
    /// Short flag for remove (default: -R). Long form --remove always works.
    #[serde(default = "default_cmd_remove")]
    pub cmd_remove: String,
    /// Short flag for list AUR packages (default: -L). Long form --list always works.
    #[serde(default = "default_cmd_list")]
    pub cmd_list: String,
    /// Short flag for updating mirrors with reflector (default: -M). Long form --update-mirrors always works.
    #[serde(default = "default_cmd_update_mirrors")]
    pub cmd_update_mirrors: String,
}

fn default_use_color() -> bool { true }
fn default_search_limit() -> usize { 15 }
fn default_update_mirrors() -> bool { true }
fn default_cmd_search() -> String { "-Q".to_string() }
fn default_cmd_install() -> String { "-S".to_string() }
fn default_cmd_update_all() -> String { "-Syu".to_string() }
fn default_cmd_remove() -> String { "-R".to_string() }
fn default_cmd_list() -> String { "-L".to_string() }
fn default_cmd_update_mirrors() -> String { "-M".to_string() }

impl Config {
    pub fn default() -> Self {
        let home = dirs::home_dir().expect("Failed to get home directory");
        Config {
            download_dir: home.join("Downloads").join("aur"),
            use_color: true,
            editor: None,
            search_limit: default_search_limit(),
            update_mirrors: default_update_mirrors(),
            cmd_search: default_cmd_search(),
            cmd_install: default_cmd_install(),
            cmd_update_all: default_cmd_update_all(),
            cmd_remove: default_cmd_remove(),
            cmd_list: default_cmd_list(),
            cmd_update_mirrors: default_cmd_update_mirrors(),
        }
    }

    pub fn config_path() -> PathBuf {
        let home = dirs::home_dir().expect("Failed to get home directory");
        home.join(".config").join("rauri").join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
            let mut config: Config = toml::from_str(&content)
                .with_context(|| "Failed to parse config file")?;

            if let Some(path_str) = config.download_dir.to_str() {
                if path_str.starts_with('~') {
                    let home = dirs::home_dir().expect("Failed to get home directory");
                    config.download_dir = home.join(path_str[1..].trim_start_matches('/'));
                }
            }

            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }

    pub fn ensure_download_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.download_dir)
            .with_context(|| format!("Failed to create download directory: {}", self.download_dir.display()))?;
        Ok(())
    }

    pub fn prompt_download_dir() -> Result<PathBuf> {
        let default = Self::default().download_dir;
        println!("Enter download directory path (default: {}): ", default.display());

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)
            .context("Failed to read input")?;

        let response = input.trim();
        if response.is_empty() {
            return Ok(default);
        }

        let mut path = PathBuf::from(response);

        if response.starts_with('~') {
            let home = dirs::home_dir().expect("Failed to get home directory");
            path = home.join(response[1..].trim_start_matches('/'));
        }

        Ok(path)
    }

    // --- Command matching helpers ---
    // Each returns true if `cmd` matches either the configured short flag or the fixed long form.

    pub fn is_search_cmd(&self, cmd: &str) -> bool {
        cmd == self.cmd_search || cmd == "--search"
    }

    /// Short flag with "A" appended, or --search-all — shows all results ignoring search_limit.
    pub fn is_search_all_cmd(&self, cmd: &str) -> bool {
        let all = format!("{}A", self.cmd_search);
        cmd == all || cmd == "--search-all"
    }

    pub fn is_install_cmd(&self, cmd: &str) -> bool {
        cmd == self.cmd_install || cmd == "--install"
    }

    pub fn is_update_all_cmd(&self, cmd: &str) -> bool {
        cmd == self.cmd_update_all || cmd == "--update-all"
    }

    pub fn is_remove_cmd(&self, cmd: &str) -> bool {
        cmd == self.cmd_remove || cmd == "--remove"
    }

    pub fn is_list_cmd(&self, cmd: &str) -> bool {
        cmd == self.cmd_list || cmd == "--list"
    }

    /// Short flag with "A" appended, or --list-all — lists all system packages.
    pub fn is_list_all_cmd(&self, cmd: &str) -> bool {
        let all = format!("{}A", self.cmd_list);
        cmd == all || cmd == "--list-all"
    }

    pub fn is_update_mirrors_cmd(&self, cmd: &str) -> bool {
        cmd == self.cmd_update_mirrors || cmd == "--update-mirrors"
    }
}
