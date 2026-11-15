//! Runs pacman / makepkg while watching their stderr for well-known failures
//! (outdated keyring, missing PGP keys, stale db lock, ...). When a failure is
//! recognised the user is offered a fix; if they accept, the fix is applied and
//! the original command is retried. Unfixable failures get a short hint instead.
//! Missing AUR dependencies are returned as a [`MissingAurDeps`] error so the
//! caller can build them and retry.

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::fmt;
use std::io::{Read, Write};
use std::mem::{discriminant, Discriminant};
use std::path::Path;
use std::process::{Command, Stdio};

use atty::Stream;

use crate::ui::Ui;

const DB_LOCK: &str = "/var/lib/pacman/db.lck";

/// pacman messages that mean the keyring can't verify a package or database.
const SIGNATURE_MARKERS: &[&str] = &[
    "signature from \"",
    "(PGP signature)",
    "is unknown trust",
    "is marginal trust",
    "required key missing from keyring",
    "missing required signature",
    "could not be looked up remotely",
];

static UNKNOWN_KEY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"unknown public key ([0-9A-Fa-f]{8,40})").unwrap()
});

static ANSI_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]|\r").unwrap()
});

static TARGET_NOT_FOUND_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"target not found: (\S+)").unwrap()
});

/// makepkg failed because these dependencies aren't in the official repos.
#[derive(Debug)]
pub struct MissingAurDeps(pub Vec<String>);

impl fmt::Display for MissingAurDeps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "missing dependencies not found in the official repos: {}", self.0.join(", "))
    }
}

impl std::error::Error for MissingAurDeps {}

#[derive(Debug)]
enum Failure {
    /// pacman: keyring outdated / package signed by an unknown key.
    Signature,
    /// pacman: /var/lib/pacman/db.lck exists.
    DbLocked,
    /// pacman: package download failed (stale databases or bad mirror).
    Download,
    DiskSpace,
    FileConflict,
    PackageConflict,
    /// makepkg: source signature made by a key missing from the user's gpg keyring.
    UnknownPgpKeys(Vec<String>),
    /// makepkg: dependencies not available in the official repos (usually AUR packages).
    MissingDeps(Vec<String>),
    /// makepkg: source checksums don't match the PKGBUILD.
    Checksum,
}

fn classify(output: &str) -> Option<Failure> {
    let stripped = ANSI_RE.replace_all(output, "");
    let stderr = stripped.as_ref();
    let keys = unique_matches(&UNKNOWN_KEY_RE, stderr);
    if !keys.is_empty() {
        return Some(Failure::UnknownPgpKeys(keys));
    }
    if SIGNATURE_MARKERS.iter().any(|m| stderr.contains(m)) {
        return Some(Failure::Signature);
    }
    if stderr.contains("unable to lock database") {
        return Some(Failure::DbLocked);
    }
    if stderr.contains("not enough free disk space") {
        return Some(Failure::DiskSpace);
    }
    if stderr.contains("exists in filesystem") {
        return Some(Failure::FileConflict);
    }
    if stderr.contains("unresolvable package conflicts") || stderr.contains("are in conflict") {
        return Some(Failure::PackageConflict);
    }
    if stderr.contains("failed retrieving file") || stderr.contains("failed to retrieve some files") {
        return Some(Failure::Download);
    }
    let deps = unique_matches(&TARGET_NOT_FOUND_RE, stderr);
    if !deps.is_empty() {
        return Some(Failure::MissingDeps(deps));
    }
    if stderr.contains("did not pass the validity check") {
        return Some(Failure::Checksum);
    }
    None
}

fn unique_matches(re: &Regex, text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for cap in re.captures_iter(text) {
        let m = cap[1].to_string();
        if !found.contains(&m) {
            found.push(m);
        }
    }
    found
}

/// Copy everything from `pipe` to `out` as it arrives, returning a copy of it.
fn tee(mut pipe: impl Read, mut out: impl Write) -> Result<String> {
    let mut captured = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = pipe.read(&mut buf).context("Failed to read command output")?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).ok();
        out.flush().ok();
        captured.extend_from_slice(&buf[..n]);
    }
    Ok(String::from_utf8_lossy(&captured).into_owned())
}

/// Run a command with stdin/stdout attached to the terminal (so prompts and
/// progress bars work) while stderr is mirrored live and also captured.
fn run_captured(mut cmd: Command) -> Result<(bool, String)> {
    let mut child = cmd
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start command")?;

    let captured = tee(child.stderr.take().expect("stderr is piped"), std::io::stderr())?;
    let status = child.wait().context("Failed to wait for command")?;
    Ok((status.success(), captured))
}

