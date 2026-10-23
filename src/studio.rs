use miette::{IntoDiagnostic as _, Result, WrapErr as _, bail};
use windows_registry::{CURRENT_USER, Key, Transaction, Type, Value as RegistryValue};

use super::{
    configuration::{Registry, RegistryData},
    export,
};

const ROOT: &str = r"Software\Roblox\RobloxStudio";
const VALUE_NOT_FOUND: i32 = -2_147_024_894;

pub(super) struct Change {
    transaction: Transaction,
    count: usize,
}

impl Change {
    pub(super) fn prepare(entries: &[Registry]) -> Result<Option<Self>> {
        if entries.is_empty() {
            return Ok(None);
        }

        let transaction = Transaction::new()
            .into_diagnostic()
            .wrap_err("Cannot start Studio registry transaction")?;

        let mut count = 0;

        for entry in entries {
            if entry.name.contains('\0')
                || entry.path.contains(['\0', '/'])
                || (!entry.path.is_empty()
                    && entry
                        .path
                        .split('\\')
                        .any(|part| matches!(part, "" | "." | "..")))
            {
                bail!(
                    "Registry entries require a relative Studio path and a name without null characters"
                );
            }

            let replacement = encode(&entry.data)?;

            let path = if entry.path.is_empty() {
                ROOT.to_owned()
            } else {
                format!("{ROOT}\\{}", entry.path)
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

            match key.get_value(&entry.name) {
                Ok(original) if original == replacement => continue,
                Ok(_) => {}
                Err(error) if error.code().0 == VALUE_NOT_FOUND => {}

                Err(error) => {
                    return Err(error).into_diagnostic().wrap_err_with(|| {
                        format!("Cannot read Studio registry value {}", entry.name)
                    });
                }
            }

            key.set_value(&entry.name, &replacement)
                .into_diagnostic()
                .wrap_err_with(|| format!("Cannot stage Studio registry value {}", entry.name))?;

            count += 1;
        }

        Ok(Some(Self { transaction, count }))
    }

    pub(super) fn commit(self) -> Result<usize> {
        self.transaction
            .commit()
            .into_diagnostic()
            .wrap_err("Cannot commit Studio registry transaction")?;

        Ok(self.count)
    }
}

pub(super) fn export() -> Result<Vec<Registry>> {
    let root = match CURRENT_USER.open(ROOT) {
        Ok(root) => root,
        Err(error) if error.code().0 == VALUE_NOT_FOUND => return Ok(Vec::new()),

        Err(error) => {
            return Err(error)
                .into_diagnostic()
                .wrap_err("Cannot read Studio registry");
        }
    };

    let mut entries = Vec::new();
    export_key(&root, "", &mut entries)?;

    Ok(entries)
}

fn export_key(key: &Key, path: &str, entries: &mut Vec<Registry>) -> Result<()> {
    for (name, value) in key.values().into_diagnostic()? {
        if name.ends_with("timeLastUsed") || name.ends_with("lastUsedDir") {
            continue;
        }

        let data = decode(&value);

        let excluded = match &data {
            RegistryData::Binary(_) | RegistryData::Raw { .. } => true,

            RegistryData::String(value) | RegistryData::ExpandString(value) => {
                export::excluded(value)
            }

            RegistryData::Strings(values) => values.is_empty(),
            RegistryData::Integer(_) | RegistryData::Long(_) => false,
        };

        if excluded {
            continue;
        }

        entries.push(Registry {
            path: path.to_owned(),
            name,
            data,
        });
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

fn decode(value: &RegistryValue) -> RegistryData {
    let decoded = match value.ty() {
        Type::String => String::try_from(value.clone()).map(RegistryData::String),
        Type::ExpandString => String::try_from(value.clone()).map(RegistryData::ExpandString),
        Type::MultiString => Vec::<String>::try_from(value.clone()).map(RegistryData::Strings),
        Type::U32 => u32::try_from(value.clone()).map(RegistryData::Integer),
        Type::U64 => u64::try_from(value.clone()).map(RegistryData::Long),
        Type::Bytes => Ok(RegistryData::Binary(value.to_vec())),

        Type::Other(_) => Ok(RegistryData::Raw {
            kind: value.ty().into(),
            bytes: value.to_vec(),
        }),
    };

    if let Ok(decoded) = decoded
        && encode(&decoded).is_ok_and(|encoded| encoded == *value)
    {
        return decoded;
    }

    RegistryData::Raw {
        kind: value.ty().into(),
        bytes: value.to_vec(),
    }
}

fn encode(data: &RegistryData) -> Result<RegistryValue> {
    let value = match data {
        RegistryData::String(value) | RegistryData::ExpandString(value) => {
            if value.contains('\0') {
                bail!(
                    "Registry strings cannot contain null characters; use REG_BINARY for raw bytes"
                );
            }

            let mut encoded = RegistryValue::from(value.as_str());

            if matches!(data, RegistryData::ExpandString(_)) {
                encoded.set_ty(Type::ExpandString);
            }

            encoded
        }

        RegistryData::Strings(values) => {
            if values
                .iter()
                .any(|value| value.is_empty() || value.contains('\0'))
            {
                bail!("REG_MULTI_SZ entries must be nonempty strings without null characters");
            }

            let mut wide = Vec::new();

            for value in values {
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

        RegistryData::Integer(value) => RegistryValue::from(*value),
        RegistryData::Long(value) => RegistryValue::from(*value),
        RegistryData::Binary(value) => RegistryValue::from(value.as_slice()),

        RegistryData::Raw { kind, bytes } => {
            let kind = Type::from(*kind);

            if matches!(kind, Type::String | Type::ExpandString | Type::MultiString)
                && !bytes.ends_with(&[0, 0])
            {
                bail!("Raw registry strings require a UTF-16 null terminator");
            }

            let mut value = RegistryValue::from(bytes.as_slice());
            value.set_ty(kind);

            value
        }
    };

    Ok(value)
}
