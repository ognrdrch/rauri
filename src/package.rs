use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::fs;

use crate::aur::Aur;
use crate::config::Config;
use crate::tracker::PackageTracker;
use crate::ui::{Ui, Colors};
use atty::Stream;
use colored::Colorize;

pub struct PackageManager;

/// Parse pacman -Ss output into (header, description) pairs.
fn parse_pacman_output(stdout: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = stdout.lines().collect();
    let mut packages = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if !line.starts_with(' ') {
            let header = line.to_string();
            let desc = if i + 1 < lines.len() && lines[i + 1].starts_with(' ') {
                i += 1;
                lines[i].to_string()
            } else {
                String::new()
            };
            packages.push((header, desc));
        }

        i += 1;
    }

    packages
}

/// Extract bare package name from a pacman header line like "extra/code 1.99.0-1 [installed]".
fn pkg_name_from_header(header: &str) -> &str {
    let first = header.split_whitespace().next().unwrap_or("");
    if let Some(pos) = first.rfind('/') { &first[pos + 1..] } else { first }
}

/// Lower = higher priority. Sorts name matches above description matches.
fn official_match_priority(header: &str, query: &str) -> u8 {
    let q = query.to_lowercase();
    let name = pkg_name_from_header(header).to_lowercase();
    if name == q { 0 }
    else if name.starts_with(&q) { 1 }
    else if name.contains(&q) { 2 }
    else { 3 } // description match (caller already filtered by pacman)
}

fn aur_match_priority(pkg: &crate::aur::AurPackage, query: &str) -> u8 {
    let q = query.to_lowercase();
    let name = pkg.name.to_lowercase();
    if name == q { 0 }
    else if name.starts_with(&q) { 1 }
    else if name.contains(&q) { 2 }
    else { 3 }
}

