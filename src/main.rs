use anyhow::{Context, Result};
use std::path::PathBuf;
use std::env;

mod config;
mod tracker;
mod aur;
mod ui;
mod package;

use config::Config;
use package::PackageManager;
use aur::Aur;
use ui::Ui;

fn main() {
    if let Err(e) = run() {
        Ui::error(&format!("Error: {}", e));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    
    // Check for -C flag (clear AUR path)
    let clear_aur_path = args.contains(&"-C".to_string());
    let mut args: Vec<String> = args.into_iter().filter(|a| a != "-C").collect();
    
    // Check for -P flag (set AUR path)
    let mut aur_path: Option<PathBuf> = None;
    if let Some(p_index) = args.iter().position(|a| a == "-P") {
        if p_index + 1 >= args.len() {
            Ui::error("Please provide a path after -P");
            std::process::exit(1);
        }
        
        let potential_path = &args[p_index + 1];
        
        // Check if the next argument looks like a command (starts with -)
        if potential_path.starts_with('-') {
            Ui::error(&format!("Invalid path: '{}'. Did you mean to use a command? Use -P with a valid directory path.", potential_path));
            std::process::exit(1);
        }
        
        aur_path = Some(PathBuf::from(potential_path));
        // Remove -P and its argument from args
        args.remove(p_index);
        args.remove(p_index);
    }
    
    // Initialize configuration
    let mut config = Config::load()
        .context("Failed to load config")?;
    
    // Handle -P flag: set AUR path
    if let Some(path) = aur_path {
        let mut expanded_path = path;
        
        // Expand ~ if present
        if let Some(path_str) = expanded_path.to_str() {
            if path_str.starts_with('~') {
                let home = dirs::home_dir().expect("Failed to get home directory");
                expanded_path = home.join(path_str[1..].trim_start_matches('/'));
            }
        }
        
        // Resolve to absolute path
        expanded_path = expanded_path.canonicalize()
            .or_else(|_| {
                // If path doesn't exist, create parent and return the path
                if let Some(parent) = expanded_path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                Ok::<PathBuf, std::io::Error>(expanded_path)
            })?;
        
        // Validate it's not a file (must be a directory or not exist yet)
        if expanded_path.exists() && expanded_path.is_file() {
            Ui::error(&format!("Path '{}' exists but is a file, not a directory", expanded_path.display()));
            std::process::exit(1);
        }
        
        // Create directory if it doesn't exist
        std::fs::create_dir_all(&expanded_path)
            .context("Failed to create directory")?;
        
        // Verify it's now a directory
        if !expanded_path.is_dir() {
            Ui::error(&format!("Failed to create directory: {}", expanded_path.display()));
            std::process::exit(1);
        }
        
        config.download_dir = expanded_path.clone();
        config.save()?;
        Ui::success(&format!("AUR download path set to: {}", expanded_path.display()));
        return Ok(());
    }
    
    // First-run setup
    if !Config::config_path().exists() {
        Ui::info("Welcome to rauri! First-time setup required.");
        let download_dir = Config::prompt_download_dir()?;
        config.download_dir = download_dir.clone();
        config.save()?;
        Ui::success(&format!("Configuration saved to {}", Config::config_path().display()));
    }
    
    // Ensure download directory exists
    config.ensure_download_dir()
        .context("Failed to create download directory")?;
    
    // Handle -C flag: clear AUR path before executing command
    if clear_aur_path {
        PackageManager::clear_aur_path()?;
    }
    
    // Handle help
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        Ui::print_help();
        return Ok(());
    }
    
    // Check if first argument is an AUR URL
    if !args.is_empty() && Aur::is_aur_url(&args[0]) {
        handle_aur_url(&args[0], &config)?;
        return Ok(());
    }
    
    // Parse command
    if args.is_empty() {
        Ui::error("No command provided");
        Ui::print_help();
        return Ok(());
    }
    
    let command = &args[0];
    match command.as_str() {
        "-Q" => {
            if args.len() < 2 {
                Ui::error("Please provide a package name to search");
                std::process::exit(1);
            }
            PackageManager::search(&args[1])?;
        }
        "-S" => {
            if args.len() < 2 {
                // No package name provided, update AUR packages only
                PackageManager::update_aur_only()?;
            } else {
                // Package name provided, install it
                PackageManager::install(&args[1], &config)?;
            }
        }
        "-Syu" => {
            PackageManager::update_system()?;
        }
        "-R" => {
            if args.len() < 2 {
                Ui::error("Please provide a package name to remove");
                std::process::exit(1);
            }
            PackageManager::remove(&args[1], Some(&config))?;
        }
        "-L" => {
            PackageManager::list_installed()?;
        }
        _ => {
            Ui::error(&format!("Unknown command: {}", command));
            Ui::print_help();
            std::process::exit(1);
        }
    }
    
    Ok(())
}

fn handle_aur_url(url: &str, config: &Config) -> Result<()> {
    let package_name = Aur::extract_package_name(url)?;
    let package_dir = Aur::clone_repo(url, &config.download_dir)?;
    let actual_package_name = Aur::build_and_install(&package_dir, &package_name)?;
    
    // Track the installed package
    if let Err(e) = tracker::PackageTracker::add(&actual_package_name) {
        Ui::warning(&format!("Failed to track package: {}", e));
    }
    
    if actual_package_name != package_name {
        Ui::success(&format!("Installed {} successfully", package_name));
    } else {
        Ui::success(&format!("Installed {} successfully", actual_package_name));
    }
    
    Ok(())
}

