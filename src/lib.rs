//! Drag files from the terminal into other applications.
//!
//! The crate is split into a platform independent core (argument parsing,
//! path handling) and per-platform GUI backends in [`platform`].

pub mod cli;
pub mod config;
pub mod items;
pub mod platform;
