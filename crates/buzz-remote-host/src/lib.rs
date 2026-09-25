//! One-shot SSH handoff; relay owns the running agent's conversation and control.
#![forbid(unsafe_code)]
#[cfg(unix)]
pub mod codex_connection;
pub mod config;
pub mod launch;
pub mod models;
pub mod setup;
#[cfg(unix)]
pub mod supervisor;
