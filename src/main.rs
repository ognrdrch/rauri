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

    // Extract global flags before any other processing
    let clear_aur_path = args.contains(&"-C".to_string());
    let skip_aur = args.contains(&"--skip-aur".to_string());
    let mut args: Vec<String> = args.into_iter()
        .filter(|a| a != "-C" && a != "--skip-aur")
        .collect();

    // Check for -P flag (set AUR path)
    let mut aur_path: Option<PathBuf> = None;
    if let Some(p_index) = args.iter().position(|a| a == "-P") {
        if p_index + 1 >= args.len() {
            Ui::error("Please provide a path after -P");
            std::process::exit(1);
        }

        let potential_path = &args[p_index + 1];

        if potential_path.starts_with('-') {
            Ui::error(&format!(
                "Invalid path: '{}'. Did you mean to use a command? Use -P with a valid directory path.",
                potential_path
            ));
            std::process::exit(1);
        }

        aur_path = Some(PathBuf::from(potential_path));
        args.remove(p_index);
        args.remove(p_index);
    }

    let mut config = Config::load()
        .context("Failed to load config")?;

    // Handle -P flag: set AUR path and exit
    if let Some(path) = aur_path {
        let mut expanded_path = path;

        if let Some(path_str) = expanded_path.to_str() {
            if path_str.starts_with('~') {
                let home = dirs::home_dir().expect("Failed to get home directory");
                expanded_path = home.join(path_str[1..].trim_start_matches('/'));
            }
        }

        expanded_path = expanded_path.canonicalize()
            .or_else(|_| {
                if let Some(parent) = expanded_path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                Ok::<PathBuf, std::io::Error>(expanded_path)
            })?;

        if expanded_path.exists() && expanded_path.is_file() {
            Ui::error(&format!(
                "Path '{}' exists but is a file, not a directory",
                expanded_path.display()
            ));
            std::process::exit(1);
        }

        std::fs::create_dir_all(&expanded_path)
            .context("Failed to create directory")?;

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

    config.ensure_download_dir()
        .context("Failed to create download directory")?;

    if clear_aur_path {
        PackageManager::clear_aur_path()?;
    }

    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        Ui::print_help(&config);
        return Ok(());
    }

    // AUR URL shortcut
    if !args.is_empty() && Aur::is_aur_url(&args[0]) {
        handle_aur_url(&args[0], &config)?;
        return Ok(());
    }

    let command = args[0].clone();
    let has_pkg = args.len() >= 2;

    if config.is_search_all_cmd(&command) {
        if !has_pkg {
            Ui::error("Please provide a package name to search");
            std::process::exit(1);
        }
        PackageManager::search(&args[1], None)?;
    } else if config.is_search_cmd(&command) {
        if !has_pkg {
            Ui::error("Please provide a package name to search");
            std::process::exit(1);
        }
        // 0 in config means unlimited
        let limit = if config.search_limit == 0 { None } else { Some(config.search_limit) };
        PackageManager::search(&args[1], limit)?;
    } else if config.is_install_cmd(&command) && has_pkg {
        PackageManager::install(&args[1], &config)?;
    } else if config.is_install_cmd(&command) || command == "--update-aur" {
        // -S with no package arg, or explicit --update-aur
        PackageManager::update_aur_only(&config)?;
    } else if config.is_update_all_cmd(&command) {
        PackageManager::update_system(&config, skip_aur)?;
    } else if config.is_remove_cmd(&command) {
        if !has_pkg {
            Ui::error("Please provide a package name to remove");
            std::process::exit(1);
        }
        PackageManager::remove(&args[1], Some(&config))?;
    } else if config.is_list_all_cmd(&command) {
        PackageManager::list_all()?;
    } else if config.is_list_cmd(&command) {
        PackageManager::list_installed()?;
    } else {
        Ui::error(&format!("Unknown command: {}", command));
        Ui::print_help(&config);
        std::process::exit(1);
    }

    Ok(())
}

fn handle_aur_url(url: &str, config: &Config) -> Result<()> {
    let package_name = Aur::extract_package_name(url)?;
    let package_dir = Aur::clone_repo(url, &config.download_dir)?;
    let actual_package_name = Aur::build_and_install(&package_dir, &package_name)?;

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
