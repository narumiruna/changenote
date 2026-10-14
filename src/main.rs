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
        name: Option<String>,
        /// Shared-bump mode; cannot be combined with --major/--minor/--patch.
        #[arg(short, long, requires = "bump", conflicts_with_all = ["major", "minor", "patch"])]
        package: Vec<String>,
        #[arg(long, value_enum, requires = "package", conflicts_with_all = ["major", "minor", "patch"])]
        bump: Option<Bump>,
        /// Repeat for each package requiring a major bump.
        #[arg(long)]
        major: Vec<String>,
        /// Repeat for each package requiring a minor bump.
        #[arg(long)]
        minor: Vec<String>,
        /// Repeat for each package requiring a patch bump.
        #[arg(long)]
        patch: Vec<String>,
        #[arg(short, long)]
        message: String,
    },
    /// Print the pending release plan as JSON without changing files.
    Status {
        /// Include the bump for each note in the JSON output.
        #[arg(long)]
        include_note_bumps: bool,
        /// Include direct note paths and fixed-group causes in the JSON output.
        #[arg(long)]
        include_note_paths: bool,
    },
    /// Check committed PR changes against a merge base; requires a clean Git tree.
    /// This is a PR-level note check, not per-package coverage. Prints JSON; writes no files.
    Check {
        #[arg(long)]
        since: String,
        /// Exempt this PR from the missing-note rule, not input validation.
        #[arg(long, requires = "reason")]
        skip_release: bool,
        #[arg(long, requires = "skip_release")]
        reason: Option<String>,
    },
    /// Apply the plan, update changelogs, and consume notes.
    Version {
        /// Reconstruct older release trees. Reserved for the publication Action.
        #[arg(long, hide = true)]
        legacy_output: bool,
        /// Reconstruct releases made before lockfile synchronization.
        #[arg(long, hide = true)]
        no_lockfiles: bool,
    },
    /// Print one package's exact changelog version body without changing files.
    ReleaseNotes {
        #[arg(long)]
        package: String,
        #[arg(long)]
        version: Version,
    },
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
            major,
            minor,
            patch,
            message,
        } => {
            let mut changes: Vec<_> = package.into_iter().map(|p| (p, bump.unwrap())).collect();
            for (packages, bump) in [
                (major, Bump::Major),
                (minor, Bump::Minor),
                (patch, Bump::Patch),
            ] {
                changes.extend(packages.into_iter().map(|p| (p, bump)));
            }
            let path = if let Some(name) = name {
                changenote::add_with_bumps(&cli.root, &name, changes, &message)?
            } else {
                changenote::add_generated(&cli.root, changes, &message)?
            };
            println!("{}", path.display());
        }
        Commands::Status {
            include_note_bumps,
            include_note_paths,
        } => {
            let details = changenote::status_with_note_paths(&cli.root)?;
            let mut output = Vec::new();
            for detail in details {
                let mut entry = if include_note_paths {
                    serde_json::to_value(&detail)?
                } else {
                    serde_json::to_value(&detail.release)?
                };
                if include_note_bumps {
                    entry["note_bumps"] = serde_json::to_value(&detail.release.note_bumps)?;
                }
                output.push(entry);
            }
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Commands::Check {
            since,
            skip_release,
            reason,
        } => {
            let report = changenote::check(&cli.root, &since, skip_release, reason.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            anyhow::ensure!(
                report.passed,
                "changed packages have no PR note; add a note or use --skip-release --reason"
            );
        }
        Commands::Version {
            legacy_output,
            no_lockfiles,
        } => println!(
            "{}",
            serde_json::to_string_pretty(&changenote::version_with_output(
                &cli.root,
                legacy_output,
                !legacy_output && !no_lockfiles
            )?)?
        ),
        Commands::ReleaseNotes { package, version } => {
            print!(
                "{}",
                changenote::release_notes(&cli.root, &package, &version)?
            );
        }
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
