use std::collections::BTreeMap;

use miette::{IntoDiagnostic as _, Result, WrapErr as _, bail, miette};
use windows_registry::{CURRENT_USER, Key, Transaction, Type, Value as RegistryValue};

use crate::export;

use super::{Value, key};

const ROOT: &str = r"Software\Roblox\RobloxStudio";
const VALUE_NOT_FOUND: i32 = -2_147_024_894;

pub(crate) struct Change {
    transaction: Transaction,
    count: usize,
}

impl Change {
    pub(crate) fn prepare(entries: &BTreeMap<String, Value>) -> Result<Option<Self>> {
        if entries.is_empty() {
            return Ok(None);
        }

        let transaction = Transaction::new()
            .into_diagnostic()
            .wrap_err("Cannot start Studio registry transaction")?;

        let mut count = 0;

        for (name, value) in entries {
            let components = key::decode(name)?;

            let (name, path) = components
                .split_last()
                .ok_or_else(|| miette!("Missing preference key"))?;

            if path.iter().any(|part| {
                part.is_empty() || part.contains(['\\', '/']) || matches!(part.as_str(), "." | "..")
            }) {
                bail!("Studio preference groups require valid relative registry paths");
            }

            let replacement = encode(value)?;

            let path = if path.is_empty() {
                ROOT.to_owned()
            } else {
                format!("{ROOT}\\{}", path.join("\\"))
            };

            let key = CURRENT_USER
                .options()
                .read()
                .write()
                .create()
                .transaction(&transaction)
                .open(&path)
                .into_diagnostic()
                .wrap_err_with(|| format!("Cannot prepare Studio registry key {path}"))?;

            match key.get_value(name) {
                Ok(original) if original == replacement => continue,
                Ok(_) => {}
                Err(error) if error.code().0 == VALUE_NOT_FOUND => {}

                Err(error) => {
                    return Err(error)
                        .into_diagnostic()
                        .wrap_err_with(|| format!("Cannot read Studio registry value {name}"));
                }
            }

            key.set_value(name, &replacement)
                .into_diagnostic()
                .wrap_err_with(|| format!("Cannot stage Studio registry value {name}"))?;

            count += 1;
        }

        Ok(Some(Self { transaction, count }))
    }

    pub(crate) fn commit(self) -> Result<usize> {
        self.transaction
            .commit()
            .into_diagnostic()
            .wrap_err("Cannot commit Studio registry transaction")?;

        Ok(self.count)
    }
}

pub(crate) fn export() -> Result<BTreeMap<String, Value>> {
    let root = match CURRENT_USER.open(ROOT) {
        Ok(root) => root,
        Err(error) if error.code().0 == VALUE_NOT_FOUND => return Ok(BTreeMap::new()),

        Err(error) => {
            return Err(error)
                .into_diagnostic()
                .wrap_err("Cannot read Studio registry");
        }
    };

    let mut entries = BTreeMap::new();
    export_key(&root, "", &mut entries)?;

    Ok(entries)
}

fn decode(value: &RegistryValue) -> Option<Value> {
    let decoded = match value.ty() {
        Type::String => String::try_from(value.clone()).map(Value::String),
        Type::ExpandString => String::try_from(value.clone()).map(Value::ExpandedString),

        Type::MultiString => Vec::<String>::try_from(value.clone())
            .map(|values| Value::Array(values.into_iter().map(Value::String).collect())),

        Type::U32 => u32::try_from(value.clone()).map(|value| Value::Integer(i64::from(value))),
        Type::U64 => u64::try_from(value.clone()).map(Value::Unsigned),
        Type::Bytes | Type::Other(_) => return None,
    }
    .ok()?;

    encode(&decoded)
        .is_ok_and(|encoded| encoded == *value)
        .then_some(decoded)
}

fn encode(data: &Value) -> Result<RegistryValue> {
    let value = match data {
        Value::String(value) | Value::ExpandedString(value) => {
            if value.contains('\0') {
                bail!("Preference strings cannot contain null characters; use data for raw bytes");
            }

            let mut encoded = RegistryValue::from(value.as_str());

            if matches!(data, Value::ExpandedString(_)) {
                encoded.set_ty(Type::ExpandString);
            }

            encoded
        }

        Value::Array(values) => {
            let mut wide = Vec::new();

            for value in values {
                let Value::String(value) = value else {
                    bail!("Windows preference arrays require strings");
                };

                if value.is_empty() || value.contains('\0') {
                    bail!(
                        "Windows preference arrays require nonempty strings without null characters"
                    );
                }

                wide.extend(value.encode_utf16());
                wide.push(0);
            }

            if wide.is_empty() {
                wide.push(0);
            }

            wide.push(0);
            let bytes: Vec<_> = wide.into_iter().flat_map(u16::to_le_bytes).collect();
            let mut encoded = RegistryValue::from(bytes.as_slice());
            encoded.set_ty(Type::MultiString);

            encoded
        }

        Value::Boolean(value) => RegistryValue::from(if *value { "true" } else { "false" }),

        Value::Integer(value) => {
            if let Ok(value) = u32::try_from(*value) {
                RegistryValue::from(value)
            } else {
                RegistryValue::from(
                    u64::try_from(*value)
                        .into_diagnostic()
                        .wrap_err("Windows preference integers must be nonnegative")?,
                )
            }
        }

        Value::Unsigned(value) => RegistryValue::from(*value),

        Value::Real(value) => {
            if !value.is_finite() {
                bail!("Windows preference real numbers must be finite");
            }

            RegistryValue::from(value.to_string().as_str())
        }

        Value::Data(value) => RegistryValue::from(value.as_slice()),

        Value::Date(_) | Value::Dictionary(_) => {
            bail!("This preference type cannot be represented in the Windows registry");
        }
    };

    Ok(value)
}

fn export_key(key: &Key, path: &str, entries: &mut BTreeMap<String, Value>) -> Result<()> {
    for (name, value) in key.values().into_diagnostic()? {
        if name.ends_with("timeLastUsed") || name.ends_with("lastUsedDir") {
            continue;
        }

        let Some(value) = decode(&value) else {
            continue;
        };

        let excluded = match &value {
            Value::String(value) | Value::ExpandedString(value) => export::excluded(value),
            Value::Array(values) => values.is_empty(),
            _ => false,
        };

        if excluded {
            continue;
        }

        let name = key::encode(
            path.split('\\')
                .filter(|part| !part.is_empty())
                .chain([name.as_str()]),
        );

        entries.insert(name, value);
    }

    for name in key.keys().into_diagnostic()? {
        let child = key.open(&name).into_diagnostic()?;

        let path = if path.is_empty() {
            name
        } else {
            format!("{path}\\{name}")
        };

        export_key(&child, &path, entries)?;
    }

    Ok(())
}
