//! Bounded alias enumeration only. OpenSSH remains authoritative for effective config.
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

/// Enumerate concrete Host names without executing Match/ProxyCommand directives.
pub fn aliases(path: &Path, home: &Path) -> Result<Vec<String>, String> {
    let mut visited = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut bytes = 0;
    collect(path, home, &mut visited, &mut names, &mut bytes, 0)?;
    Ok(names.into_iter().collect())
}
fn collect(
    path: &Path,
    home: &Path,
    visited: &mut BTreeSet<PathBuf>,
    names: &mut BTreeSet<String>,
    bytes: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > 8 || visited.len() >= 64 {
        return Err("SSH config includes exceed the supported discovery limit".into());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "Cannot read SSH config. Choose another file.".to_string())?;
    if !visited.insert(path.clone()) {
        return Ok(());
    }
    let mut content = String::new();
    std::fs::File::open(&path)
        .map_err(|_| "Cannot open SSH config".to_string())?
        .take(1_048_577)
        .read_to_string(&mut content)
        .map_err(|_| "Cannot read SSH config as text".to_string())?;
    *bytes += content.len();
    if *bytes > 1_048_576 {
        return Err("SSH config exceeds its discovery size limit".into());
    }
    for line in content.lines() {
        let normalized = line.trim_start();
        // OpenSSH accepts both `Host alias` and `Host=alias`.
        let normalized = match normalized.split_once('=') {
            Some((key, tail)) if !key.contains(char::is_whitespace) => format!("{key} {tail}"),
            _ => normalized.to_string(),
        };
        let Some(tokens) = shlex::split(&normalized) else {
            continue;
        };
        let Some(directive) = tokens.first() else {
            continue;
        };
        if directive.eq_ignore_ascii_case("host") {
            for name in &tokens[1..] {
                if !name.is_empty()
                    && !name.starts_with('-')
                    && !name.contains(['*', '?', '!', '/', '\\', '@'])
                    && !name.contains(char::is_whitespace)
                {
                    names.insert(name.clone());
                    if names.len() > 1024 {
                        return Err("Too many SSH aliases".into());
                    }
                }
            }
        } else if directive.eq_ignore_ascii_case("include") {
            for pattern in &tokens[1..] {
                let pattern = if let Some(relative) = pattern.strip_prefix("~/") {
                    home.join(relative)
                } else if Path::new(pattern).is_absolute() {
                    PathBuf::from(pattern)
                } else {
                    home.join(".ssh").join(pattern)
                };
                let pattern = pattern.to_str().ok_or("SSH include path is not UTF-8")?;
                for entry in glob::glob(pattern).map_err(|_| "Invalid SSH include pattern")? {
                    let entry = entry.map_err(|_| "Cannot read SSH include")?;
                    collect(&entry, home, visited, names, bytes, depth + 1)?;
                }
            }
        }
    }
    Ok(())
}
