use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

use miette::{IntoDiagnostic as _, Result, WrapErr as _, bail, miette};
use plist::{Date, Dictionary, Value as Property};
use tempfile::NamedTempFile;

use crate::export;

use super::{Value, key};

const DOMAIN: &str = "com.roblox.RobloxStudio";

pub(crate) struct Change {
    original: Option<Dictionary>,
    replacement: Dictionary,
    staged: NamedTempFile,
    rollback: Option<NamedTempFile>,
}

impl Change {
    pub(crate) fn prepare(entries: &BTreeMap<String, Value>) -> Result<Option<Self>> {
        if entries.is_empty() {
            return Ok(None);
        }

        let original = read()?;
        let mut replacement = original.clone().unwrap_or_default();
        let mut names = BTreeSet::new();

        for (name, value) in entries {
            let name = key::decode(name)?.join(".");

            if !names.insert(name.clone()) {
                bail!("Duplicate Studio preference key");
            }

            replacement.insert(name, encode(value)?);
        }

        if original.as_ref() == Some(&replacement) {
            return Ok(None);
        }

        Ok(Some(Self {
            staged: stage(&replacement)?,
            rollback: original.as_ref().map(stage).transpose()?,
            original,
            replacement,
        }))
    }

    pub(crate) fn commit(mut self) -> Result<usize> {
        if read()? != self.original {
            bail!("Studio preferences changed during preparation; no preferences were applied");
        }

        let result = import(self.staged.path()).and_then(|()| {
            if read()?.as_ref() != Some(&self.replacement) {
                bail!("Studio preferences did not retain the requested values");
            }

            Ok(1)
        });

        match result {
            Ok(count) => Ok(count),

            Err(error) => {
                let restored = self.restore().and_then(|()| {
                    if read()? != self.original {
                        bail!("Studio preference restoration could not be verified");
                    }

                    Ok(())
                });

                if let Err(rollback_error) = restored {
                    if let Some(rollback) = &mut self.rollback {
                        rollback.disable_cleanup(true);

                        return Err(error).wrap_err(format!("Studio preference rollback failed: {rollback_error}; original retained at {}", rollback.path().display()));
                    }

                    return Err(error).wrap_err(format!(
                        "Studio preference rollback failed: {rollback_error}"
                    ));
                }

                Err(error).wrap_err("Original Studio preferences were restored")
            }
        }
    }

    fn restore(&self) -> Result<()> {
        if let Some(rollback) = &self.rollback {
            return import(rollback.path());
        }

        let output = Command::new("/usr/bin/defaults")
            .args(["delete", DOMAIN])
            .output()
            .into_diagnostic()
            .wrap_err("Cannot restore absent Studio preferences")?;

        if !output.status.success() && read()?.is_some() {
            bail!("Cannot remove newly created Studio preferences");
        }

        Ok(())
    }
}

pub(crate) fn export() -> Result<BTreeMap<String, Value>> {
    let mut entries = BTreeMap::new();

    for (name, value) in read()?.unwrap_or_default() {
        if name.ends_with("timeLastUsed") || name.ends_with("lastUsedDir") {
            continue;
        }

        let excluded = match &value {
            Property::Data(_) => true,
            Property::String(value) => export::excluded(value),
            Property::Array(values) => values.is_empty(),
            Property::Dictionary(values) => values.is_empty(),
            _ => false,
        };

        if !excluded {
            entries.insert(key::encode(name.split('.')), decode(value)?);
        }
    }

    Ok(entries)
}

fn decode(value: Property) -> Result<Value> {
    Ok(match value {
        Property::Boolean(value) => Value::Boolean(value),

        Property::Integer(value) => {
            if let Some(value) = value.as_signed() {
                Value::Integer(value)
            } else {
                Value::Unsigned(
                    value
                        .as_unsigned()
                        .ok_or_else(|| miette!("Invalid preference integer"))?,
                )
            }
        }

        Property::Real(value) => Value::Real(value),
        Property::String(value) => Value::String(value),
        Property::Date(value) => Value::Date(value.to_xml_format()),
        Property::Data(value) => Value::Data(value),

        Property::Array(values) => {
            Value::Array(values.into_iter().map(decode).collect::<Result<_>>()?)
        }

        Property::Dictionary(values) => Value::Dictionary(
            values
                .into_iter()
                .map(|(name, value)| Ok((name, decode(value)?)))
                .collect::<Result<_>>()?,
        ),

        _ => {
            bail!("Unsupported Studio preference type");
        }
    })
}

fn encode(data: &Value) -> Result<Property> {
    Ok(match data {
        Value::Boolean(value) => Property::Boolean(*value),
        Value::Integer(value) => Property::Integer((*value).into()),
        Value::Unsigned(value) => Property::Integer((*value).into()),
        Value::Real(value) => Property::Real(*value),
        Value::String(value) => Property::String(value.clone()),

        Value::ExpandedString(_) => {
            bail!("Expanded preference strings are not supported on macOS");
        }

        Value::Date(value) => Property::Date(Date::from_xml_format(value).into_diagnostic()?),
        Value::Data(value) => Property::Data(value.clone()),

        Value::Array(values) => Property::Array(values.iter().map(encode).collect::<Result<_>>()?),

        Value::Dictionary(values) => Property::Dictionary(
            values
                .iter()
                .map(|(name, value)| Ok((name.clone(), encode(value)?)))
                .collect::<Result<_>>()?,
        ),
    })
}

fn read() -> Result<Option<Dictionary>> {
    let domains = Command::new("/usr/bin/defaults")
        .arg("domains")
        .output()
        .into_diagnostic()
        .wrap_err("Cannot enumerate preference domains")?;

    if !domains.status.success() {
        bail!("Cannot enumerate preference domains");
    }

    let domains = String::from_utf8(domains.stdout).into_diagnostic()?;

    if !domains.split(',').any(|domain| domain.trim() == DOMAIN) {
        return Ok(None);
    }

    let output = Command::new("/usr/bin/defaults")
        .args(["export", DOMAIN, "-"])
        .output()
        .into_diagnostic()
        .wrap_err("Cannot export Studio preferences")?;

    if !output.status.success() {
        bail!("Cannot export Studio preferences");
    }

    let preferences = Property::from_reader_xml(output.stdout.as_slice())
        .into_diagnostic()
        .wrap_err("Cannot parse Studio preferences")?;

    Ok(Some(preferences.into_dictionary().ok_or_else(|| {
        miette!("Studio preferences must be a dictionary")
    })?))
}

fn stage(values: &Dictionary) -> Result<NamedTempFile> {
    let mut file = NamedTempFile::new().into_diagnostic()?;

    Property::Dictionary(values.clone())
        .to_writer_xml(file.as_file_mut())
        .into_diagnostic()?;

    file.as_file().sync_all().into_diagnostic()?;

    Ok(file)
}

fn import(path: &Path) -> Result<()> {
    let output = Command::new("/usr/bin/defaults")
        .args(["import", DOMAIN])
        .arg(path)
        .output()
        .into_diagnostic()
        .wrap_err("Cannot import Studio preferences")?;

    if !output.status.success() {
        bail!("Cannot import Studio preferences");
    }

    Ok(())
}
