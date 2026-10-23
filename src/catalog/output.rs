use std::{
    collections::BTreeMap,
    env,
    io::{self, IsTerminal as _, Write as _},
};

use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use console::{Style, Term, colors_enabled};
use miette::{IntoDiagnostic as _, Result, bail, miette};

use super::{
    model::Catalog,
    property::{Kind, Property},
};

pub(crate) fn print(catalog: &Catalog, filter: Option<&str>) -> Result<()> {
    let terminal = Term::stdout();
    let (_, width) = terminal.size();
    let mut output = io::stdout().lock();
    let colored = output.is_terminal() && env::var_os("NO_COLOR").is_none() && colors_enabled();
    let style = Style::new().force_styling(colored);
    let filter = filter.map(str::to_lowercase);

    let client = catalog
        .classes
        .get("UserGameSettings")
        .ok_or_else(|| miette!("Missing UserGameSettings in catalog"))?;

    let sections = [
        ("Client", "client".to_owned(), client),
        ("Playtest", "playtest".to_owned(), client),
    ]
    .into_iter()
    .chain(
        catalog
            .classes
            .iter()
            .filter(|(name, _)| *name != "UserGameSettings")
            .map(|(name, properties)| ("Studio", format!("studio.{name}"), properties)),
    );

    let mut found = false;

    for (title, section, properties) in sections {
        let mut categories = BTreeMap::<&str, Vec<_>>::new();

        for (name, property) in properties {
            if filter
                .as_ref()
                .is_some_and(|filter| !format!("{section}.{name}").to_lowercase().contains(filter))
            {
                continue;
            }

            categories
                .entry(property.category.as_str())
                .or_default()
                .push((name.as_str(), property));
        }

        for (category, properties) in categories {
            if found {
                writeln!(output).into_diagnostic()?;
            }

            found = true;

            writeln!(
                output,
                "{}  {}",
                style
                    .clone()
                    .bold()
                    .apply_to(format!("{title} / {category}")),
                style.clone().dim().apply_to(format!("[{section}]"))
            )
            .into_diagnostic()?;

            writeln!(output, "{}", table(&properties, width, colored)).into_diagnostic()?;
        }
    }

    if !found {
        bail!("No settings match the catalog filter");
    }

    Ok(())
}

fn table(properties: &[(&str, &Property)], width: u16, colored: bool) -> Table {
    let mut table = Table::new();

    table
        .load_style(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(width)
        .set_header(
            ["Setting", "Type", "Values"]
                .map(|name| Cell::new(name).add_attribute(Attribute::Bold)),
        );

    if !colored {
        table.force_no_tty();
    }

    for (name, property) in properties {
        table.add_row([
            Cell::new(name).fg(Color::Cyan),
            Cell::new(property.kind.name()).add_attribute(Attribute::Dim),
            values(property),
        ]);
    }

    table
}

fn values(property: &Property) -> Cell {
    let mut values = match &property.kind {
        Kind::Enumeration { choices, .. } => {
            let mut choices: Vec<_> = choices.iter().collect();
            choices.sort_by_key(|(_, number)| **number);

            choices
                .into_iter()
                .map(|(name, number)| format!("{name} ({number})"))
                .collect::<Vec<_>>()
                .join("\n")
        }

        Kind::Boolean => "true, false".to_owned(),
        Kind::Color => "R, G, B: 0..1".to_owned(),
        Kind::Vector2 => "X, Y".to_owned(),
        Kind::Vector3 => "X, Y, Z".to_owned(),
        Kind::Unsupported(_) => "Unsupported XML type".to_owned(),

        Kind::Integer | Kind::Long | Kind::Float | Kind::Double | Kind::Text | Kind::Directory => {
            String::new()
        }
    };

    if property.deprecated {
        if !values.is_empty() {
            values.push('\n');
        }

        values.push_str("Deprecated by Roblox");
    }

    let cell = Cell::new(values);

    if property.deprecated || matches!(&property.kind, Kind::Unsupported(_)) {
        cell.fg(Color::Yellow)
    } else {
        cell
    }
}
