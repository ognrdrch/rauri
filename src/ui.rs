use colored::*;
use atty::Stream;

pub struct Colors;

impl Colors {
    pub const RESET: &'static str = "\x1b[0m";
    pub const BOLD: &'static str = "\x1b[1m";
    pub const DIM: &'static str = "\x1b[2m";
    #[allow(dead_code)]
    pub const RED: &'static str = "\x1b[31m";
    #[allow(dead_code)]
    pub const GREEN: &'static str = "\x1b[32m";
    #[allow(dead_code)]
    pub const YELLOW: &'static str = "\x1b[33m";
    #[allow(dead_code)]
    pub const BLUE: &'static str = "\x1b[34m";
    #[allow(dead_code)]
    pub const CYAN: &'static str = "\x1b[36m";
}

pub struct Ui;

impl Ui {
    fn is_tty() -> bool {
        atty::is(Stream::Stdout)
    }

    pub fn success(msg: &str) {
        if Self::is_tty() {
            println!("{} {}", "✓".bright_green(), msg.green());
        } else {
            println!("✓ {}", msg);
        }
    }

    pub fn error(msg: &str) {
        if Self::is_tty() {
            eprintln!("{} {}", "✗".bright_red(), msg.red());
        } else {
            eprintln!("✗ {}", msg);
        }
    }

    pub fn warning(msg: &str) {
        if Self::is_tty() {
            println!("{} {}", "⚠".bright_yellow(), msg.yellow());
        } else {
            println!("⚠ {}", msg);
        }
    }

    pub fn info(msg: &str) {
        if Self::is_tty() {
            println!("{} {}", "ℹ".bright_cyan(), msg.cyan());
        } else {
            println!("ℹ {}", msg);
        }
    }

    /// Ask a yes/no question (default yes). Answers no when stdin isn't a terminal.
    pub fn confirm(question: &str) -> bool {
        use std::io::Write;

        if !atty::is(Stream::Stdin) {
            Self::warning(&format!("{} — no terminal to ask, assuming no", question));
            return false;
        }

        if Self::is_tty() {
            print!("{} {} [Y/n] ", "?".bright_magenta(), question.bold());
        } else {
            print!("? {} [Y/n] ", question);
        }
        std::io::stdout().flush().ok();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() {
            return false;
        }
        matches!(input.trim().to_lowercase().as_str(), "" | "y" | "yes")
    }

