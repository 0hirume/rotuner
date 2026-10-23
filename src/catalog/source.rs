use std::{collections::BTreeMap, process::Command};

use miette::{IntoDiagnostic as _, Result, WrapErr as _, bail};
use serde::Deserialize;
use serde_json::Value;

use super::property::{Kind, Property};

const SOURCE: &str =
    "https://raw.githubusercontent.com/MaximumADHD/Roblox-Client-Tracker/roblox/Full-API-Dump.json";

const CLASSES: [&str; 10] = [
    "UserGameSettings",
    "GameSettings",
    "LuaSettings",
    "DebugSettings",
    "PhysicsSettings",
    "SlimDebugSettings",
    "Studio",
    "NetworkSettings",
    "RenderSettings",
    "TaskScheduler",
];

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Dump {
    classes: Vec<Class>,
    enums: Vec<Enumeration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Class {
    name: String,
    members: Vec<Member>,
}

#[derive(Deserialize)]
#[serde(tag = "MemberType")]
enum Member {
    #[serde(rename_all = "PascalCase")]
    Property {
        name: String,
        category: String,
        value_type: ValueType,
        serialization: Serialization,
        #[serde(default)]
        tags: Vec<Value>,
    },

    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ValueType {
    category: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Serialization {
    can_load: bool,
    can_save: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Enumeration {
    name: String,
    items: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Item {
    name: String,
    value: i64,
}

pub(super) fn load() -> Result<BTreeMap<String, BTreeMap<String, Property>>> {
    let response = Command::new("curl.exe")
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "30",
            SOURCE,
        ])
        .output()
        .into_diagnostic()
        .wrap_err("Cannot run Windows curl to download the Roblox Full API Dump")?;

    if !response.status.success() {
        bail!(
            "Cannot download the Roblox Full API Dump: {}",
            String::from_utf8_lossy(&response.stderr).trim()
        );
    }

    let dump: Dump = serde_json::from_slice(&response.stdout)
        .into_diagnostic()
        .wrap_err("Cannot parse the Roblox Full API Dump")?;

    let enumerations: BTreeMap<_, _> = dump
        .enums
        .into_iter()
        .map(|entry| (entry.name, entry.items))
        .collect();

    let mut classes = BTreeMap::new();

    for class in dump.classes {
        if !CLASSES.contains(&class.name.as_str()) {
            continue;
        }

        let mut properties = BTreeMap::new();

        for member in class.members {
            let Member::Property {
                name,
                category,
                value_type,
                serialization,
                tags,
            } = member
            else {
                continue;
            };

            if !serialization.can_load
                || !serialization.can_save
                || tags.iter().any(|tag| tag.as_str() == Some("ReadOnly"))
            {
                continue;
            }

            let kind = if value_type.category == "Enum" {
                let items = enumerations.get(&value_type.name).ok_or_else(|| {
                    miette::miette!("Missing enum {} in the Full API Dump", value_type.name)
                })?;

                let choices = items
                    .iter()
                    .map(|item| (item.name.clone(), item.value))
                    .collect();

                Kind::Enumeration {
                    name: value_type.name,
                    choices,
                }
            } else {
                Kind::from_name(value_type.name)
            };

            let deprecated = tags.iter().any(|tag| tag.as_str() == Some("Deprecated"));

            if properties
                .insert(
                    name,
                    Property {
                        category,
                        kind,
                        deprecated,
                    },
                )
                .is_some()
            {
                bail!("Duplicate property in API class {}", class.name);
            }
        }

        if classes.insert(class.name, properties).is_some() {
            bail!("Duplicate settings class in the Full API Dump");
        }
    }

    if CLASSES.iter().any(|name| !classes.contains_key(*name)) {
        bail!("The Full API Dump is missing a required settings class");
    }

    Ok(classes)
}
