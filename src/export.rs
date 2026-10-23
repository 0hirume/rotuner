use serde_json::Value;

pub(super) fn excluded(value: &str) -> bool {
    let value = value.trim_start();

    if value.starts_with("@ByteArray(")
        || value.starts_with("@Variant(")
        || (value.contains('<') && value.contains("</"))
    {
        return true;
    }

    match serde_json::from_str(value) {
        Ok(Value::Array(values)) => values.is_empty(),
        Ok(Value::Object(values)) => values.is_empty(),
        _ => false,
    }
}
