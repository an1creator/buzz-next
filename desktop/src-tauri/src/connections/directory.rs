//! Local directory resolution, reused by validation and process launch.
use buzz_connections::model::WorkingDirectory;
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
