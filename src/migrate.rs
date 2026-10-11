//! Changing the AUR download path: moving existing package folders to the new
//! location, and adopting AUR packages found there that rauri doesn't track yet.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::aur::Aur;
use crate::config::Config;
use crate::tracker::PackageTracker;
use crate::ui::Ui;

/// Set a new AUR download path, offering to move the package folders from the
/// old path, then look for untracked AUR packages in the new path.
pub fn change_aur_path(config: &mut Config, new_dir: &Path) -> Result<()> {
    let old_dir = config.download_dir.clone();
    let same = old_dir.canonicalize().map_or(false, |old| old == new_dir);

    if same {
        Ui::info(&format!("AUR download path is already {}", new_dir.display()));
    } else {
        migrate_packages(&old_dir, new_dir)?;
        config.download_dir = new_dir.to_path_buf();
        config.save()?;
        Ui::success(&format!("AUR download path set to: {}", new_dir.display()));
    }

    adopt_untracked(new_dir)
}

/// Look for AUR package folders in `dir` whose packages are installed but not
/// tracked by rauri (e.g. built by hand or by another helper), and offer to track them.
pub fn adopt_untracked(dir: &Path) -> Result<()> {
    let foreign = foreign_packages();
    let tracked = PackageTracker::load().unwrap_or_default();

    let mut untracked: Vec<String> = Aur::package_folders(dir)
        .iter()
        .flat_map(|folder| Aur::srcinfo_pkgnames(folder))
        .filter(|name| foreign.contains(name) && !tracked.contains(name))
        .collect();
    untracked.sort();
    untracked.dedup();

    if untracked.is_empty() {
        return Ok(());
    }

    Ui::warning(&format!(
        "Found {} installed AUR package(s) in {} that rauri doesn't manage: {}",
        untracked.len(), dir.display(), untracked.join(", ")
    ));
    if !Ui::confirm("Track them with rauri so they are updated with -S / -Syu?") {
        return Ok(());
    }

    for name in &untracked {
        if let Err(e) = PackageTracker::add(name) {
            Ui::warning(&format!("Failed to track {}: {}", name, e));
        }
    }
    Ui::success(&format!("Now tracking {} package(s)", untracked.len()));
    Ok(())
}

fn migrate_packages(old_dir: &Path, new_dir: &Path) -> Result<()> {
    let folders = Aur::package_folders(old_dir);
    if folders.is_empty() {
        return Ok(());
    }

    Ui::info(&format!(
        "{} package folder(s) in the current AUR path {}",
        folders.len(), old_dir.display()
    ));
    if !Ui::confirm(&format!("Move them to {}?", new_dir.display())) {
        Ui::info("Leaving package folders in the old location");
        return Ok(());
    }

    let mut moved = 0;
    for folder in &folders {
        let Some(name) = folder.file_name() else { continue };
        let target = new_dir.join(name);

        if target.exists() {
            Ui::warning(&format!("Skipping {}: {} already exists", name.to_string_lossy(), target.display()));
            continue;
        }

        match move_dir(folder, &target) {
            Ok(()) => moved += 1,
            Err(e) => Ui::warning(&format!("Failed to move {}: {}", folder.display(), e)),
        }
    }
    Ui::success(&format!("Moved {} of {} package folder(s)", moved, folders.len()));

    // Only succeeds if nothing else is left in the old directory.
    if fs::remove_dir(old_dir).is_ok() {
        Ui::info(&format!("Removed empty directory {}", old_dir.display()));
    }
    Ok(())
}

/// Rename, falling back to `mv` when the target is on a different filesystem.
fn move_dir(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }

    let status = Command::new("mv")
        .args(["--no-clobber", "--no-target-directory", "--"])
        .arg(from)
        .arg(to)
        .status()
        .context("Failed to run mv")?;
    if !status.success() {
        anyhow::bail!("mv exited with {}", status);
    }
    Ok(())
}

/// Installed packages that aren't in any sync repo (`pacman -Qmq`).
fn foreign_packages() -> HashSet<String> {
    Command::new("pacman")
        .arg("-Qmq")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect())
        .unwrap_or_default()
}
