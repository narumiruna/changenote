use anyhow::Result;
use changenote::Bump;
use clap::{Parser, Subcommand};
use semver::Version;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Language-independent change notes and releases")]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create configuration and a language-independent VERSION file.
    Init {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "0.0.0")]
        version: Version,
    },
    /// Add a Markdown note without an interactive prompt.
    Add {
        name: String,
        #[arg(short, long, required = true)]
        package: Vec<String>,
        #[arg(long, value_enum)]
        bump: Bump,
        #[arg(short, long)]
        message: String,
    },
    /// Print the pending release plan as JSON without changing files.
    Status {
        /// Include the bump for each note in the JSON output.
        #[arg(long)]
        include_note_bumps: bool,
    },
    /// Apply the plan, update changelogs, and consume notes.
    Version,
    /// Create local annotated tags for the committed release plan.
    Tag,
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { name, version } => changenote::init(&cli.root, &name, version)?,
        Commands::Add {
            name,
            package,
            bump,
            message,
        } => println!(
            "{}",
            changenote::add(&cli.root, &name, package, bump, &message)?.display()
        ),
        Commands::Status { include_note_bumps } => {
            let releases = changenote::status(&cli.root)?;
            if include_note_bumps {
                let mut plan = serde_json::to_value(&releases)?;
                for (entry, release) in plan.as_array_mut().unwrap().iter_mut().zip(&releases) {
                    entry["note_bumps"] = serde_json::to_value(&release.note_bumps)?;
                }
                println!("{}", serde_json::to_string_pretty(&plan)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&releases)?);
            }
        }
        Commands::Version => println!(
            "{}",
            serde_json::to_string_pretty(&changenote::version(&cli.root)?)?
        ),
        Commands::Tag => {
            for tag in changenote::tag(&cli.root)? {
                println!("{tag}");
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
