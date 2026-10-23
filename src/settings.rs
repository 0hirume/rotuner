use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

use miette::{IntoDiagnostic as _, Result, WrapErr as _, bail, miette};
use tempfile::NamedTempFile;

use super::{configuration::Configuration, xml};

struct Change {
    path: PathBuf,
    original: String,
    replacement: NamedTempFile,
    rollback: NamedTempFile,
}

pub(super) fn apply(directory: &Path, configuration: Configuration) -> Result<usize> {
    if !cfg!(windows) {
        bail!("Rotuner currently supports Windows Roblox installations");
    }

    ensure_closed()?;
    let mut changes = Vec::new();

    for (filename, classes) in configuration.files() {
        if classes.values().all(BTreeMap::is_empty) {
            continue;
        }

        let path = directory.join(filename);
        ensure_regular(&path)?;

        let original = fs::read_to_string(&path)
            .into_diagnostic()
            .wrap_err_with(|| format!("Cannot read {}", path.display()))?;

        let updated = xml::patch(&original, &classes)
            .wrap_err_with(|| format!("Cannot update {filename}"))?;

        if updated != original {
            changes.push(Change {
                replacement: stage(&path, &updated)?,
                rollback: stage(&path, &original)?,
                path,
                original,
            });
        }
    }

    ensure_closed()?;

    for change in &changes {
        ensure_regular(&change.path)?;

        if fs::read_to_string(&change.path).into_diagnostic()? != change.original {
            bail!(
                "{} changed during preparation; nothing was applied",
                change.path.display()
            );
        }
    }

    commit(changes)
}

fn ensure_closed() -> Result<()> {
    let output = Command::new("tasklist.exe")
        .args(["/FO", "CSV", "/NH"])
        .output()
        .into_diagnostic()
        .wrap_err("Cannot check running Roblox applications")?;

    if !output.status.success() {
        bail!("Could not enumerate running applications");
    }

    let processes = String::from_utf8_lossy(&output.stdout);

    for line in processes.lines() {
        let name = line.split(',').next().unwrap_or_default().trim_matches('"');

        if name.eq_ignore_ascii_case("RobloxPlayerBeta.exe")
            || name.eq_ignore_ascii_case("RobloxStudioBeta.exe")
        {
            bail!("Close Roblox and Studio before applying settings");
        }
    }

    Ok(())
}

fn ensure_regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "Cannot access {}; launch and close the application first",
                path.display()
            )
        })?;

    if !metadata.file_type().is_file() || metadata.permissions().readonly() {
        bail!("{} must be a writable regular file", path.display());
    }

    Ok(())
}

fn stage(path: &Path, contents: &str) -> Result<NamedTempFile> {
    let parent = path
        .parent()
        .ok_or_else(|| miette!("Missing settings directory"))?;

    let mut temporary = NamedTempFile::new_in(parent).into_diagnostic()?;
    temporary.write_all(contents.as_bytes()).into_diagnostic()?;
    temporary.as_file().sync_all().into_diagnostic()?;

    Ok(temporary)
}

fn commit(changes: Vec<Change>) -> Result<usize> {
    let mut completed: Vec<(PathBuf, NamedTempFile)> = Vec::new();

    for change in changes {
        if let Err(error) = change.replacement.persist(&change.path) {
            let mut failures = Vec::new();

            for (path, rollback) in completed.into_iter().rev() {
                if let Err(mut error) = rollback.persist(&path) {
                    error.file.disable_cleanup(true);

                    failures.push(format!(
                        "{}: {}; original retained at {}",
                        path.display(),
                        error.error,
                        error.file.path().display()
                    ));
                }
            }

            if !failures.is_empty() {
                bail!(
                    "Failed to write {}: {error}; rollback failed: {}",
                    change.path.display(),
                    failures.join("; ")
                );
            }

            bail!(
                "Failed to write {}: {error}; previous changes were restored",
                change.path.display()
            );
        }

        completed.push((change.path, change.rollback));
    }

    Ok(completed.len())
}
