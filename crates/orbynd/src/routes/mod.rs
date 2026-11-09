//! The HTTP routes, one module per area.

pub mod mcp_apps;
pub mod media;
pub mod preview;
pub mod pulls;
pub mod raw_files;
pub mod renders;
pub mod stream;
pub mod terminal;
pub mod ui;

#[cfg(test)]
mod tests;
