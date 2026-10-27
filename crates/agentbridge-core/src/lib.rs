//! Core of agentBridge: everything except the user interface, so any front end
//! (the `agb` CLI first) shares one implementation of routing, budgets and storage.

pub mod audio;
pub mod capability;
pub mod config;
pub mod engine;
pub mod error;
pub mod icons;
pub mod imaging;
pub mod keystore;
pub mod paths;
pub mod pricing;
pub mod project;
pub mod providers;
pub mod request;
pub mod store;

pub use capability::{Capability, ProviderId};
pub use config::Config;
pub use engine::{Attempt, AttemptStatus, Engine, RunFailure, RunOptions, RunResult};
pub use error::AppError;
pub use imaging::Fit;
pub use paths::Paths;
pub use request::{
    parse_size, AudioFormat, AudioRequest, ImageFormat, ImageRequest, Request, SpeechRequest,
    TextRequest,
};
pub use store::Store;
