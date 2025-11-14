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
}

fn default_use_color() -> bool {
    true
}

impl Config {
    pub fn default() -> Self {
        let home = dirs::home_dir().expect("Failed to get home directory");
        let download_dir = home.join("Downloads").join("aur");
        Config {
            download_dir,
            use_color: true,
            editor: None,
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
            
            // Expand ~ in path if present
            if let Some(path_str) = config.download_dir.to_str() {
                if path_str.starts_with('~') {
                    let home = dirs::home_dir().expect("Failed to get home directory");
                    let expanded = home.join(path_str[1..].trim_start_matches('/'));
                    config.download_dir = expanded;
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
        
        // Expand ~ if present
        if response.starts_with('~') {
            let home = dirs::home_dir().expect("Failed to get home directory");
            path = home.join(response[1..].trim_start_matches('/'));
        }
        
        Ok(path)
    }
}

