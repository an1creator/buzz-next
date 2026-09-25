//! Device-local connection contracts shared by Desktop and the SSH host.
//! No persona or relay publication contains these machine-specific settings.
#![forbid(unsafe_code)]

pub mod catalog;
pub mod model;
pub mod process;
pub mod prompt;
pub mod registry;
pub mod remote;
pub mod ssh;
pub mod ssh_config;
pub mod wire;
