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

    pub fn print_help() {
        use crate::config::Config;
        
        let is_tty = Self::is_tty();
        
        println!();
        
        // Get current AUR path
        let current_path = Config::load()
            .map(|c| c.download_dir.display().to_string())
            .unwrap_or_else(|_| "Not configured".to_string());
        
        if is_tty {
            println!("{}", "Usage:".bold());
        } else {
            println!("Usage:");
        }
        println!("  rauri [OPTIONS] [COMMAND] [ARGS]\n");
        
        if is_tty {
            println!("{}", "Options:".bold());
            print!("  {}  ", "-C".yellow());
            println!("Clear AUR download directory before executing command");
            print!("  {}  ", "-P <path>".yellow());
            println!("Set AUR download directory path");
        } else {
            println!("Options:");
            println!("  -C  Clear AUR download directory before executing command");
            println!("  -P <path>  Set AUR download directory path");
        }
        
        if is_tty {
            println!("\n{}", "Current AUR Path:".bold());
            println!("  {}", current_path.green());
        } else {
            println!("\nCurrent AUR Path:");
            println!("  {}", current_path);
        }
        println!();
        
        if is_tty {
            println!("{}", "Commands:".bold());
            print!("  {}  ", "-Q <package>".yellow());
            println!("Search for packages");
            print!("  {}  ", "-S <package>".yellow());
            println!("Install package (AUR or official)");
            print!("  {}  ", "-Syu".yellow());
            println!("        Update installed packages");
            print!("  {}  ", "-R <package>".yellow());
            println!("Remove package (also removes package folder)");
            print!("  {}  ", "-L".yellow());
            println!("          List installed packages");
            print!("  {}  ", "<AUR_URL>".yellow());
            println!("   Install from AUR git link");
        } else {
            println!("Commands:");
            println!("  -Q <package>  Search for packages");
            println!("  -S <package>  Install package (AUR or official)");
            println!("  -Syu  Update system packages");
            println!("  -R <package>  Remove package (also removes package folder)");
            println!("  -L  List installed packages");
            println!("  <AUR_GIT_URL>  Install from AUR git link");
        }
        
        if is_tty {
            println!("\n{}", "Examples:".bold());
        } else {
            println!("\nExamples:");
        }
        println!("  rauri -Q package-name");
        println!("  rauri -S package-name");
        println!("  rauri -Syu");
        println!("  rauri -R package-name");
        println!("  rauri -L");
        println!("  rauri -C -L  # Clear AUR path then list packages");
        println!("  rauri -P ~/.AUR  # Set AUR path to ~/.AUR");
        println!("  rauri https://aur.archlinux.org/package-name.git");
        
        // Show configuration path
        if let Ok(config_path) = Config::config_path().canonicalize() {
            if is_tty {
                println!("\n{}", format!("Configuration: {}", config_path.display()).dimmed());
            } else {
                println!("\nConfiguration: {}", config_path.display());
            }
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

