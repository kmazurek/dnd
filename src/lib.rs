//! Drag and drop files from and to the terminal.
//!
//! The crate is split into a platform independent core (argument parsing,
//! path handling, output) and per-platform GUI backends in [`platform`].

pub mod cli;
pub mod config;
pub mod items;
pub mod output;
pub mod platform;
pub mod uri;
