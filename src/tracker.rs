use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct PackageData {
    packages: Vec<String>,
}

impl PackageTracker {
    pub fn tracking_file_path() -> PathBuf {
        let home = dirs::home_dir().expect("Failed to get home directory");
        home.join(".config").join("rauri").join("packages.toml")
    }

    pub fn load() -> Result<HashSet<String>> {
        let path = Self::tracking_file_path();
        
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read tracking file: {}", path.display()))?;
            
            let data: PackageData = toml::from_str(&content)
                .with_context(|| "Failed to parse tracking file")?;
            
            Ok(data.packages.into_iter().collect())
        } else {
            Ok(HashSet::new())
        }
    }

    pub fn save(packages: &HashSet<String>) -> Result<()> {
        let path = Self::tracking_file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }
        
        let mut packages_list: Vec<String> = packages.iter().cloned().collect();
        packages_list.sort();
        
        let data = PackageData {
            packages: packages_list,
        };
        
        let content = toml::to_string_pretty(&data)
            .context("Failed to serialize tracking data")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write tracking file: {}", path.display()))?;
        
        Ok(())
    }

    pub fn add(package_name: &str) -> Result<()> {
        let mut packages = Self::load().unwrap_or_default();
        packages.insert(package_name.to_string());
        Self::save(&packages)
    }

    pub fn remove(package_name: &str) -> Result<()> {
        let mut packages = Self::load().unwrap_or_default();
        packages.remove(package_name);
        Self::save(&packages)
    }

    #[allow(dead_code)]
    pub fn is_tracked(package_name: &str) -> bool {
        Self::load().unwrap_or_default().contains(package_name)
    }
}

pub struct PackageTracker;

