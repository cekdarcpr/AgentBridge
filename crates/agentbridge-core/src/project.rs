use std::{
    env,
    path::{Path, PathBuf},
};

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
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root = cwd
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(&cwd);
    display_path(root)
}

/// Drops the `\\?\` verbatim prefix Windows adds to canonical paths; agents and users never expect it.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}
