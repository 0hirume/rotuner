use std::mem;

use miette::{Result, bail};

pub(super) fn encode<'component>(parts: impl IntoIterator<Item = &'component str>) -> String {
    parts
        .into_iter()
        .map(|part| part.replace('\\', "\\\\").replace('.', "\\."))
        .collect::<Vec<_>>()
        .join(".")
}

pub(super) fn decode(name: &str) -> Result<Vec<String>> {
    if name.contains('\0') {
        bail!("Studio preference names cannot contain null characters");
    }

    let mut parts = Vec::new();
    let mut part = String::new();
    let mut escaped = false;

    for character in name.chars() {
        if escaped {
            if !matches!(character, '.' | '\\') {
                bail!("Preference keys only allow escaping dots and backslashes");
            }

            part.push(character);
            escaped = false;
        } else {
            match character {
                '\\' => escaped = true,
                '.' => parts.push(mem::take(&mut part)),
                _ => part.push(character),
            }
        }
    }

    if escaped {
        bail!("Incomplete preference key escape");
    }

    parts.push(part);

    Ok(parts)
}
