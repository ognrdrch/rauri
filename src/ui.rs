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
        let r = &config.cmd_remove;
        let l = &config.cmd_list;
        let la = format!("{}A", l);
        let lim = config.search_limit;

        if is_tty {
            println!("{}", "Commands:".bold());
            println!("  {}  Search packages (top {} per repo)",
                format!("{s}, --search <pkg>").yellow(), lim);
            println!("  {}  Search packages (show all results)",
                format!("{sa}, --search-all <pkg>").yellow());
            println!("  {}  Install package (AUR or official)",
                format!("{i}, --install <pkg>").yellow());
            println!("  {}  Update AUR packages only",
                format!("{i}, --update-aur").yellow());
            println!("  {}  Update whole system (pacman -Syy then -Syu, then AUR)",
                format!("{u}, --update-all").yellow());
            println!("  {}  Update official packages only (skip AUR)",
                format!("{u} --skip-aur, --update-all --skip-aur").yellow());
            println!("  {}  Remove packages and their build folders",
                format!("{r}, --remove <pkg>...").yellow());
            println!("  {}  Remove with pacman modifiers: s recursive, n nosave, c cascade, u unneeded",
                format!("{r}s, {r}ns, {r}sc ...").yellow());
            println!("  {}  List AUR packages installed via rauri",
                format!("{l}, --list").yellow());
            println!("  {}  List all installed system packages",
                format!("{la}, --list-all").yellow());
            println!("  {}  Install from AUR git link",
                "<AUR_URL>".yellow());
        } else {
            println!("Commands:");
            println!("  {s}, --search <pkg>                   Search packages (top {lim} per repo)");
            println!("  {sa}, --search-all <pkg>               Search packages (show all results)");
            println!("  {i}, --install <pkg>                  Install package (AUR or official)");
            println!("  {i}, --update-aur                     Update AUR packages only");
            println!("  {u}, --update-all                     Update whole system (pacman -Syy then -Syu, then AUR)");
            println!("  {u} --skip-aur, --update-all --skip-aur  Update official packages only");
            println!("  {r}, --remove <pkg>...                Remove packages and their build folders");
            println!("  {r}s, {r}ns, {r}sc ...                    Remove with pacman modifiers (s, n, c, u)");
            println!("  {l}, --list                           List AUR packages installed via rauri");
            println!("  {la}, --list-all                       List all installed system packages");
            println!("  <AUR_URL>                             Install from AUR git link");
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
        println!("  rauri {u} --skip-aur");
        println!("  rauri --update-aur");
        println!("  rauri {i} --fix-sig package-name  # Refresh keyring / import PGP keys first");
        println!("  rauri {r} package-name");
        println!("  rauri {r}ns package-name  # Also remove unneeded dependencies and config backups");
        println!("  rauri {l}");
        println!("  rauri {la}");
        println!("  rauri -C -{l}  # Clear AUR path then list packages");
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
