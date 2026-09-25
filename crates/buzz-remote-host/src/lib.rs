//! One-shot SSH handoff; relay owns the running agent's conversation and control.
#![forbid(unsafe_code)]
pub mod config;
pub mod launch;
#[cfg(unix)]
pub mod supervisor;
