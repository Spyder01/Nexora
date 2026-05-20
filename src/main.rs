use clap::{Parser, Subcommand};
use nexora::lua::repl::{OpenMode, exec_script, run};

/// Nexora — embedded graph database
#[derive(Parser)]
#[command(
    name    = "nexora",
    version,
    about   = "Nexora — embedded graph database with a Lua scripting REPL",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Database file used when no subcommand is given (REPL mode).
    #[arg(
        value_name = "PATH",
        default_value = "nexora.nxr",
        help = "Database file (created automatically if it does not exist)"
    )]
    path: std::path::PathBuf,

    /// Force-create a new database; fails if the file already exists (REPL mode only).
    #[arg(long, help = "Force-create a new database (fails if PATH already exists)")]
    new: bool,

    /// Disable WAL — faster but no crash safety (REPL mode only).
    #[arg(long, help = "Disable write-ahead log (no crash safety)")]
    no_wal: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Execute a Lua script file against a database (non-interactive).
    Exec {
        /// Database file.
        #[arg(value_name = "PATH")]
        db: std::path::PathBuf,

        /// Lua script file to run.
        #[arg(value_name = "SCRIPT")]
        script: std::path::PathBuf,

        /// Force-create a new database; fails if PATH already exists.
        #[arg(long)]
        new: bool,

        /// Disable WAL — faster but no crash safety.
        #[arg(long)]
        no_wal: bool,
    },

    /// Evaluate an inline Lua string against a database (non-interactive).
    Eval {
        /// Database file.
        #[arg(value_name = "PATH")]
        db: std::path::PathBuf,

        /// Lua code to evaluate.
        #[arg(value_name = "SCRIPT")]
        script: String,

        /// Force-create a new database; fails if PATH already exists.
        #[arg(long)]
        new: bool,

        /// Disable WAL — faster but no crash safety.
        #[arg(long)]
        no_wal: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        None => {
            let mode = if cli.new { OpenMode::ForceNew } else { OpenMode::Auto };
            run(&cli.path, mode, !cli.no_wal)
        }
        Some(Command::Exec { db, script, new, no_wal }) => {
            let mode = if new { OpenMode::ForceNew } else { OpenMode::Auto };
            match std::fs::read_to_string(&script) {
                Err(e) => {
                    eprintln!("error: cannot read '{}': {e}", script.display());
                    std::process::exit(1);
                }
                Ok(src) => exec_script(&db, &src, mode, !no_wal),
            }
        }
        Some(Command::Eval { db, script, new, no_wal }) => {
            let mode = if new { OpenMode::ForceNew } else { OpenMode::Auto };
            exec_script(&db, &script, mode, !no_wal)
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
