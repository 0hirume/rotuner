use std::{borrow::Cow, collections::BTreeSet};

use miette::{IntoDiagnostic as _, Result, bail, miette};

use quick_xml::{
    Reader, Writer, XmlVersion,
    events::{BytesStart, BytesText, Event},
};

use super::configuration::{Classes, Value};

pub(super) fn patch(source: &str, classes: &Classes) -> Result<String> {
    validate(source)?;
    let mut reader = Reader::from_str(source);
    let mut output = String::new();
    let mut copied = 0;
    let mut stack = Vec::new();
    let mut class = String::new();
    let mut found = BTreeSet::new();

    loop {
        let beginning = position(&reader)?;

        match reader.read_event().into_diagnostic()? {
            Event::Start(element) | Event::Empty(element) => {
                let opening_end = position(&reader)?;
                let empty = source[..opening_end].ends_with("/>");
                let name = element.name().as_ref().to_owned();

                if stack == ["roblox", "Item", "Properties"] {
                    let property = attribute(&element, "name")?;

                    if let Some(value) = classes
                        .get(&class)
                        .and_then(|properties| properties.get(&property))
                    {
                        if !found.insert((class.clone(), property.clone())) {
                            bail!("Duplicate property {class}.{property}");
                        }

                        if !empty {
                            reader.read_to_end(element.name()).into_diagnostic()?;
                        }

                        output.push_str(&source[copied..beginning]);

                        output.push_str(
                            &render(&element, value)
                                .map_err(|error| miette!("{class}.{property}: {error}"))?,
                        );

                        copied = position(&reader)?;
                        continue;
                    }
                }

                if stack == ["roblox"] && name == "Item" {
                    class = attribute(&element, "class")?;
                }

                if !empty {
                    stack.push(name);
                }
            }

            Event::End(_) => {
                stack.pop();
            }

            Event::Eof => break,
            _ => {}
        }
    }

    for (class, properties) in classes {
        for property in properties.keys() {
            if !found.contains(&(class.clone(), property.clone())) {
                bail!(
                    "Unknown or absent property {class}.{property}; launch and close Roblox or Studio to generate its settings first"
                );
            }
        }
    }

    output.push_str(&source[copied..]);
    validate(&output)?;

    Ok(output)
}

fn position(reader: &Reader<&[u8]>) -> Result<usize> {
    usize::try_from(reader.buffer_position()).into_diagnostic()
}

fn attribute(element: &BytesStart<'_>, name: &str) -> Result<String> {
    element
        .try_get_attribute(name)
        .into_diagnostic()?
        .ok_or_else(|| miette!("Missing XML attribute {name}"))?
        .normalized_value(XmlVersion::Implicit1_0)
        .map(Cow::into_owned)
        .into_diagnostic()
}

fn render(element: &BytesStart<'_>, value: &Value) -> Result<String> {
    let mut writer = Writer::new(Vec::new());

    writer
        .write_event(Event::Start(element.to_owned()))
        .into_diagnostic()?;

    let name = element.name();
    let kind = name.as_ref();

    if let Value::Components(components) = value {
        let required: &[&str] = match kind {
            "Color3" => &["R", "G", "B"],
            "Vector2" => &["X", "Y"],
            "Vector3" => &["X", "Y", "Z"],

            _ => {
                bail!("{kind} does not accept components");
            }
        };

        if components.len() != required.len()
            || required.iter().any(|key| !components.contains_key(*key))
        {
            bail!(
                "{kind} requires exactly these components: {}",
                required.join(", ")
            );
        }

        for key in required {
            let number = components[*key];

            if !valid_float(number) || (kind == "Color3" && !(0.0..=1.0).contains(&number)) {
                bail!("Invalid {kind} component {key}");
            }

            writer
                .create_element(*key)
                .write_text_content(BytesText::new(&number.to_string()))
                .into_diagnostic()?;
        }
    } else {
        let text = scalar(kind, value)?;

        writer
            .write_event(Event::Text(BytesText::new(&text)))
            .into_diagnostic()?;
    }

    writer
        .write_event(Event::End(element.to_end()))
        .into_diagnostic()?;

    String::from_utf8(writer.into_inner()).into_diagnostic()
}

fn scalar(kind: &str, value: &Value) -> Result<String> {
    match (kind, value) {
        ("bool", Value::Boolean(value)) => Ok(value.to_string()),
        ("int", Value::Integer(value)) if i32::try_from(*value).is_ok() => Ok(value.to_string()),
        ("int64" | "float" | "double", Value::Integer(value)) => Ok(value.to_string()),

        ("token" | "Color3uint8", Value::Integer(value)) if u32::try_from(*value).is_ok() => {
            Ok(value.to_string())
        }

        ("float", Value::Float(value)) if valid_float(*value) => Ok(value.to_string()),
        ("double", Value::Float(value)) if value.is_finite() => Ok(value.to_string()),

        ("string" | "QDir", Value::String(value)) if value.chars().all(valid_character) => {
            Ok(value.clone())
        }

        _ => Err(miette!("Value does not match supported XML type {kind}")),
    }
}

fn valid_float(value: f64) -> bool {
    value.is_finite() && value.abs() <= f64::from(f32::MAX)
}

fn valid_character(character: char) -> bool {
    matches!(u32::from(character), 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x0001_0000..=0x0010_FFFF)
}

fn validate(source: &str) -> Result<()> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().check_comments = true;
    let mut depth = 0usize;
    let mut roots = 0;

    loop {
        match reader.read_event().into_diagnostic()? {
            Event::Start(element) | Event::Empty(element) => {
                let empty = source[..position(&reader)?].ends_with("/>");

                if depth == 0 {
                    roots += 1;

                    if roots != 1 || element.name().as_ref() != "roblox" {
                        bail!("Expected one roblox XML root");
                    }
                }

                for attribute in element.attributes() {
                    attribute
                        .into_diagnostic()?
                        .normalized_value(XmlVersion::Implicit1_0)
                        .into_diagnostic()?;
                }

                if !empty {
                    depth += 1;
                }
            }

            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| miette!("Unexpected XML closing tag"))?;
            }

            Event::DocType(_) => {
                bail!("XML document types are not supported");
            }

            Event::Text(text) if depth == 0 && !text.as_ref().trim().is_empty() => {
                bail!("Text outside the XML root");
            }

            Event::Eof => break,
            _ => {}
        }
    }

    if depth != 0 || roots != 1 {
        bail!("Incomplete Roblox XML document");
    }

    Ok(())
}
