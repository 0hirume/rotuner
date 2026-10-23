use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about)]
pub(super) struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(super) enum Command {
    #[command(about = "Apply a TOML configuration while Roblox and Studio are closed")]
    Apply {
        #[arg(help = "Config file; omit to search local and user config directories")]
        configuration: Option<PathBuf>,
    },

    #[command(about = "Export current settings, client JSON, and Studio registry values as TOML")]
    Export {
        #[arg(value_enum, help = "Source to export; omit to export all sources")]
        source: Option<Source>,
    },

    #[command(about = "List supported settings and enum choices")]
    Catalog {
        #[arg(help = "Filter by configuration path or setting name")]
        filter: Option<String>,
    },

    #[command(about = "Print the configuration JSON Schema")]
    Schema,
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum Source {
    Settings,
    ClientStorage,
    StudioRegistry,
}
