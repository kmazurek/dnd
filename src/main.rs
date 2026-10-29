use std::io;
use std::process::ExitCode;

use clap::Parser;

use dnd::cli::Cli;
use dnd::config::Options;
use dnd::config_file::{self, ConfigFile};

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("dnd: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let file = match config_file::path() {
        Some(path) => ConfigFile::load(&path)?,
        None => ConfigFile::default(),
    };
    let options = Options::from_cli(cli, file, io::stdin().lock(), &cwd)?;
    if options.verbose {
        eprintln!("dnd: {options:#?}");
    }
    dnd::platform::run(options)
}
