//! Local directory resolution, reused by validation and process launch.
use buzz_connections::model::WorkingDirectory;

/// Shared spawn boundary, including lazy start and restore, requires explicit local assignment.
pub(crate) fn local_execution_directory(
    app: &tauri::AppHandle,
    execution: Option<&buzz_connections::model::Execution>,
) -> Result<String, String> {
    let execution =
        execution.ok_or("Choose a connection in Execution before starting this agent")?;
    let connection = super::get(app, &execution.connection_id)?;
    execution.validate(&connection)?;
    if !matches!(connection.target, buzz_connections::model::Target::Local) {
        return Err("This agent must be started on its SSH connection".into());
    }
    if execution.harness_id.as_deref().is_none_or(str::is_empty) {
        return Err("Choose a harness before starting this agent".into());
    }
    local_directory(&execution.directory)
}

pub(crate) fn local_directory(directory: &WorkingDirectory) -> Result<String, String> {
    let path = match directory {
        WorkingDirectory::Automatic => crate::managed_agents::default_agent_workdir()
            .ok_or("Automatic working directory is unavailable. Choose a folder.")?,
        WorkingDirectory::Explicit { path } => std::path::PathBuf::from(path),
    };
    if !path.is_absolute() {
        return Err("Working directory must be absolute".into());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "Working directory does not exist or is inaccessible")?;
    if !path.is_dir() {
        return Err("Working directory is not a folder".into());
    }
    std::fs::read_dir(&path).map_err(|_| "Working directory is not readable")?;
    path.into_os_string()
        .into_string()
        .map_err(|_| "Working directory must be valid Unicode".into())
}
