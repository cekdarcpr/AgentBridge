pub mod elevenlabs;
pub mod gemini;
mod http;
pub mod openai;
pub mod openrouter;

use std::time::Duration;

use reqwest::{
    blocking::{Client, RequestBuilder},
    redirect::Policy,
};
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
const MAX_REDIRECTS: usize = 5;
/// Short strings are not redacted: they could match ordinary words in an error message.
const MIN_REDACT_LEN: usize = 8;

/// Removes the API key from a message before it is shown or logged; some providers echo the key
/// they rejected.
pub fn redact(message: &str, secret: &str) -> String {
    if secret.len() < MIN_REDACT_LEN {
        return message.to_string();
    }
    message.replace(secret, "[redacted]")
}

/// The client every provider request goes through. Redirects are only followed within the same
/// origin: reqwest drops `Authorization` on a cross-origin redirect, but not custom key headers
/// such as Gemini's `x-goog-api-key` or ElevenLabs' `xi-api-key`.
pub fn client(timeout: Duration) -> Result<Client, String> {
    let same_origin = Policy::custom(|attempt| {
        let origin = attempt.previous().first().map(|first| first.origin());
        if attempt.previous().len() > MAX_REDIRECTS || origin != Some(attempt.url().origin()) {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    Client::builder()
        .timeout(timeout)
        .user_agent(concat!("agentbridge/", env!("CARGO_PKG_VERSION")))
        .redirect(same_origin)
        .build()
        .map_err(|e| format!("could not build HTTP client: {e}"))
}

/// Checks a key without generating anything, so it costs nothing.
pub fn verify_key(id: ProviderId, api_key: &str) -> Result<(), String> {
    let client = client(Duration::from_secs(VERIFY_TIMEOUT_SECS))?;
    let response = get(id)
        .verify_request(&client, api_key)
        .send()
        .map_err(|e| redact(&http::network_error(e), api_key))?;
    http::ensure_success(response)
        .map(|_| ())
        .map_err(|e| redact(&e, api_key))
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

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    /// Answers one request with a redirect to `location` and returns the server's address.
    fn redirect_to(location: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0; 4096]);
            let reply = format!(
                "HTTP/1.1 302 Found
Location: {location}
Content-Length: 0
Connection: close

"
            );
            stream.write_all(reply.as_bytes()).unwrap();
        });
        address
    }

    #[test]
    fn keys_are_not_sent_to_another_origin() {
        // Nothing listens on the target: following the redirect would fail to connect.
        let elsewhere = TcpListener::bind("127.0.0.1:0").unwrap();
        let target = format!("http://{}/steal", elsewhere.local_addr().unwrap());
        drop(elsewhere);
        let response = client(Duration::from_secs(5))
            .unwrap()
            .get(redirect_to(target))
            .header("x-goog-api-key", "secret-key-123")
            .send()
            .unwrap();
        assert_eq!(response.status().as_u16(), 302);
    }

    #[test]
    fn redacts_keys_from_messages() {
        assert_eq!(
            redact("bad key sk-12345678", "sk-12345678"),
            "bad key [redacted]"
        );
        assert_eq!(redact("short", "abc"), "short");
    }
}
