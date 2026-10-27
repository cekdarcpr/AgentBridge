use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Budget(String),
    #[error("{0}")]
    NoProvider(String),
    #[error("{0}")]
    Provider(String),
    #[error("{0}")]
    Output(String),
    #[error("{0}")]
    Keychain(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    /// Stable identifier for `--json` output; agents branch on this, not on the message.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Usage(_) => "usage",
            Self::Config(_) => "config_error",
            Self::Budget(_) => "budget_exceeded",
            Self::NoProvider(_) => "no_provider",
            Self::Provider(_) => "provider_error",
            Self::Output(_) => "output_error",
            Self::Keychain(_) => "keychain_error",
            Self::Db(_) => "database_error",
            Self::Io(_) => "io_error",
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) | Self::Config(_) => 2,
            Self::Budget(_) => 3,
            Self::NoProvider(_) => 4,
            Self::Provider(_) => 5,
            Self::Output(_) => 6,
            _ => 1,
        }
    }
}
