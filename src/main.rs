use clap::Parser;
use nexora::lua::repl::{OpenMode, run};

/// Nexora — embedded graph database
#[derive(Parser)]
#[command(
    name    = "nexora",
    version,
    about   = "Nexora — embedded graph database with a Lua scripting REPL",
    long_about = None,
)]
struct Cli {
    /// Path to the database file.
    #[arg(
        value_name = "PATH",
        default_value = "nexora.nxr",
        help = "Database file (created automatically if it does not exist)"
    )]
    path: std::path::PathBuf,

    /// Create a new database. Exits with an error if the file already exists.
    #[arg(long, help = "Force-create a new database (fails if PATH already exists)")]
    new: bool,
}

fn main() {
    let cli = Cli::parse();
    let mode = if cli.new { OpenMode::ForceNew } else { OpenMode::Auto };

    if let Err(e) = run(&cli.path, mode) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