/// Run a shell command line inside a pseudo-terminal via util-linux `script`,
/// so tools that only colour output on a terminal (makepkg) keep their colours.
/// Everything the command prints is mirrored live and also captured.
fn run_in_pty(command_line: &str, dir: &Path) -> Result<(bool, String)> {
    let mut child = Command::new("script")
        .args(["--quiet", "--return", "--flush", "--command", command_line, "/dev/null"])
        .current_dir(dir)
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to start script (util-linux)")?;

    let captured = tee(child.stdout.take().expect("stdout is piped"), std::io::stdout())?;
    let status = child.wait().context("Failed to wait for makepkg")?;
    Ok((status.success(), captured))
}

/// `sudo pacman <args>` with English messages so failures can be recognised.
fn pacman_cmd(args: &[&str]) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.args(["env", "LC_ALL=C", "pacman"]).args(args);
    cmd
}

/// Run `sudo pacman <args>`, offering fixes for recognised failures.
pub fn run_pacman(args: &[&str], fix_sig: bool) -> Result<()> {
    let label = format!("pacman {}", args.join(" "));
    // A retried -Su/-Syu upgrades the system itself, so fixes don't need to.
    let upgrades = args.iter().any(|a| a.starts_with("-S") && a.contains('u'));
    run_with_fixes(&label, || run_captured(pacman_cmd(args)), fix_sig, upgrades)
}

/// Run `makepkg -si` in `package_dir`, offering fixes for recognised failures.
/// With `fix_sig`, the PGP keys listed in the package's .SRCINFO are imported first.
/// With `as_dep`, the package is installed as a dependency (`--asdeps`).
pub fn run_makepkg(package_dir: &Path, fix_sig: bool, as_dep: bool) -> Result<()> {
    if fix_sig {
        let keys = srcinfo_pgp_keys(package_dir);
        if !keys.is_empty() {
            import_pgp_keys(&keys)?;
        }
    }

    let command_line = if as_dep { "makepkg -si --asdeps" } else { "makepkg -si" };

    // Only use a pseudo-terminal when there is a terminal to show colours on.
    if atty::is(Stream::Stdout) {
        return run_with_fixes(command_line, || run_in_pty(command_line, package_dir), fix_sig, false);
    }

    run_with_fixes(command_line, || {
        let mut cmd = Command::new("makepkg");
        cmd.args(command_line.split_whitespace().skip(1))
            .current_dir(package_dir)
            .env("LC_ALL", "C");
        run_captured(cmd)
    }, fix_sig, false)
}

/// `upgrades`: the command being retried is a system upgrade (`-Su`/`-Syu`).
fn run_with_fixes(
    label: &str,
    run: impl Fn() -> Result<(bool, String)>,
    fix_sig: bool,
    upgrades: bool,
) -> Result<()> {
    // Each kind of fix is attempted at most once, so a fix that doesn't help can't loop.
    let mut attempted: HashSet<Discriminant<Failure>> = HashSet::new();

    loop {
        let (ok, output) = run()?;
        if ok {
            return Ok(());
        }

        let Some(failure) = classify(&output) else {
            anyhow::bail!("{} failed", label);
        };

        if let Failure::MissingDeps(deps) = failure {
            return Err(MissingAurDeps(deps).into());
        }

        if !attempted.insert(discriminant(&failure)) {
            Ui::warning("The automatic fix did not resolve the problem.");
            print_hint(&failure);
            anyhow::bail!("{} failed", label);
        }

        if !offer_fix(&failure, fix_sig, upgrades)? {
            print_hint(&failure);
            anyhow::bail!("{} failed", label);
        }

        Ui::info(&format!("Retrying {}...", label));
    }
}

