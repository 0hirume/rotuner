use std::path::PathBuf;

use clap::{Parser, Subcommand};

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

    #[command(about = "List persistent settings and enum choices from the Roblox Full API Dump")]
    Catalog {
        #[arg(help = "Filter by configuration path or setting name")]
        filter: Option<String>,
    },

    #[command(about = "Print the configuration JSON Schema")]
    Schema,
}
