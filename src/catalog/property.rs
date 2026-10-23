use std::collections::BTreeMap;

use miette::{Result, bail};
use schemars::schema_for;
use serde_json::{Map, Value as SchemaValue, json};

use crate::configuration::Value;

pub(super) struct Property {
    pub category: String,
    pub kind: Kind,
    pub deprecated: bool,
}

impl Property {
    pub(super) fn schema(&self) -> SchemaValue {
        let mut schema = self.kind.schema();
        schema["description"] = json!(format!("{} ({})", self.kind.name(), self.category));

        if self.deprecated {
            schema["deprecated"] = json!(true);
        }

        if let Kind::Unsupported(name) = &self.kind {
            schema["description"] = json!(format!("{name} is not supported by rotuner"));
        }

        schema
    }

    pub(super) fn resolve(&self, value: &mut Value) -> Result<()> {
        match &self.kind {
            Kind::Enumeration { name, choices } => {
                let resolved = match value {
                    Value::String(choice) => choices.get(choice).copied(),

                    Value::Integer(number) => {
                        choices.values().find(|choice| **choice == *number).copied()
                    }

                    _ => None,
                };

                let Some(resolved) = resolved else {
                    let names: Vec<_> = choices.keys().map(String::as_str).collect();
                    bail!("Expected {name}: {}", names.join(", "));
                };

                *value = Value::Integer(resolved);
            }

            Kind::Unsupported(name) => {
                bail!("XML type {name} is not supported by rotuner");
            }

            _ => {}
        }

        Ok(())
    }
}

pub(super) enum Kind {
    Boolean,
    Integer,
    Long,
    Float,
    Double,
    Text,
    Directory,
    Color,
    Vector2,
    Vector3,

    Enumeration {
        name: String,
        choices: BTreeMap<String, i64>,
    },

    Unsupported(String),
}

impl Kind {
    pub(super) fn from_name(name: String) -> Self {
        match name.as_str() {
            "bool" => Self::Boolean,
            "int" => Self::Integer,
            "int64" => Self::Long,
            "float" => Self::Float,
            "double" => Self::Double,
            "string" => Self::Text,
            "QDir" => Self::Directory,
            "Color3" => Self::Color,
            "Vector2" => Self::Vector2,
            "Vector3" => Self::Vector3,
            _ => Self::Unsupported(name),
        }
    }

    pub(super) fn name(&self) -> &str {
        match self {
            Self::Boolean => "bool",
            Self::Integer => "int",
            Self::Long => "int64",
            Self::Float => "float",
            Self::Double => "double",
            Self::Text => "string",
            Self::Directory => "QDir",
            Self::Color => "Color3",
            Self::Vector2 => "Vector2",
            Self::Vector3 => "Vector3",
            Self::Enumeration { name, .. } | Self::Unsupported(name) => name,
        }
    }

    fn schema(&self) -> SchemaValue {
        match self {
            Self::Boolean => schema_for!(bool).to_value(),
            Self::Integer => schema_for!(i32).to_value(),
            Self::Long => schema_for!(i64).to_value(),
            Self::Float => json!({ "type": "number", "minimum": -f32::MAX, "maximum": f32::MAX }),
            Self::Double => schema_for!(f64).to_value(),
            Self::Text | Self::Directory => schema_for!(String).to_value(),

            Self::Color => components(
                &["R", "G", "B"],
                &json!({ "type": "number", "minimum": 0, "maximum": 1 }),
            ),

            Self::Vector2 => components(&["X", "Y"], &Self::Float.schema()),
            Self::Vector3 => components(&["X", "Y", "Z"], &Self::Float.schema()),

            Self::Enumeration { choices, .. } => {
                let names: Vec<_> = choices.keys().collect();
                let numbers: Vec<_> = choices.values().collect();

                json!({ "anyOf": [{ "type": "string", "enum": names }, { "type": "integer", "enum": numbers }] })
            }

            Self::Unsupported(_) => json!({ "not": {} }),
        }
    }
}

pub(super) fn object(properties: Map<String, SchemaValue>) -> SchemaValue {
    let mut schema = json!({ "type": "object", "additionalProperties": false });
    schema["properties"] = SchemaValue::Object(properties);

    schema
}

fn components(names: &[&str], value: &SchemaValue) -> SchemaValue {
    let properties = names
        .iter()
        .map(|name| ((*name).to_owned(), value.clone()))
        .collect();

    let mut schema = object(properties);
    schema["required"] = json!(names);

    schema
}
