//! One-shot SSH handoff; relay owns the running agent's conversation and control.
#![forbid(unsafe_code)]
pub mod config;
pub mod launch;
pub mod models;
#[cfg(unix)]
pub mod supervisor;
