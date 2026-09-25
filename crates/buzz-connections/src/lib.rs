//! Device-local connection contracts shared by Desktop and the SSH host.
//! No persona or relay publication contains these machine-specific settings.
#![forbid(unsafe_code)]

pub mod model;
pub mod process;
pub mod prompt;
pub mod registry;
pub mod ssh;
pub mod wire;
