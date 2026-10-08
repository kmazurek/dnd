//! Native GUI backends.

use anyhow::Result;

use crate::config::Options;

/// Shows the window and runs the event loop until the user is done.
pub fn run(options: Options) -> Result<()> {
    anyhow::bail!(
        "no GUI backend is available for {} yet ({} item(s) requested)",
        std::env::consts::OS,
        options.items.len()
    )
}
