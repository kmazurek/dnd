//! Fallback for platforms without a backend yet (Linux is planned next).

use anyhow::{Result, bail};

use crate::config::Options;

pub fn run(_options: Options) -> Result<()> {
    bail!(
        "no GUI backend is available for {} yet",
        std::env::consts::OS
    )
}
