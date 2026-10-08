//! Native GUI backends.
//!
//! Each backend exposes `run(Options) -> Result<()>`, which shows the window
//! and blocks until the user is done.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::run;

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub use unsupported::run;
