//! Read-only local release diagnostics. Package managers are never executed.
use crate::config;
use crate::lockfile;
use crate::note;
use crate::paths;
use crate::release;
use crate::sections;
use crate::transaction;
use serde::Serialize;
use std::collections::BTreeSet;
use std::env;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorStatus,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    pub passed: bool,
}

impl DoctorReport {
    fn push(&mut self, name: &str, status: DoctorStatus, message: String) {
        self.passed &= status != DoctorStatus::Fail;
        self.checks.push(DoctorCheck {
            name: name.to_owned(),
            status,
            message,
        });
    }

    fn release_checks(&mut self, root: &Path) {
        use DoctorStatus::{Fail, Pass, Unknown};
        let config = match config::load_release_config(root) {
            Ok(config) => {
                self.push(
                    "config",
                    Pass,
                    "Configuration and version files are valid.".to_owned(),
                );
                config
            }
            Err(error) => {
                self.push("config", Fail, format!("{error:#}"));
                self.push(
                    "notes",
                    Unknown,
                    "Configuration validation failed.".to_owned(),
                );
                self.push(
                    "plan",
                    Unknown,
                    "Configuration validation failed.".to_owned(),
                );
                self.push(
                    "lockfiles",
                    Unknown,
                    "Configuration validation failed.".to_owned(),
                );
                return;
            }
        };
        let notes = match note::notes(root, &config) {
            Ok(notes) => {
                self.push(
                    "notes",
                    Pass,
                    format!("{} valid pending notes.", notes.len()),
                );
                notes
            }
            Err(error) => {
                self.push("notes", Fail, format!("{error:#}"));
                self.push("plan", Unknown, "Note validation failed.".to_owned());
                self.push("lockfiles", Unknown, "Note validation failed.".to_owned());
                return;
            }
        };
        let releases = match release::plan(root, &config, &notes) {
            Ok(releases) => {
                self.push(
                    "plan",
                    Pass,
                    format!("{} pending releases.", releases.len()),
                );
                releases
            }
            Err(error) => {
                self.push("plan", Fail, format!("{error:#}"));
                self.push("lockfiles", Unknown, "Release planning failed.".to_owned());
                return;
            }
        };
        let paths = match config::release_version_paths(root, &config, &releases) {
            Ok(paths) => paths,
            Err(error) => {
                self.push("outputs", Fail, format!("{error:#}"));
                self.push(
                    "lockfiles",
                    Unknown,
                    "Version output validation failed.".to_owned(),
                );
                return;
            }
        };
        let locks = match lockfile::discover_paths(root, &paths) {
            Ok(locks) => locks,
            Err(error) => {
                self.push("lockfiles", Fail, format!("{error:#}"));
                return;
            }
        };
        self.push(
            "lockfiles",
            Pass,
            format!(
                "{} existing lockfiles require synchronization.",
                locks.len()
            ),
        );
        let outputs = (|| -> anyhow::Result<()> {
            let mut outputs = paths;
            outputs.extend(locks.iter().map(|lock| lock.path.clone()));
            for release in &releases {
                let package = &config.config.packages[&release.package];
                if sections::enabled(config.changelog.as_ref()) {
                    outputs.push(paths::checked_path(
                        root,
                        &package.path.join("CHANGELOG.md"),
                    )?);
                }
            }
            if !releases.is_empty() {
                outputs.push(paths::checked_path(
                    root,
                    Path::new(".changenote/releases.json"),
                )?);
            }
            transaction::validate_paths(&outputs)
        })();
        match outputs {
            Ok(()) => self.push(
                "outputs",
                Pass,
                "Pending release output path types are valid; write access was not tested."
                    .to_owned(),
            ),
            Err(error) => self.push("outputs", Fail, format!("{error:#}")),
        }
        let tools: BTreeSet<_> = locks.iter().map(|lock| lock.tool()).collect();
        for tool in tools {
            let (status, message) = if executable_on_path(tool) {
                (
                    Pass,
                    format!("{tool} is available on PATH; it was not executed."),
                )
            } else {
                (
                    Fail,
                    format!("Required package manager {tool} is not executable on PATH."),
                )
            };
            self.push(&format!("tool:{tool}"), status, message);
        }
    }

    fn git_checks(&mut self, root: &Path) {
        use DoctorStatus::{Fail, Pass, Unknown};
        // Only fixed read-only Git commands are used. Do not refresh the index.
        let git = |args: &[&str]| {
            Command::new("git")
                .current_dir(root)
                .env("GIT_OPTIONAL_LOCKS", "0")
                .args(["-c", "core.fsmonitor=false"])
                .args(args)
                .output()
        };
        match git(&["rev-parse", "--is-inside-work-tree"]) {
            Ok(output) if output.status.success() && output.stdout.starts_with(b"true") => {}
            Ok(_) => {
                self.push(
                    "git",
                    Unknown,
                    "No local Git working tree is available; check and tag require Git.".to_owned(),
                );
                return;
            }
            Err(error) => {
                self.push(
                    "git",
                    Unknown,
                    format!("Local Git state is unavailable: {error}"),
                );
                return;
            }
        }
        match git(&["rev-parse", "--show-prefix"]) {
            Ok(output)
                if output.status.success()
                    && (output.stdout == b"\n" || output.stdout == b"\r\n") => {}
            Ok(_) => {
                self.push(
                    "git",
                    Fail,
                    "Use the Git repository root; check requires a root checkout.".to_owned(),
                );
                return;
            }
            Err(error) => {
                self.push(
                    "git",
                    Fail,
                    format!("Cannot determine the Git repository root: {error}"),
                );
                return;
            }
        }
        match git(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]) {
            Ok(output) if output.status.success() => {
                if output.stdout.is_empty() {
                    self.push(
                        "git",
                        Pass,
                        "The local Git working tree is clean.".to_owned(),
                    );
                } else {
                    self.push("git", Fail, "The local Git working tree has changes; check and tag require a clean tree.".to_owned());
                }
            }
            Ok(output) => self.push(
                "git",
                Fail,
                format!(
                    "Cannot read local Git state: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ),
            Err(error) => self.push("git", Fail, format!("Cannot read local Git state: {error}")),
        }
    }
}

/// Diagnose local inputs without writes, synchronization, or network access.
/// Unknown cache and credential readiness do not fail the report.
pub fn doctor(root: &Path) -> DoctorReport {
    let mut report = DoctorReport {
        checks: Vec::new(),
        passed: true,
    };
    report.release_checks(root);
    report.git_checks(root);
    report.push(
        "cache",
        DoctorStatus::Unknown,
        "Package-manager cache readiness was not checked; synchronization was not run.".to_owned(),
    );
    report.push(
        "credentials",
        DoctorStatus::Unknown,
        "Remote credential readiness was not checked; no network requests were made.".to_owned(),
    );
    report
}

fn executable_on_path(tool: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|directory| {
        #[cfg(windows)]
        {
            let extensions = env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
            extensions
                .to_string_lossy()
                .split(';')
                .filter(|ext| !ext.is_empty())
                .any(|ext| directory.join(format!("{tool}{ext}")).is_file())
        }
        #[cfg(not(windows))]
        {
            let Ok(metadata) = directory.join(tool).metadata() else {
                return false;
            };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                metadata.is_file()
            }
        }
    })
}
