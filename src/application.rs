use std::io::{self, Write as _};

use clap::Parser as _;
use miette::{IntoDiagnostic as _, Result, miette};

use super::{
    arguments::{Arguments, Command},
    catalog::{self, Catalog},
    configuration::Configuration,
    diagnostics, settings,
};

pub(super) fn run() -> Result<()> {
    miette::set_hook(Box::new(|_| Box::new(diagnostics::handler()))).into_diagnostic()?;

    match Arguments::parse().command {
        Command::Apply { configuration } => {
            let mut configuration = Configuration::load(configuration)?;
            Catalog::load()?.resolve(&mut configuration)?;

            let directory = dirs::data_local_dir()
                .ok_or_else(|| miette!("Cannot locate Local AppData"))?
                .join("Roblox");

            let changed = settings::apply(&directory, configuration)?;
            println!("Updated {changed} settings file(s).");
        }

        Command::Catalog { filter } => catalog::print(&Catalog::load()?, filter.as_deref())?,

        Command::Schema => {
            let mut output = io::stdout().lock();

            serde_json::to_writer_pretty(&mut output, &Catalog::load()?.schema()?)
                .into_diagnostic()?;

            writeln!(output).into_diagnostic()?;
        }
    }

    Ok(())
}
