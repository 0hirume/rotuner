use std::collections::BTreeMap;

use miette::{Result, WrapErr as _, bail, miette};
use schemars::schema_for;
use serde_json::{Map, Value as SchemaValue, json};

use super::{
    property::{self, Property},
    source,
};

use crate::configuration::{Configuration, Properties, Raw};

pub(crate) struct Catalog {
    pub(super) classes: BTreeMap<String, BTreeMap<String, Property>>,
}

impl Catalog {
    pub(crate) fn load() -> Result<Self> {
        Ok(Self {
            classes: source::load()?,
        })
    }

    pub(crate) fn resolve(&self, configuration: &mut Configuration) -> Result<()> {
        self.resolve_properties("client", "UserGameSettings", &mut configuration.client)?;
        self.resolve_properties("playtest", "UserGameSettings", &mut configuration.playtest)?;

        for (class, properties) in &mut configuration.studio {
            if class == "UserGameSettings" {
                bail!("Use the playtest section for Studio gameplay settings");
            }

            self.resolve_properties(&format!("studio.{class}"), class, properties)?;
        }

        Ok(())
    }

    pub(crate) fn schema(&self) -> Result<SchemaValue> {
        let client = self
            .classes
            .get("UserGameSettings")
            .ok_or_else(|| miette!("Missing UserGameSettings in catalog"))?;

        let client = properties_schema(client);

        let studio: Map<_, _> = self
            .classes
            .iter()
            .filter(|(name, _)| *name != "UserGameSettings")
            .map(|(name, properties)| (name.clone(), properties_schema(properties)))
            .collect();

        let properties = [
            ("client".into(), client.clone()),
            ("raw".into(), schema_for!(Raw).to_value()),
            ("playtest".into(), client),
            ("studio".into(), property::object(studio)),
        ]
        .into_iter()
        .collect();

        let mut schema = property::object(properties);
        schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
        schema["title"] = json!("rotuner configuration");

        Ok(schema)
    }

    fn resolve_properties(
        &self,
        section: &str,
        class: &str,
        properties: &mut Properties,
    ) -> Result<()> {
        let catalog = self
            .classes
            .get(class)
            .ok_or_else(|| miette!("Unknown settings class {class}"))?;

        for (name, value) in properties {
            let property = catalog
                .get(name)
                .ok_or_else(|| miette!("Unknown or non-persistent setting {section}.{name}"))?;

            property
                .resolve(value)
                .wrap_err_with(|| format!("Invalid setting {section}.{name}"))?;
        }

        Ok(())
    }
}

fn properties_schema(properties: &BTreeMap<String, Property>) -> SchemaValue {
    let properties = properties
        .iter()
        .map(|(name, property)| (name.clone(), property.schema()))
        .collect();

    property::object(properties)
}