/// Returns true if a fix was applied and the command should be retried.
fn offer_fix(failure: &Failure, fix_sig: bool, upgrades: bool) -> Result<bool> {
    match failure {
        Failure::Signature => {
            Ui::warning("Package signature verification failed — your archlinux-keyring is probably outdated.");
            // A newer keyring in the already-synced databases can be installed on its own:
            // no database refresh, so no partial upgrade and no full system upgrade needed.
            if keyring_upgradable() {
                if fix_sig || Ui::confirm(
                    "Install the newer archlinux-keyring (pacman -S archlinux-keyring, no system upgrade)?",
                ) {
                    install_keyring()?;
                    return Ok(true);
                }
                return Ok(false);
            }
            // Otherwise the databases must be refreshed, and after -Sy the system has to be
            // upgraded (-Su) too, since Arch doesn't support partial upgrades.
            let prompt = if upgrades {
                "Refresh the databases and update archlinux-keyring (pacman -Sy archlinux-keyring), then continue the upgrade?"
            } else {
                "Your package databases are out of date too. Refresh them, update archlinux-keyring and upgrade the system (pacman -Sy archlinux-keyring, then pacman -Su)?"
            };
            if fix_sig || Ui::confirm(prompt) {
                refresh_keyring(!upgrades)?;
                return Ok(true);
            }
            Ok(false)
        }
        Failure::UnknownPgpKeys(keys) => {
            Ui::warning(&format!(
                "Source signature could not be verified, missing PGP key(s): {}",
                keys.join(", ")
            ));
            if fix_sig || Ui::confirm("Import the missing key(s) from a keyserver (gpg --recv-keys)?") {
                import_pgp_keys(keys)?;
                return Ok(true);
            }
            Ok(false)
        }
        Failure::DbLocked => {
            if pacman_running() {
                Ui::warning("Another pacman process is running. Wait for it to finish and try again.");
                return Ok(false);
            }
            Ui::warning(&format!("The pacman database is locked ({}), but no pacman process is running.", DB_LOCK));
            if Ui::confirm("Remove the stale lock file? (make sure no other package manager like pamac or GNOME Software is running)") {
                let status = Command::new("sudo")
                    .args(["rm", "-f", DB_LOCK])
                    .status()
                    .context("Failed to remove lock file")?;
                return Ok(status.success());
            }
            Ok(false)
        }
        Failure::Download => {
            Ui::warning("Some packages could not be downloaded (stale package databases or an unreachable mirror).");
            if Ui::confirm("Force-refresh the package databases and upgrade the system (pacman -Syyu)?") {
                run_captured(pacman_cmd(&["-Syyu", "--noconfirm"]))?;
                return Ok(true);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn print_hint(failure: &Failure) {
    let hint = match failure {
        Failure::Signature => "Re-run with --fix-sig to update the keyring automatically, or run:\n  \
            sudo pacman -Sy --needed archlinux-keyring && sudo pacman -Su".to_string(),
        Failure::UnknownPgpKeys(keys) => format!(
            "Re-run with --fix-sig to import the keys automatically, or run:\n  gpg --recv-keys {}",
            keys.join(" ")
        ),
        Failure::DbLocked => format!(
            "If no package manager is running, remove the lock with: sudo rm {}", DB_LOCK
        ),
        Failure::Download => "Check your mirrors in /etc/pacman.d/mirrorlist, e.g.:\n  \
            sudo reflector --latest 20 --sort rate --save /etc/pacman.d/mirrorlist".to_string(),
        Failure::DiskSpace => "Free up disk space, e.g. clear the package cache with: sudo pacman -Sc".to_string(),
        Failure::FileConflict => "Files already exist on disk that this package wants to install.\n  \
            Check who owns them with: pacman -Qo <file>\n  \
            Only use 'pacman --overwrite' if you are sure they are leftovers.".to_string(),
        Failure::PackageConflict => "rauri runs pacman with --noconfirm, which declines replacing conflicting packages.\n  \
            Run the pacman command yourself to resolve the conflict interactively.".to_string(),
        Failure::MissingDeps(deps) => format!(
            "Install the missing dependencies first with: rauri -S {}", deps.join(" ")
        ),
        Failure::Checksum => "Downloaded sources don't match the PKGBUILD checksums. Upstream may have changed\n  \
            the files; check the package's AUR comments before installing.".to_string(),
    };
    Ui::info(&hint);
}

/// True if the synced databases already have a newer archlinux-keyring than the installed one.
fn keyring_upgradable() -> bool {
    Command::new("pacman")
        .args(["-Qu", "archlinux-keyring"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Install archlinux-keyring from the current databases, without refreshing them.
fn install_keyring() -> Result<()> {
    Ui::info("Updating archlinux-keyring...");
    let (ok, _) = run_captured(pacman_cmd(&["-S", "--needed", "--noconfirm", "archlinux-keyring"]))?;
    if !ok {
        anyhow::bail!("Failed to update archlinux-keyring");
    }
    Ui::success("Keyring updated");
    Ok(())
}

/// Refresh the databases and update archlinux-keyring. With `upgrade`, also run
/// `pacman -Su` afterwards so the system isn't left partially upgraded.
fn refresh_keyring(upgrade: bool) -> Result<()> {
    Ui::info("Updating archlinux-keyring...");
    let (ok, _) = run_captured(pacman_cmd(&["-Sy", "--needed", "--noconfirm", "archlinux-keyring"]))?;

    if !ok {
        // A very old keyring may not even trust the new keyring package.
        Ui::warning("Updating archlinux-keyring failed.");
        if !Ui::confirm("Re-initialise the pacman keyring (pacman-key --init, --populate archlinux) and try again?") {
            anyhow::bail!("Failed to update archlinux-keyring");
        }
        for args in [&["--init"][..], &["--populate", "archlinux"][..]] {
            let status = Command::new("sudo")
                .arg("pacman-key")
                .args(args)
                .status()
                .context("Failed to run pacman-key")?;
            if !status.success() {
                anyhow::bail!("pacman-key {} failed", args.join(" "));
            }
        }
        let (ok, _) = run_captured(pacman_cmd(&["-Sy", "--needed", "--noconfirm", "archlinux-keyring"]))?;
        if !ok {
            anyhow::bail!("Failed to update archlinux-keyring");
        }
    }

    if upgrade {
        Ui::info("Upgrading system with the new keyring...");
        let (ok, _) = run_captured(pacman_cmd(&["-Su", "--noconfirm"]))?;
        if !ok {
            anyhow::bail!("System upgrade after keyring update failed");
        }
    }

    Ui::success("Keyring updated");
    Ok(())
}

/// Import keys into the user's gpg keyring, falling back to keyserver.ubuntu.com.
fn import_pgp_keys(keys: &[String]) -> Result<()> {
    Ui::info(&format!("Importing PGP key(s): {}", keys.join(", ")));

    let default_ok = Command::new("gpg")
        .arg("--recv-keys")
        .args(keys)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if default_ok {
        return Ok(());
    }

    Ui::warning("Default keyserver failed, trying keyserver.ubuntu.com...");
    let status = Command::new("gpg")
        .args(["--keyserver", "hkps://keyserver.ubuntu.com", "--recv-keys"])
        .args(keys)
        .status()
        .context("Failed to run gpg")?;
    if !status.success() {
        anyhow::bail!("Failed to import PGP key(s): {}", keys.join(", "));
    }
    Ok(())
}

/// The `validpgpkeys` entries of a package's .SRCINFO.
fn srcinfo_pgp_keys(package_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(package_dir.join(".SRCINFO"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("validpgpkeys = "))
        .map(|key| key.trim().to_string())
        .collect()
}

fn pacman_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "pacman"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_failures() {
        let sig = "error: foo: signature from \"Some Packager <x@archlinux.org>\" is unknown trust\n\
                   :: File /var/cache/pacman/pkg/foo.pkg.tar.zst is corrupted (invalid or corrupted package (PGP signature)).";
        assert!(matches!(classify(sig), Some(Failure::Signature)));

        let keys = "    foo-1.0.tar.gz ... FAILED (unknown public key 1234ABCD5678EF90)\n\
                    ==> ERROR: One or more PGP signatures could not be verified!";
        match classify(keys) {
            Some(Failure::UnknownPgpKeys(k)) => assert_eq!(k, vec!["1234ABCD5678EF90"]),
            other => panic!("unexpected {:?}", other),
        }

        let lock = "error: failed to init transaction (unable to lock database)\nerror: could not lock database: File exists";
        assert!(matches!(classify(lock), Some(Failure::DbLocked)));

        let colored = "\x1b[1;31merror:\x1b[0m target not found: libbar\r\n";
        match classify(colored) {
            Some(Failure::MissingDeps(d)) => assert_eq!(d, vec!["libbar"]),
            other => panic!("unexpected {:?}", other),
        }

        let deps = "error: target not found: libfoo-git\nerror: target not found: libfoo-git\n==> ERROR: 'pacman' failed to install missing dependencies.";
        match classify(deps) {
            Some(Failure::MissingDeps(d)) => assert_eq!(d, vec!["libfoo-git"]),
            other => panic!("unexpected {:?}", other),
        }

        let dl = "error: failed retrieving file 'foo-1.0-1-x86_64.pkg.tar.zst' from mirror : The requested URL returned error: 404\nerror: failed to retrieve some files";
        assert!(matches!(classify(dl), Some(Failure::Download)));

        assert!(classify("error: something unexpected").is_none());
    }
}