    pub fn print_help(config: &crate::config::Config) {
        let is_tty = Self::is_tty();

        println!();

        let current_path = config.download_dir.display().to_string();

        if is_tty {
            println!("{}", "Usage:".bold());
        } else {
            println!("Usage:");
        }
        println!("  rauri [OPTIONS] [COMMAND] [ARGS]\n");

        if is_tty {
            println!("{}", "Options:".bold());
            println!("  {}  Clear AUR download directory before command", "-C".yellow());
            println!("  {}  Set AUR download directory path (offers to move existing packages)", "-P <path>".yellow());
            println!("  {}  Fix signature/keyring problems without asking", "--fix-sig".yellow());
        } else {
            println!("Options:");
            println!("  -C          Clear AUR download directory before command");
            println!("  -P <path>   Set AUR download directory path (offers to move existing packages)");
            println!("  --fix-sig   Fix signature/keyring problems without asking");
        }

        if is_tty {
            println!("\n{}", "Current AUR Path:".bold());
            println!("  {}", current_path.green());
        } else {
            println!("\nCurrent AUR Path:");
            println!("  {}", current_path);
        }
        println!();

        let s = &config.cmd_search;
        let sa = format!("{}A", s);
        let i = &config.cmd_install;
        let u = &config.cmd_update_all;
        let uy = config.update_all_refresh_flag().map(|f| format!("{f}, ")).unwrap_or_default();
        let r = &config.cmd_remove;
        let l = &config.cmd_list;
        let la = format!("{}A", l);
        let e = &config.cmd_exclude;
        let inc = &config.cmd_include;
        let lim = config.search_limit;

        let commands: Vec<(String, String)> = vec![
            (format!("{s}, --search <pkg>"), format!("Search packages (top {lim} per repo)")),
            (format!("{sa}, --search-all <pkg>"), "Search packages (show all results)".into()),
            (format!("{i}, --install <pkg>"), "Install package (AUR or official)".into()),
            (format!("{u}, --update-all"), "Update official packages (pacman -Syu)".into()),
            (format!("{uy}{u} --refresh"), "...force-refresh package databases first (pacman -Syyu)".into()),
            (format!("{u} --update-aur"), "Update official packages, then AUR packages".into()),
            ("--update-aur".into(), "Update AUR packages only".into()),
            (format!("{r}, --remove <pkg>..."), "Remove packages and their build folders (asks first)".into()),
            (format!("{r}s, --recursive"), "...also remove dependencies nothing else needs".into()),
            (format!("{r}n, --nosave"), "...also delete config backups (.pacsave)".into()),
            (format!("{r}c, --cascade"), "...also remove packages that depend on them".into()),
            (format!("{r}u, --unneeded"), "...only remove packages nothing depends on".into()),
            (format!("{r}ns, {r}sc, ..."), "Modifiers combine in any order, like pacman".into()),
            (format!("{l}, --list"), "List AUR packages installed via rauri".into()),
            (format!("{la}, {l} --all, --list-all"), "List all installed packages, outdated first".into()),
            (format!("{e}, --exclude [pkg]..."), "Exclude packages from updates (no pkg: show excluded)".into()),
            (format!("{inc}, --include [pkg]..."), "Include excluded packages again (no pkg: show excluded)".into()),
            ("<AUR_URL>".into(), "Install from AUR git link".into()),
        ];
        let width = commands.iter().map(|(usage, _)| usage.len()).max().unwrap_or(0);

        if is_tty {
            println!("{}", "Commands:".bold());
        } else {
            println!("Commands:");
        }
        for (usage, description) in &commands {
            let padded = format!("{:width$}", usage);
            if is_tty {
                println!("  {}  {}", padded.yellow(), description);
            } else {
                println!("  {}  {}", padded, description);
            }
        }

        if is_tty {
            println!("\n{}", "Examples:".bold());
        } else {
            println!("\nExamples:");
        }
        println!("  rauri {s} package-name");
        println!("  rauri {sa} package-name");
        println!("  rauri {i} package-name");
        println!("  rauri {u}");
        println!("  rauri {u} --update-aur");
        if let Some(f) = config.update_all_refresh_flag() {
            println!("  rauri {f} --update-aur  # Force-refresh databases first");
        }
        println!("  rauri --update-aur");
        println!("  rauri {i} --fix-sig package-name  # Refresh keyring / import PGP keys first");
        println!("  rauri {r} package-name");
        println!("  rauri {r}ns package-name  # Also remove unneeded dependencies and config backups");
        println!("  rauri {l}");
        println!("  rauri {la}");
        println!("  rauri {e} package-name  # Skip package-name during updates");
        println!("  rauri {inc} package-name  # Update package-name again");
        println!("  rauri -C {l}  # Clear AUR path then list packages");
        println!("  rauri -P ~/.AUR  # Set AUR path to ~/.AUR");
        println!("  rauri https://aur.archlinux.org/package-name.git");

        let config_path = crate::config::Config::config_path();
        let display_path = config_path.canonicalize().unwrap_or(config_path);
        if is_tty {
            println!("\n{}", format!("Configuration: {}", display_path.display()).dimmed());
        } else {
            println!("\nConfiguration: {}", display_path.display());
        }
    }

    pub fn format_package(name: &str, version: &str, outdated: bool) -> String {
        if !Self::is_tty() {
            if outdated {
                return format!("{} {} (outdated)", name, version);
            } else {
                return format!("{} {}", name, version);
            }
        }

        if outdated {
            format!("{} {} {}", name.bold(), version.yellow(), "(outdated)".yellow())
        } else {
            format!("{} {}", name.bold(), version.green())
        }
    }
}
