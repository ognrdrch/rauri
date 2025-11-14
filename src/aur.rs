use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct AurPackage {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    #[allow(dead_code)]
    pub votes: i64,
    #[allow(dead_code)]
    pub popularity: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct AurSearchResponse {
    results: Vec<AurPackageJson>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AurPackageJson {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Description")]
    description: Option<String>,
    #[serde(rename = "NumVotes")]
    num_votes: Option<i64>,
    #[serde(rename = "Popularity")]
    popularity: Option<f64>,
}

// Reusable HTTP client to avoid creating a new one for each request
static HTTP_CLIENT: Lazy<reqwest::blocking::Client> = Lazy::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to create HTTP client")
});

// Pre-compiled regex for extracting package names from AUR URLs
static AUR_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"aur\.archlinux\.org/([^/]+)\.git")
        .expect("Failed to compile AUR URL regex")
});

pub struct Aur;

impl Aur {
    pub fn extract_package_name(url: &str) -> Result<String> {
        if let Some(caps) = AUR_URL_REGEX.captures(url) {
            Ok(caps.get(1).unwrap().as_str().to_string())
        } else {
            anyhow::bail!("Invalid AUR URL: {}", url)
        }
    }

    pub fn is_aur_url(url: &str) -> bool {
        url.contains("aur.archlinux.org") && url.ends_with(".git")
    }

    pub fn clone_repo(url: &str, download_dir: &Path) -> Result<PathBuf> {
        let package_name = Self::extract_package_name(url)?;
        let target_dir = download_dir.join(&package_name);
        
        // Remove existing directory if it exists
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)
                .with_context(|| format!("Failed to remove existing directory: {}", target_dir.display()))?;
        }
        
        let output = Command::new("git")
            .arg("clone")
            .arg(url)
            .arg(&target_dir)
            .output()
            .context("Failed to execute git clone")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git clone failed: {}", stderr);
        }
        
        Ok(target_dir)
    }

    pub fn build_and_install(package_dir: &Path, requested_package: &str) -> Result<String> {
        let output = Command::new("makepkg")
            .arg("-si")
            .current_dir(package_dir)
            .status()
            .context("Failed to execute makepkg")?;
        
        if !output.success() {
            anyhow::bail!("makepkg -si failed");
        }
        
        // Find what package was actually installed
        let mut actual_package_name = requested_package.to_string();
        
        // Look for .pkg.tar.zst files
        if let Ok(entries) = std::fs::read_dir(package_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("zst") {
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if file_name.ends_with(".pkg.tar.zst") {
                            // Get package name from the built package
                            let output = Command::new("pacman")
                                .arg("-Qp")
                                .arg(&path)
                                .output();
                            
                            if let Ok(output) = output {
                                if output.status.success() {
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    if let Some(name) = stdout.trim().split_whitespace().next() {
                                        actual_package_name = name.to_string();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(actual_package_name)
    }

    pub fn search(query: &str) -> Result<Vec<AurPackage>> {
        let url = format!("https://aur.archlinux.org/rpc/?v=5&type=search&arg={}", 
                         urlencoding::encode(query));
        
        let response = HTTP_CLIENT.get(&url)
            .send()
            .context("Failed to send search request")?;
        
        let json_data: AurSearchResponse = response.json()
            .context("Failed to parse search response")?;
        
        let packages: Vec<AurPackage> = json_data.results.into_iter().map(|pkg| {
            AurPackage {
                name: pkg.name,
                version: pkg.version,
                description: pkg.description,
                votes: pkg.num_votes.unwrap_or(0),
                popularity: pkg.popularity.unwrap_or(0.0),
            }
        }).collect();
        
        Ok(packages)
    }

    pub fn get_package_info(package_name: &str) -> Result<AurPackage> {
        let url = format!("https://aur.archlinux.org/rpc/?v=5&type=info&arg={}", 
                         urlencoding::encode(package_name));
        
        let response = HTTP_CLIENT.get(&url)
            .send()
            .context("Failed to send info request")?;
        
        let json_data: AurSearchResponse = response.json()
            .context("Failed to parse info response")?;
        
        if let Some(pkg) = json_data.results.first() {
            Ok(AurPackage {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                description: pkg.description.clone(),
                votes: pkg.num_votes.unwrap_or(0),
                popularity: pkg.popularity.unwrap_or(0.0),
            })
        } else {
            anyhow::bail!("Package not found: {}", package_name)
        }
    }
}

