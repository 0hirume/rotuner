use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Write as _},
    path::{Path, PathBuf},
    process::Command,
};

use miette::{IntoDiagnostic as _, Result, WrapErr as _, bail, miette};
use tempfile::NamedTempFile;

use super::{
    arguments::Source,
    catalog::Catalog,
    configuration::{Classes, Configuration},
    desktop, xml,
};

#[cfg(windows)]
use super::studio;

struct Change {
    path: PathBuf,
    original: String,
    replacement: NamedTempFile,
    rollback: NamedTempFile,
}

pub(super) fn directory() -> Result<PathBuf> {
    let directory = if cfg!(target_os = "macos") {
        dirs::home_dir().map(|home| home.join("Library"))
    } else if cfg!(windows) {
        dirs::data_local_dir()
    } else {
        bail!("Roblox settings are supported on Windows and macOS");
    };

    Ok(directory
        .ok_or_else(|| miette!("Cannot locate the Roblox settings directory"))?
        .join("Roblox"))
}

pub(super) fn apply(directory: &Path, configuration: Configuration) -> Result<usize> {
    if !cfg!(any(windows, target_os = "macos")) {
        bail!("Roblox settings are supported on Windows and macOS");
    }

    if !cfg!(windows) && !configuration.raw.registry.is_empty() {
        bail!("Studio registry writes are only supported on Windows");
    }

    ensure_closed()?;
    let mut changes = Vec::new();

    #[cfg(windows)]
    let preferences = studio::Change::prepare(&configuration.raw.registry)?;

    if !configuration.raw.storage.is_empty() {
        let path = directory.join("LocalStorage").join("appStorage.json");

        changes.extend(prepare(path, |source| {
            desktop::patch(source, &configuration.raw.storage)
        })?);
    }

    for (filename, classes) in configuration.files() {
        if classes.values().all(BTreeMap::is_empty) {
            continue;
        }

        changes.extend(prepare(directory.join(filename), |source| {
            xml::patch(source, &classes)
        })?);
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

    commit(
        changes,
        #[cfg(windows)]
        preferences,
    )
}

pub(super) fn export(directory: &Path, source: Option<Source>) -> Result<Configuration> {
    if !cfg!(any(windows, target_os = "macos")) {
        bail!("Roblox settings are supported on Windows and macOS");
    }

    if !cfg!(windows) && matches!(source, Some(Source::StudioRegistry)) {
        bail!("Studio registry export is only supported on Windows");
    }

    let mut configuration = Configuration::default();

    if matches!(source, None | Some(Source::Settings)) {
        let catalog = Catalog::load()?;

        for (filename, target) in [
            ("GlobalBasicSettings_13.xml", &mut configuration.client),
            (
                "GlobalBasicSettings_13_Studio.xml",
                &mut configuration.playtest,
            ),
        ] {
            let mut classes = export_xml(&directory.join(filename), &catalog)?;
            *target = classes.remove("UserGameSettings").unwrap_or_default();
        }

        configuration.studio = export_xml(&directory.join("GlobalSettings_13.xml"), &catalog)?;
    }

    if matches!(source, None | Some(Source::ClientStorage))
        && let Some(source) =
            read_optional(&directory.join("LocalStorage").join("appStorage.json"))?
    {
        configuration.raw.storage = desktop::export(&source)?;
    }

    #[cfg(windows)]
    if matches!(source, None | Some(Source::StudioRegistry)) {
        configuration.raw.registry = studio::export()?;
    }

    Ok(configuration)
}

fn export_xml(path: &Path, catalog: &Catalog) -> Result<Classes> {
    read_optional(path)?.map_or_else(
        || Ok(Classes::new()),
        |source| {
            xml::export(&source, catalog)
                .wrap_err_with(|| format!("Cannot export {}", path.display()))
        },
    )
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),

        Err(error) => Err(error)
            .into_diagnostic()
            .wrap_err_with(|| format!("Cannot read {}", path.display())),
    }
}

fn ensure_closed() -> Result<()> {
    let (program, arguments) = if cfg!(windows) {
        ("tasklist.exe", ["/FO", "CSV", "/NH"])
    } else {
        ("/bin/ps", ["-A", "-o", "comm="])
    };

    let output = Command::new(program)
        .args(arguments)
        .output()
        .into_diagnostic()
        .wrap_err("Cannot check running Roblox applications")?;

    if !output.status.success() {
        bail!("Could not enumerate running applications");
    }

    let processes = String::from_utf8_lossy(&output.stdout);

    for line in processes.lines() {
        let name = if cfg!(windows) {
            line.split(',').next().unwrap_or_default().trim_matches('"')
        } else {
            line.trim().rsplit('/').next().unwrap_or_default()
        };

        if name.eq_ignore_ascii_case("RobloxPlayerBeta.exe")
            || name.eq_ignore_ascii_case("RobloxStudioBeta.exe")
            || matches!(name, "RobloxPlayer" | "RobloxStudio")
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

fn prepare(path: PathBuf, update: impl FnOnce(&str) -> Result<String>) -> Result<Option<Change>> {
    ensure_regular(&path)?;

    let original = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("Cannot read {}", path.display()))?;

    let updated =
        update(&original).wrap_err_with(|| format!("Cannot update {}", path.display()))?;

    if updated == original {
        return Ok(None);
    }

    Ok(Some(Change {
        replacement: stage(&path, &updated)?,
        rollback: stage(&path, &original)?,
        path,
        original,
    }))
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

fn commit(
    changes: Vec<Change>,
    #[cfg(windows)] preferences: Option<studio::Change>,
) -> Result<usize> {
    let mut completed = Vec::new();

    let result: Result<usize> = (|| {
        for change in changes {
            change
                .replacement
                .persist(&change.path)
                .into_diagnostic()
                .wrap_err_with(|| format!("Cannot write {}", change.path.display()))?;

            completed.push((change.path, change.rollback));
        }

        #[cfg(windows)]
        if let Some(preferences) = preferences {
            return Ok(completed.len() + preferences.commit()?);
        }

        Ok(completed.len())
    })();

    let error = match result {
        Ok(count) => return Ok(count),
        Err(error) => error,
    };

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
        return Err(error).wrap_err(format!("Rollback failed: {}", failures.join("; ")));
    }

    Err(error).wrap_err("Previous changes were restored")
}
