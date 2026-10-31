use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub(crate) enum Value {
    Boolean(bool),
    Integer(i64),
    Unsigned(u64),
    Real(f64),
    String(String),

    #[serde(rename = "expanded-string")]
    ExpandedString(String),

    Date(String),
    Data(Vec<u8>),
    Array(Vec<Self>),
    Dictionary(BTreeMap<String, Self>),
}
