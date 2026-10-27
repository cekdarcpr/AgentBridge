pub mod elevenlabs;
pub mod gemini;
mod http;
pub mod openai;
pub mod openrouter;

use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder};
use serde_json::Value;

use crate::{
    capability::{Capability, ProviderId},
    config::Models,
    request::Request,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Amount the provider says it billed, when it reports one (OpenRouter does).
    pub cost_usd: Option<f64>,
}

#[derive(Debug)]
pub enum Payload {
    Bytes { data: Vec<u8>, mime: String },
    Text(String),
}

#[derive(Debug)]
pub struct ProviderOutput {
    pub payload: Payload,
    pub usage: Option<Usage>,
    pub warnings: Vec<String>,
}

pub struct CallContext<'a> {
    pub http: &'a Client,
    pub api_key: &'a str,
    pub model: &'a str,
    pub models: &'a Models,
}

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    fn supports(&self, capability: Capability) -> bool;

    /// Whether the model itself can return an image with an alpha channel. Other providers get
    /// transparency by drawing on a flat backdrop that is removed afterwards (see `imaging`).
    fn native_transparency(&self) -> bool {
        false
    }

    /// Errors are plain messages; the engine redacts the API key from them before logging.
    fn execute(&self, ctx: &CallContext, request: &Request) -> Result<ProviderOutput, String>;

    /// A free, read-only request that only succeeds with a valid key.
    fn verify_request(&self, http: &Client, api_key: &str) -> RequestBuilder;
}

const VERIFY_TIMEOUT_SECS: u64 = 20;

/// Checks a key without generating anything, so it costs nothing.
pub fn verify_key(id: ProviderId, api_key: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(VERIFY_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("could not build HTTP client: {e}"))?;
    let response = get(id)
        .verify_request(&client, api_key)
        .send()
        .map_err(http::network_error)?;
    http::ensure_success(response).map(|_| ())
}

pub fn get(id: ProviderId) -> &'static dyn Provider {
    match id {
        ProviderId::Openai => &openai::OpenAi,
        ProviderId::Gemini => &gemini::Gemini,
        ProviderId::Elevenlabs => &elevenlabs::ElevenLabs,
        ProviderId::Openrouter => &openrouter::OpenRouter,
    }
}

pub fn capabilities(id: ProviderId) -> Vec<Capability> {
    let provider = get(id);
    Capability::ALL
        .into_iter()
        .filter(|c| provider.supports(*c))
        .collect()
}

fn usage_from(value: &Value, input_pointer: &str, output_pointer: &str) -> Option<Usage> {
    Some(Usage {
        input_tokens: value.pointer(input_pointer)?.as_u64()?,
        output_tokens: value.pointer(output_pointer)?.as_u64()?,
        cost_usd: None,
    })
}

fn unsupported(provider: ProviderId, capability: Capability) -> String {
    format!("{provider} does not support {capability}")
}