impl PackageManager {
    /// Search AUR and official repos. Pass limit=None to show all results.
    pub fn search(query: &str, limit: Option<usize>) -> Result<()> {
        let mut aur_packages = match Aur::search(query) {
            Ok(packages) => packages,
            Err(e) => {
                Ui::warning(&format!("Failed to search AUR: {}", e));
                Vec::new()
            }
        };

        // Sort AUR results: exact name match → starts with → contains → description match
        aur_packages.sort_by_key(|pkg| aur_match_priority(pkg, query));

        let is_tty = atty::is(Stream::Stdout);

        if !aur_packages.is_empty() {
            let total = aur_packages.len();
            let shown = limit.map_or(total, |n| n.min(total));

            if is_tty {
                println!("\n{}", "AUR Packages:".cyan().bold());
            } else {
                println!("\nAUR Packages:");
            }

            for pkg in aur_packages.iter().take(shown) {
                let desc = pkg.description.as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default();
                if is_tty {
                    println!("  {}{}{} {}({}){}{}",
                        Colors::BOLD, pkg.name.yellow(), Colors::RESET,
                        Colors::DIM, pkg.version, Colors::RESET, desc);
                } else {
                    println!("  {} ({}){}", pkg.name, pkg.version, desc);
                }
            }

            if let Some(n) = limit {
                if total > n {
                    if is_tty {
                        println!("  {}... and {} more AUR results. Use --search-all to see all{}",
                            Colors::DIM, total - n, Colors::RESET);
                    } else {
                        println!("  ... and {} more AUR results. Use --search-all to see all", total - n);
                    }
                }
            }
        }

        // Search official repos
        let official_result = Command::new("pacman")
            .arg("-Ss")
            .arg(query)
            .output();

        let has_official_results = match &official_result {
            Ok(output) if output.status.success() && !output.stdout.is_empty() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut packages = parse_pacman_output(&stdout);

                // Sort: name matches first, description matches last
                packages.sort_by_key(|(header, _)| official_match_priority(header, query));

                let total = packages.len();
                let shown = limit.map_or(total, |n| n.min(total));

                if shown > 0 {
                    if is_tty {
                        println!("\n{}", "Official Repository Packages:".cyan().bold());
                    } else {
                        println!("\nOfficial Repository Packages:");
                    }

                    for (header, desc) in packages.iter().take(shown) {
                        if is_tty {
                            println!("{}", header.bold());
                        } else {
                            println!("{}", header);
                        }
                        if !desc.is_empty() {
                            println!("{}", desc);
                        }
                    }

                    if let Some(n) = limit {
                        if total > n {
                            if is_tty {
                                println!("  {}... and {} more official results. Use --search-all to see all{}",
                                    Colors::DIM, total - n, Colors::RESET);
                            } else {
                                println!("  ... and {} more official results. Use --search-all to see all", total - n);
                            }
                        }
                    }

                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        if aur_packages.is_empty() && !has_official_results {
            Ui::warning("No packages found");
        }

        Ok(())
    }

    pub fn install(package_name: &str, config: &Config) -> Result<()> {
        let check_result = Command::new("pacman")
            .arg("-Si")
            .arg(package_name)
            .output();

        match check_result {
            Ok(output) if output.status.success() => {
                Ui::info(&format!("Installing {} from official repositories...", package_name));

                let install_result = Command::new("sudo")
                    .arg("pacman")
                    .arg("-S")
                    .arg("--noconfirm")
                    .arg(package_name)
                    .status()
                    .context("Failed to execute pacman install")?;

                if install_result.success() {
                    Ui::success(&format!("Installed {} successfully", package_name));
                } else {
                    anyhow::bail!("Installation failed");
                }
            }
            _ => {
                Ui::info(&format!("Installing {} from AUR...", package_name));

                let aur_url = format!("https://aur.archlinux.org/{}.git", package_name);
                let package_dir = Aur::clone_repo(&aur_url, &config.download_dir)?;
                let actual_package_name = Aur::build_and_install(&package_dir, package_name)?;

                if let Err(e) = PackageTracker::add(&actual_package_name) {
                    Ui::warning(&format!("Failed to track package: {}", e));
                }

                if actual_package_name != package_name {
                    Ui::success(&format!("Installed {} successfully", package_name));
                } else {
                    Ui::success(&format!("Installed {} successfully", actual_package_name));
                }
            }
        }

        Ok(())
    }

    pub fn cleanup_tracking() -> Result<()> {
        let tracked_packages = PackageTracker::load().unwrap_or_default();

        if tracked_packages.is_empty() {
            return Ok(());
        }

        let mut packages_to_remove = Vec::new();
        for package_name in &tracked_packages {
            let check_result = Command::new("pacman")
                .arg("-Q")
                .arg(package_name)
                .output();

            if let Ok(output) = check_result {
                if !output.status.success() {
                    packages_to_remove.push(package_name.clone());
                }
            }
        }

        for package_name in &packages_to_remove {
            if let Err(e) = PackageTracker::remove(package_name) {
                Ui::warning(&format!("Failed to remove {} from tracking: {}", package_name, e));
            }
        }

        if !packages_to_remove.is_empty() {
            Ui::info(&format!("Cleaned up {} uninstalled package(s) from tracking", packages_to_remove.len()));
        }

        Ok(())
    }

    /// Run reflector to update the mirrorlist. Skipped if config.update_mirrors is false
    /// or reflector is not installed.
    pub fn update_mirrors(config: &Config) -> Result<()> {
        if !config.update_mirrors {
            return Ok(());
        }

        let available = Command::new("which")
            .arg("reflector")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !available {
            Ui::warning("reflector not found — skipping mirror update. Install with: rauri -S reflector");
            return Ok(());
        }

        Ui::info("Updating mirrorlist with reflector...");
        let status = Command::new("sudo")
            .arg("reflector")
            .arg("--latest")
            .arg("20")
            .arg("--sort")
            .arg("rate")
            .arg("--save")
            .arg("/etc/pacman.d/mirrorlist")
            .status()
            .context("Failed to run reflector")?;

        if status.success() {
            Ui::success("Mirrorlist updated");
        } else {
            Ui::warning("Mirror update failed, continuing anyway");
        }

        Ok(())
    }

    pub fn update_aur_only(config: &Config) -> Result<()> {
        Self::update_mirrors(config)?;
        Self::cleanup_tracking()?;

        let tracked_packages = PackageTracker::load().unwrap_or_default();

        let mut base_packages = HashSet::new();
        for pkg in &tracked_packages {
            if pkg.ends_with("-debug") {
                let base_name = pkg.strip_suffix("-debug").unwrap_or(pkg);
                base_packages.insert(base_name.to_string());
            } else {
                base_packages.insert(pkg.clone());
            }
        }

        if base_packages.is_empty() {
            Ui::info("No AUR packages tracked by rauri to update.");
            return Ok(());
        }

        for package_name in &base_packages {
            let installed_result = Command::new("pacman")
                .arg("-Q")
                .arg(package_name)
                .output();

            match installed_result {
                Ok(output) if output.status.success() => {
                    let installed_info = String::from_utf8_lossy(&output.stdout);
                    let installed_version = installed_info.trim().split_whitespace().nth(1)
                        .unwrap_or("");

                    match Aur::get_package_info(package_name) {
                        Ok(aur_pkg) => {
                            if installed_version != aur_pkg.version {
                                Ui::info(&format!("Updating {} from {} to {}...",
                                    package_name, installed_version, aur_pkg.version));

                                let aur_url = format!("https://aur.archlinux.org/{}.git", package_name);
                                let package_dir = Aur::clone_repo(&aur_url, &config.download_dir)?;
                                let actual_package_name = Aur::build_and_install(&package_dir, package_name)?;

                                if let Err(e) = PackageTracker::add(&actual_package_name) {
                                    Ui::warning(&format!("Failed to track package: {}", e));
                                }
                            } else {
                                Ui::info(&format!("{} is up to date", package_name));
                            }
                        }
                        Err(e) => {
                            Ui::warning(&format!("Could not check AUR for {}, skipping: {}", package_name, e));
                        }
                    }
                }
                _ => {
                    Ui::warning(&format!("Package {} is not installed, skipping", package_name));
                }
            }
        }

        Ui::success("AUR package updates complete");
        Ok(())
    }

    /// Update the full system. Pass skip_aur=true to only update official packages.
    pub fn update_system(config: &Config, skip_aur: bool) -> Result<()> {
        Self::update_mirrors(config)?;

        Ui::info("Updating official packages...");

        let sync_result = Command::new("sudo")
            .arg("pacman")
            .arg("-Syy")
            .status()
            .context("Failed to sync package databases")?;

        if !sync_result.success() {
            anyhow::bail!("Failed to sync package databases");
        }

        let update_result = Command::new("sudo")
            .arg("pacman")
            .arg("-Syu")
            .arg("--noconfirm")
            .status()
            .context("Failed to update system packages")?;

        if !update_result.success() {
            anyhow::bail!("Failed to update system packages");
        }

        Ui::success("Official packages updated");

        if skip_aur {
            Ui::info("Skipping AUR package updates (--skip-aur)");
        } else {
            Self::update_aur_only(config)?;
        }

        Ok(())
    }

    pub fn remove(package_name: &str, config: Option<&Config>) -> Result<()> {
        let config_owned;
        let config = match config {
            Some(c) => c,
            None => {
                config_owned = Config::load()?;
                &config_owned
            }
        };

        if package_name.is_empty() {
            Ui::error("Please provide a package name to remove");
            return Ok(());
        }

        let check_exact = Command::new("pacman")
            .arg("-Q")
            .arg(package_name)
            .output();

        let mut actual_package_name = package_name.to_string();
        let mut repo_name = package_name.to_string();

        match check_exact {
            Ok(output) if output.status.success() => {
                actual_package_name = package_name.to_string();
            }
            _ => {
                if let Ok(tracked) = PackageTracker::load() {
                    for p in &tracked {
                        let check = Command::new("pacman")
                            .arg("-Q")
                            .arg(p)
                            .output();

                        if let Ok(check_output) = check {
                            if check_output.status.success() {
                                if p.contains(package_name) || package_name.contains(p.as_str()) {
                                    actual_package_name = p.clone();
                                    if config.download_dir.exists() {
                                        if let Ok(entries) = fs::read_dir(&config.download_dir) {
                                            for entry in entries.flatten() {
                                                let path = entry.path();
                                                if path.is_dir() {
                                                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                                        if name == p || name == package_name {
                                                            repo_name = name.to_string();
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let final_check = Command::new("pacman")
            .arg("-Q")
            .arg(&actual_package_name)
            .output();

        if let Ok(output) = final_check {
            if !output.status.success() {
                anyhow::bail!("Package '{}' is not installed", package_name);
            }
        } else {
            anyhow::bail!("Package '{}' is not installed", package_name);
        }

        let remove_result = Command::new("sudo")
            .arg("pacman")
            .arg("-R")
            .arg("--noconfirm")
            .arg(&actual_package_name)
            .status()
            .context("Failed to execute pacman remove")?;

        if !remove_result.success() {
            anyhow::bail!("Package removal failed");
        }

        let debug_package_name = format!("{}-debug", actual_package_name);
        let debug_check = Command::new("pacman")
            .arg("-Q")
            .arg(&debug_package_name)
            .output();

        if let Ok(output) = debug_check {
            if output.status.success() {
                if let Err(e) = Command::new("sudo")
                    .arg("pacman")
                    .arg("-R")
                    .arg("--noconfirm")
                    .arg(&debug_package_name)
                    .status()
                {
                    Ui::warning(&format!("Failed to remove debug package {}: {}", debug_package_name, e));
                }
            }
        }

        let mut packages_to_untrack = HashSet::new();
        packages_to_untrack.insert(actual_package_name.clone());
        packages_to_untrack.insert(package_name.to_string());

        if let Ok(tracked) = PackageTracker::load() {
            for tracked_pkg in &tracked {
                if tracked_pkg == &actual_package_name || tracked_pkg == package_name {
                    packages_to_untrack.insert(tracked_pkg.clone());
                }
            }
        }

        for pkg_to_remove in &packages_to_untrack {
            if let Err(e) = PackageTracker::remove(pkg_to_remove) {
                Ui::warning(&format!("Failed to untrack package: {}", e));
            }
        }

        if config.download_dir.exists() {
            let mut folder_removed = false;

            if let Ok(entries) = fs::read_dir(&config.download_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && !folder_removed {
                        if path.join("PKGBUILD").exists() {
                            if let Ok(pkg_files) = fs::read_dir(&path) {
                                for pkg_file in pkg_files.flatten() {
                                    let pkg_path = pkg_file.path();
                                    if pkg_path.extension().and_then(|s| s.to_str()) == Some("zst") {
                                        if let Some(file_name) = pkg_path.file_name().and_then(|n| n.to_str()) {
                                            if file_name.ends_with(".pkg.tar.zst") {
                                                let result = Command::new("pacman")
                                                    .arg("-Qp")
                                                    .arg(&pkg_path)
                                                    .output();

                                                if let Ok(output) = result {
                                                    if output.status.success() {
                                                        let stdout = String::from_utf8_lossy(&output.stdout);
                                                        if let Some(pkg_name_from_file) = stdout.trim().split_whitespace().next() {
                                                            if pkg_name_from_file == actual_package_name ||
                                                               pkg_name_from_file == package_name {
                                                                if let Err(e) = fs::remove_dir_all(&path) {
                                                                    Ui::warning(&format!("Failed to remove package folder {}: {}", path.display(), e));
                                                                } else {
                                                                    folder_removed = true;
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !folder_removed {
                let folder_names_to_try = vec![&repo_name, package_name, &actual_package_name];
                for folder_name in folder_names_to_try {
                    let folder_path = config.download_dir.join(folder_name);
                    if folder_path.exists() && folder_path.is_dir() {
                        if let Err(e) = fs::remove_dir_all(&folder_path) {
                            Ui::warning(&format!("Failed to remove package folder {}: {}", folder_path.display(), e));
                        }
                        break;
                    }
                }
            }
        }

        let success_msg = if actual_package_name != package_name {
            format!("Removed {} (was installed as {})", package_name, actual_package_name)
        } else {
            format!("Removed {}", package_name)
        };

        Ui::success(&format!("{} successfully", success_msg));
        Ok(())
    }

    pub fn clear_aur_path() -> Result<()> {
        let config = Config::load()?;
        let download_dir = &config.download_dir;

        if !download_dir.exists() {
            Ui::info("AUR download directory does not exist. Nothing to clear.");
            return Ok(());
        }

        let dirs_to_remove: Vec<PathBuf> = fs::read_dir(download_dir)?
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();

        if dirs_to_remove.is_empty() {
            Ui::info("AUR download directory is already empty.");
            return Ok(());
        }

        Ui::warning(&format!("This will remove {} package folder(s) from {}",
            dirs_to_remove.len(), download_dir.display()));
        Ui::info("Removing package folders...");

        let mut removed_count = 0;
        for folder in &dirs_to_remove {
            if let Err(e) = fs::remove_dir_all(folder) {
                Ui::warning(&format!("Failed to remove {}: {}", folder.display(), e));
            } else {
                removed_count += 1;
            }
        }

        Ui::success(&format!("Cleared AUR path: removed {} folder(s)", removed_count));
        Ok(())
    }

    /// List AUR packages tracked and installed via rauri.
    pub fn list_installed() -> Result<()> {
        let config = Config::load()?;
        let download_dir = &config.download_dir;

        if !download_dir.exists() {
            Ui::info("No AUR packages found in download directory.");
            return Ok(());
        }

        let mut packages: Vec<(String, String, String)> = Vec::new();

        if let Ok(entries) = fs::read_dir(download_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let repo_name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    if path.join("PKGBUILD").exists() {
                        let mut pkg_name = repo_name.clone();
                        let mut installed_packages: Vec<(String, String)> = Vec::new();

                        if let Ok(pkg_files) = fs::read_dir(&path) {
                            for pkg_file in pkg_files.flatten() {
                                let pkg_path = pkg_file.path();
                                if pkg_path.extension().and_then(|s| s.to_str()) == Some("zst") {
                                    if let Some(file_name) = pkg_path.file_name().and_then(|n| n.to_str()) {
                                        if file_name.ends_with(".pkg.tar.zst") && !file_name.ends_with("-debug.pkg.tar.zst") {
                                            let result = Command::new("pacman")
                                                .arg("-Qp")
                                                .arg(&pkg_path)
                                                .output();

                                            if let Ok(output) = result {
                                                if output.status.success() {
                                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                                    let parts: Vec<&str> = stdout.trim().split_whitespace().collect();
                                                    if parts.len() >= 2 {
                                                        let file_pkg_name = parts[0];

                                                        let check_result = Command::new("pacman")
                                                            .arg("-Q")
                                                            .arg(file_pkg_name)
                                                            .output();

                                                        if let Ok(check_output) = check_result {
                                                            if check_output.status.success() {
                                                                let installed_stdout = String::from_utf8_lossy(&check_output.stdout);
                                                                let installed_parts: Vec<&str> = installed_stdout.trim().split_whitespace().collect();
                                                                if installed_parts.len() >= 2 {
                                                                    let installed_version = installed_parts[1];
                                                                    installed_packages.push((file_pkg_name.to_string(), installed_version.to_string()));

                                                                    if !file_pkg_name.ends_with("-debug") && pkg_name == repo_name {
                                                                        pkg_name = file_pkg_name.to_string();
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(main_pkg) = installed_packages.iter()
                            .find(|p| !p.0.ends_with("-debug"))
                            .or_else(|| installed_packages.first())
                        {
                            packages.push((repo_name, main_pkg.0.clone(), main_pkg.1.clone()));
                        }
                    }
                }
            }
        }

        if packages.is_empty() {
            Ui::info("No AUR packages found in download directory.");
            return Ok(());
        }

        let mut outdated = HashSet::new();
        for (_, pkg_name, installed_version) in &packages {
            if installed_version.contains("(not installed)") {
                continue;
            }
            if let Ok(aur_pkg) = Aur::get_package_info(pkg_name) {
                let clean_version = installed_version.replace(" (not installed)", "");
                if clean_version != aur_pkg.version {
                    outdated.insert(pkg_name.clone());
                }
            }
        }

        let is_tty = atty::is(Stream::Stdout);
        for (repo_name, pkg_name, version) in &packages {
            let is_outdated = outdated.contains(pkg_name);
            let formatted = Ui::format_package(pkg_name, version, is_outdated);

            if repo_name != pkg_name {
                if is_tty {
                    println!("  {}{}{} → {}", Colors::BOLD, repo_name.yellow(), Colors::RESET, formatted);
                } else {
                    println!("  {} → {}", repo_name, formatted);
                }
            } else {
                println!("  {}", formatted);
            }
        }

        if !outdated.is_empty() {
            if is_tty {
                println!("\n{}{}{}", Colors::BOLD,
                    format!("{} packages have updates available", outdated.len()).yellow(),
                    Colors::RESET);
            } else {
                println!("\n{} packages have updates available", outdated.len());
            }
        }

        Ok(())
    }

    /// List all installed system packages (AUR + official) via pacman -Q.
    pub fn list_all() -> Result<()> {
        let is_tty = atty::is(Stream::Stdout);

        let output = Command::new("pacman")
            .arg("-Q")
            .output()
            .context("Failed to run pacman -Q")?;

        if !output.status.success() {
            anyhow::bail!("Failed to list installed packages");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: Vec<(&str, &str)> = stdout.lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, ' ');
                let name = parts.next()?;
                let version = parts.next()?;
                Some((name, version))
            })
            .collect();

        if packages.is_empty() {
            Ui::info("No packages installed.");
            return Ok(());
        }

        if is_tty {
            println!("\n{}", format!("All Installed Packages ({}):", packages.len()).cyan().bold());
        } else {
            println!("\nAll Installed Packages ({}):", packages.len());
        }

        for (name, version) in &packages {
            if is_tty {
                println!("  {} {}", name.bold(), version.green());
            } else {
                println!("  {} {}", name, version);
            }
        }

        Ok(())
    }
}
