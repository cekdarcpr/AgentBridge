use std::{env, path::PathBuf};

use directories::ProjectDirs;

use crate::error::AppError;

/// Overrides both locations; used by tests and portable installs.
pub const HOME_ENV: &str = "AGENTBRIDGE_HOME";

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_file: PathBuf,
    pub db_file: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self, AppError> {
        if let Some(home) = env::var_os(HOME_ENV).filter(|h| !h.is_empty()) {
            let home = PathBuf::from(home);
            return Ok(Self {
                config_file: home.join("config.toml"),
                db_file: home.join("agentbridge.db"),
            });
        }
        let dirs = ProjectDirs::from("", "", "agentbridge")
            .ok_or_else(|| AppError::Internal("could not determine the user's home directory".into()))?;
        Ok(Self {
            config_file: dirs.config_dir().join("config.toml"),
            db_file: dirs.data_local_dir().join("agentbridge.db"),
        })
    }
}
