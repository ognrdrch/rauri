use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::fs;

use crate::aur::Aur;
use crate::config::Config;
use crate::fix;
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

    pub fn install(package_name: &str, config: &Config, fix_sig: bool) -> Result<()> {
        let check_result = Command::new("pacman")
            .arg("-Si")
            .arg(package_name)
            .output();

        match check_result {
            Ok(output) if output.status.success() => {
                Ui::info(&format!("Installing {} from official repositories...", package_name));

                fix::run_pacman(&["-S", "--noconfirm", package_name], fix_sig)?;
                Ui::success(&format!("Installed {} successfully", package_name));
            }
            _ => {
                Ui::info(&format!("Installing {} from AUR...", package_name));

                let actual_package_name = Self::install_aur(package_name, config, fix_sig)?;

                if actual_package_name != package_name {
                    Ui::success(&format!("Installed {} successfully", package_name));
                } else {
                    Ui::success(&format!("Installed {} successfully", actual_package_name));
                }
            }
        }

        Ok(())
    }

    /// Clone, build and install an AUR package (by its AUR repo name) and track it.
    /// If makepkg reports dependencies missing from the official repos, offers to
    /// build them from the AUR first. Returns the name of the installed package.
    pub fn install_aur(repo_name: &str, config: &Config, fix_sig: bool) -> Result<String> {
        Self::build_aur(repo_name, config, fix_sig, false, &mut Vec::new())
    }

    /// `chain` holds the packages currently being built, to detect dependency cycles.
    fn build_aur(
        repo_name: &str,
        config: &Config,
        fix_sig: bool,
        as_dep: bool,
        chain: &mut Vec<String>,
    ) -> Result<String> {
        let aur_url = format!("https://aur.archlinux.org/{}.git", repo_name);
        let package_dir = Aur::clone_repo(&aur_url, &config.download_dir)?;

        chain.push(repo_name.to_string());
        let mut result = Aur::build_and_install(&package_dir, repo_name, fix_sig, as_dep);
        if let Err(e) = &result {
            if let Some(missing) = e.downcast_ref::<fix::MissingAurDeps>() {
                let deps = missing.0.clone();
                result = Self::install_aur_deps(&deps, config, fix_sig, chain)
                    .and_then(|_| {
                        Ui::info(&format!("Dependencies installed, building {} again...", repo_name));
                        Aur::build_and_install(&package_dir, repo_name, fix_sig, as_dep)
                    });
            }
        }
        chain.pop();

        let installed = result?;
        if let Err(e) = PackageTracker::add(&installed) {
            Ui::warning(&format!("Failed to track package: {}", e));
        }
        Ok(installed)
    }

    /// Look up missing dependencies in the AUR and, after confirmation, build them.
    fn install_aur_deps(deps: &[String], config: &Config, fix_sig: bool, chain: &mut Vec<String>) -> Result<()> {
        // pacman reports versioned targets like "foo>=1.2"
        let names: Vec<String> = deps.iter()
            .map(|d| d.split(|c| c == '<' || c == '>' || c == '=').next().unwrap_or(d).to_string())
            .collect();

        let mut found = Vec::new();
        let mut not_found = Vec::new();
        for name in &names {
            match Aur::get_package_info(name) {
                Ok(pkg) => found.push(pkg),
                Err(_) => not_found.push(name.clone()),
            }
        }

        if !not_found.is_empty() {
            anyhow::bail!(
                "Dependencies not found in the official repos or the AUR: {}",
                not_found.join(", ")
            );
        }

        if let Some(pkg) = found.iter().find(|p| chain.contains(&p.package_base)) {
            anyhow::bail!("Dependency cycle: {} -> {}", chain.join(" -> "), pkg.package_base);
        }

        let parent = chain.last().map(String::as_str).unwrap_or("package");
        Ui::warning(&format!("{} needs these packages from the AUR: {}", parent, names.join(", ")));
        if !Ui::confirm("Build and install them from the AUR first?") {
            anyhow::bail!(
                "Missing AUR dependencies. Install them first with: rauri -S {}",
                names.join(" ")
            );
        }

        for pkg in &found {
            if pkg.package_base != pkg.name {
                Ui::info(&format!("Installing dependency {} (from {})...", pkg.name, pkg.package_base));
            } else {
                Ui::info(&format!("Installing dependency {}...", pkg.name));
            }
            Self::build_aur(&pkg.package_base, config, fix_sig, true, chain)?;
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

    pub fn update_aur_only(config: &Config, fix_sig: bool) -> Result<()> {
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

                                Self::install_aur(&aur_pkg.package_base, config, fix_sig)?;
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

    /// Update official packages. Pass update_aur=true to also update AUR packages,
    /// refresh=true to force re-downloading the package databases (pacman -Syyu).
    pub fn update_system(config: &Config, update_aur: bool, refresh: bool, fix_sig: bool) -> Result<()> {
        Ui::info("Updating official packages...");

        let flag = if refresh { "-Syyu" } else { "-Syu" };
        fix::run_pacman(&[flag, "--noconfirm"], fix_sig)?;

        Ui::success("Official packages updated");

        if update_aur {
            Self::update_aur_only(config, fix_sig)?;
        }

        Ok(())
    }

    /// Remove packages like `pacman -R<flags>` (e.g. flags "s", "ns", "c"), then
    /// untrack every removed package and delete build folders nothing still needs.
    pub fn remove(package_names: &[String], flags: &str, config: &Config) -> Result<()> {
        let mut targets = Vec::new();
        for name in package_names {
            let actual = Self::resolve_installed(name)
                .ok_or_else(|| anyhow::anyhow!("Package '{}' is not installed", name))?;
            if &actual != name {
                Ui::info(&format!("{} is installed as {}", name, actual));
            }

            let debug = format!("{}-debug", actual);
            if Self::is_installed(&debug) {
                targets.push(debug);
            }
            targets.push(actual);
        }

        let before = Self::installed_packages();

        // No --noconfirm: like pacman, show what -s / -c will take with it and ask.
        let op = format!("-R{}", flags);
        let mut args = vec![op.as_str()];
        args.extend(targets.iter().map(String::as_str));
        fix::run_pacman(&args, false)?;

        let after = Self::installed_packages();
        let mut removed: Vec<&String> = before.difference(&after).collect();
        removed.sort();

        let tracked = PackageTracker::load().unwrap_or_default();
        for name in removed.iter().filter(|n| tracked.contains(n.as_str())) {
            if let Err(e) = PackageTracker::remove(name) {
                Ui::warning(&format!("Failed to untrack {}: {}", name, e));
            }
        }

        let removed: HashSet<String> = removed.into_iter().cloned().collect();
        Self::remove_build_folders(&removed, &after, config);

        let mut shown: Vec<&str> = removed.iter()
            .map(String::as_str)
            .filter(|n| !n.ends_with("-debug"))
            .collect();
        shown.sort();
        Ui::success(&format!("Removed {}", shown.join(", ")));
        Ok(())
    }

    /// The installed package `name` refers to: itself, or a tracked AUR package whose
    /// name contains it (or is contained in it), e.g. "foo" -> "foo-git".
    fn resolve_installed(name: &str) -> Option<String> {
        if Self::is_installed(name) {
            return Some(name.to_string());
        }
        let mut tracked: Vec<String> = PackageTracker::load().unwrap_or_default().into_iter().collect();
        tracked.sort();
        tracked.into_iter()
            .find(|p| (p.contains(name) || name.contains(p.as_str())) && Self::is_installed(p))
    }

    fn is_installed(name: &str) -> bool {
        Command::new("pacman")
            .arg("-Q")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn installed_packages() -> HashSet<String> {
        Command::new("pacman")
            .arg("-Qq")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Delete build folders that built one of the `removed` packages, unless another
    /// package from the same folder (split packages) is still installed.
    fn remove_build_folders(removed: &HashSet<String>, installed: &HashSet<String>, config: &Config) {
        for folder in Aur::package_folders(&config.download_dir) {
            let builds = Aur::srcinfo_pkgnames(&folder);
            let built_removed = builds.iter().any(|n| removed.contains(n));
            let still_needed = builds.iter().any(|n| installed.contains(n));

            if built_removed && !still_needed {
                if let Err(e) = fs::remove_dir_all(&folder) {
                    Ui::warning(&format!("Failed to remove package folder {}: {}", folder.display(), e));
                }
            }
        }
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

    /// List AUR packages tracked by rauri, from the tracking file rather than the
    /// download directory, so the list survives that folder being wiped or moved.
    pub fn list_installed() -> Result<()> {
        let mut names: Vec<String> = PackageTracker::load()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.ends_with("-debug"))
            .collect();
        names.sort();

        if names.is_empty() {
            Ui::info("No AUR packages tracked by rauri.");
            return Ok(());
        }

        // One pacman call: it prints the installed ones and errors about the rest.
        let installed: std::collections::HashMap<String, String> = Command::new("pacman")
            .arg("-Q")
            .args(&names)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.split_once(' ').map(|(n, v)| (n.to_string(), v.to_string())))
                .collect())
            .unwrap_or_default();

        if installed.is_empty() {
            Ui::info("No AUR packages installed via rauri.");
            return Ok(());
        }

        // Only ask the AUR about packages that are actually installed.
        names.retain(|n| installed.contains_key(n));

        let aur = match Aur::get_packages_info(&names) {
            Ok(found) => Some(found),
            Err(e) => {
                Ui::warning(&format!("Could not reach the AUR, not checking for updates: {}", e));
                None
            }
        };

        let is_tty = atty::is(Stream::Stdout);
        let mut outdated = 0;
        for name in &names {
            let aur_pkg = aur.as_ref().and_then(|a| a.get(name));
            let version = &installed[name];

            let is_outdated = aur_pkg.is_some_and(|p| &p.version != version);
            if is_outdated {
                outdated += 1;
            }
            let mut line = Ui::format_package(name, version, is_outdated);
            if aur.is_some() && aur_pkg.is_none() {
                line.push_str(if is_tty { "  \x1b[2m(not in AUR)\x1b[0m" } else { "  (not in AUR)" });
            }

            match aur_pkg.filter(|p| &p.package_base != name) {
                Some(p) if is_tty => println!("  {}{}{} → {}", Colors::BOLD, p.package_base.yellow(), Colors::RESET, line),
                Some(p) => println!("  {} → {}", p.package_base, line),
                None => println!("  {}", line),
            }
        }

        if outdated > 0 {
            let msg = format!("{} packages have updates available", outdated);
            if is_tty {
                println!("\n{}{}{}", Colors::BOLD, msg.yellow(), Colors::RESET);
            } else {
                println!("\n{}", msg);
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
