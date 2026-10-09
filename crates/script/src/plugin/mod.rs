//! Where plugins come from and which of them run: git and directory sources,
//! installing, updating and removing plugins, and the settings they declare.

pub mod catalog;
pub mod git;
pub mod manager;
pub mod manifest;
pub mod materialize;
pub mod paths;
pub mod registry;
pub mod settings;
pub mod worker;
