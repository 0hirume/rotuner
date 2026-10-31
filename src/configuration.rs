use std::{collections::BTreeMap, env, fs, io::ErrorKind, path::PathBuf};

use miette::{IntoDiagnostic as _, Result, WrapErr as _, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::preferences;

pub(super) type Properties = BTreeMap<String, Value>;
pub(super) type Classes = BTreeMap<String, Properties>;

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Configuration {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub client: Properties,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub playtest: Properties,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub studio: Classes,

    #[serde(default, skip_serializing_if = "Raw::is_empty")]
    pub raw: Raw,
}

#[derive(Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct Raw {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub storage: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preferences: BTreeMap<String, preferences::Value>,
}

impl Raw {
    fn is_empty(&self) -> bool {
        self.storage.is_empty() && self.preferences.is_empty()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum Value {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Components(BTreeMap<String, f64>),
}

impl Configuration {
    pub(super) fn load(path: Option<PathBuf>) -> Result<Self> {
        let path = match path {
            Some(path) => path,
            None => discover()?,
        };

        let source = fs::read_to_string(&path)
            .into_diagnostic()
            .wrap_err_with(|| format!("Cannot read {}", path.display()))?;

        let configuration: Self = toml::from_str(&source)
            .into_diagnostic()
            .wrap_err_with(|| format!("Invalid configuration in {}", path.display()))?;

        Ok(configuration)
    }

    pub(super) fn needs_catalog(&self) -> bool {
        !self.client.is_empty() || !self.playtest.is_empty() || !self.studio.is_empty()
    }

    pub(super) fn files(self) -> [(&'static str, Classes); 3] {
        [
            (
                "GlobalBasicSettings_13.xml",
                BTreeMap::from([("UserGameSettings".into(), self.client)]),
            ),
            (
                "GlobalBasicSettings_13_Studio.xml",
                BTreeMap::from([("UserGameSettings".into(), self.playtest)]),
            ),
            ("GlobalSettings_13.xml", self.studio),
        ]
    }
}

fn discover() -> Result<PathBuf> {
    let current = env::current_dir()
        .into_diagnostic()
        .wrap_err("Cannot locate the current directory")?;

    let mut paths = vec![current.join("rotuner.toml"), current.join("config.toml")];

    let directories = dirs::config_dir()
        .into_iter()
        .chain(dirs::home_dir().map(|home| home.join(".config")));

    for directory in directories {
        let path = directory.join("rotuner").join("config.toml");

        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    for path in &paths {
        match fs::symlink_metadata(path) {
            Ok(_) => return Ok(path.clone()),
            Err(error) if error.kind() == ErrorKind::NotFound => {}

            Err(error) => {
                return Err(error)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("Cannot access {}", path.display()));
            }
        }
    }

    let searched = paths
        .iter()
        .map(|path| format!("  {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n");

    bail!("No configuration found. Searched:\n{searched}\nPass a file path to rotuner apply.");
}
