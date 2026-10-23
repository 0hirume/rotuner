use std::collections::BTreeMap;

use miette::{IntoDiagnostic as _, Result, WrapErr as _};
use serde_json::{Map, Value as JsonValue};

use super::export;

pub(super) fn patch(source: &str, entries: &BTreeMap<String, String>) -> Result<String> {
    let original: Map<String, JsonValue> = serde_json::from_str(source)
        .into_diagnostic()
        .wrap_err("Cannot parse appStorage.json as an object")?;

    let mut storage = original.clone();

    for (name, value) in entries {
        let value = serde_json::from_str(value)
            .into_diagnostic()
            .wrap_err_with(|| format!("Invalid JSON value for raw.storage.{name}"))?;

        storage.insert(name.clone(), value);
    }

    if storage == original {
        return Ok(source.to_owned());
    }

    serde_json::to_string(&storage).into_diagnostic()
}

pub(super) fn export(source: &str) -> Result<BTreeMap<String, String>> {
    let storage: Map<String, JsonValue> = serde_json::from_str(source)
        .into_diagnostic()
        .wrap_err("Cannot parse appStorage.json as an object")?;

    let mut entries = BTreeMap::new();

    for (name, value) in storage {
        if matches!(
            name.as_str(),
            "ExperimentCache" | "DiscoveryClientFallbackCache"
        ) {
            continue;
        }

        let serialized = serde_json::to_string(&value).into_diagnostic()?;

        if !export::excluded(value.as_str().unwrap_or(&serialized)) {
            entries.insert(name, serialized);
        }
    }

    Ok(entries)
}
