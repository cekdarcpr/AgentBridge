use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::error::AppError;

pub const PROJECT_ENV: &str = "AGENTBRIDGE_PROJECT";

/// Identifies the project for per-project budgets: explicit value, then env, then the git root, then cwd.
pub fn resolve(explicit: Option<&str>) -> String {
    if let Some(name) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return name.to_string();
    }
    if let Ok(name) = env::var(PROJECT_ENV) {
        if !name.trim().is_empty() {
            return name.trim().to_string();
        }
    }
    display_path(&root())
}

/// The git repository containing the current directory, or the current directory outside git.
pub fn root() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map_or_else(|| cwd.clone(), Path::to_path_buf)
}

/// Makes `path` absolute with consistent separators and refuses to replace an existing file
/// unless `overwrite` is set, so a generated asset never silently destroys one.
pub fn resolve_output(path: &Path, overwrite: bool) -> Result<PathBuf, AppError> {
    let normalized = absolute(path)?;
    if normalized.exists() && !overwrite {
        return Err(AppError::Output(format!(
            "{} already exists; pass --overwrite to replace it",
            display_path(&normalized)
        )));
    }
    Ok(normalized)
}

/// Absolute path with the platform's separators, e.g. `assets/a.png` becomes `C:\proj\assets\a.png`.
pub fn absolute(path: &Path) -> Result<PathBuf, AppError> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    Ok(joined.components().collect())
}

pub fn write_file(path: &Path, data: &[u8]) -> Result<(), AppError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, data)
        .map_err(|e| AppError::Output(format!("could not write {}: {e}", display_path(path))))
}

/// Drops the `\\?\` verbatim prefix Windows adds to canonical paths; agents and users never expect it.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}
