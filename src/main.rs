//! Apply configured Roblox client and Studio preferences.

mod application;
mod arguments;
mod catalog;
mod configuration;
mod desktop;
mod diagnostics;
mod export;
mod preferences;
mod settings;
mod xml;

use std::process::ExitCode;

fn main() -> ExitCode {
    match application::run() {
        Ok(()) => ExitCode::SUCCESS,

        Err(error) => {
            eprintln!("{error:?}");

            ExitCode::FAILURE
        }
    }
}
