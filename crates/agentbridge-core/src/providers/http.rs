use std::{thread, time::Duration};

use reqwest::{
    blocking::{RequestBuilder, Response},
    header::RETRY_AFTER,
    StatusCode,
};
use serde_json::Value;

const MAX_ERROR_CHARS: usize = 500;
/// The first try plus two retries; after that the engine falls back to the next provider.
const MAX_ATTEMPTS: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 1_000;
/// A longer wait is better spent on the next provider than on this one.
const MAX_RETRY_DELAY_SECS: u64 = 20;

/// Sends a request, retrying rate limits and temporary server errors with exponential backoff.
/// Timeouts are not retried: the provider may still finish, and bill for, the first request.
pub(crate) fn send(request: RequestBuilder) -> Result<Response, String> {
    let mut attempt = 1;
    loop {
        // Every body agentBridge sends is in memory, so cloning only fails for streaming bodies.
        let retry = (attempt < MAX_ATTEMPTS)
            .then(|| request.try_clone())
            .flatten();
        let Some(next) = retry else {
            return request.send().map_err(network_error);
        };
        let delay = match next.send() {
            Ok(response) if is_retryable(response.status()) => {
                let retry_after = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok());
                match retry_delay(attempt, retry_after) {
                    Some(delay) => delay,
                    None => return Ok(response),
                }
            }
            Ok(response) => return Ok(response),
            // The request never reached the server, so retrying cannot bill twice.
            Err(error) if error.is_connect() => backoff(attempt),
            Err(error) => return Err(network_error(error)),
        };
        thread::sleep(delay);
        attempt += 1;
    }
}

fn is_retryable(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

/// `None` when the server asks for a longer wait than is worth spending on one provider.
fn retry_delay(attempt: u32, retry_after: Option<&str>) -> Option<Duration> {
    let Some(seconds) = retry_after.and_then(|v| v.trim().parse::<u64>().ok()) else {
        return Some(backoff(attempt));
    };
    (seconds <= MAX_RETRY_DELAY_SECS).then(|| Duration::from_secs(seconds))
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(BASE_RETRY_DELAY_MS << (attempt - 1))
}

pub(crate) fn ensure_success(response: Response) -> Result<Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    Err(format!(
        "HTTP {}: {}",
        status.as_u16(),
        truncate(&error_message(&body), MAX_ERROR_CHARS)
    ))
}

pub(crate) fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "request timed out (raise http_timeout_secs in config if this keeps happening)".into()
    } else {
        format!("request failed: {error}")
    }
}

pub(crate) fn read_json(response: Response) -> Result<Value, String> {
    response
        .json::<Value>()
        .map_err(|e| format!("invalid JSON response: {e}"))
}

pub(crate) fn read_bytes(response: Response) -> Result<Vec<u8>, String> {
    response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("could not read response body: {e}"))
}

pub(crate) fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars).collect();
    cut.push('…');
    cut
}

/// Providers wrap errors differently (OpenAI/Gemini: `error.message`, ElevenLabs: `detail`).
fn error_message(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return body.trim().to_string();
    };
    let candidates = [
        value.pointer("/error/message"),
        value.pointer("/detail/message"),
        value.get("detail"),
        value.get("message"),
    ];
    let message = candidates
        .into_iter()
        .flatten()
        .find_map(|v| v.as_str().map(str::to_string));
    message.unwrap_or_else(|| body.trim().to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    use reqwest::blocking::Client;

    use super::*;

    /// Answers each connection with the next status in order and returns the address to call.
    fn serve(statuses: &'static [u16]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for status in statuses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let reply = format!(
                    "HTTP/1.1 {status} X
Retry-After: 0
Content-Length: 0
Connection: close

"
                );
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        address
    }

    #[test]
    fn retries_until_the_server_recovers() {
        let address = serve(&[503, 429, 200]);
        let response = send(Client::new().post(address).body("{}")).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn gives_up_after_the_last_attempt() {
        let address = serve(&[503, 503, 503]);
        let response = send(Client::new().post(address).body("{}")).unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn does_not_retry_client_errors() {
        let address = serve(&[400]);
        let response = send(Client::new().post(address).body("{}")).unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn extracts_nested_error_messages() {
        assert_eq!(
            error_message(r#"{"error":{"message":"bad key"}}"#),
            "bad key"
        );
        assert_eq!(error_message(r#"{"detail":{"message":"quota"}}"#), "quota");
        assert_eq!(error_message(r#"{"detail":"nope"}"#), "nope");
        assert_eq!(error_message("plain text"), "plain text");
    }

    #[test]
    fn retries_only_temporary_failures() {
        assert!(is_retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable(StatusCode::BAD_REQUEST));
        assert!(!is_retryable(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable(StatusCode::PAYMENT_REQUIRED));
    }

    #[test]
    fn backs_off_exponentially_or_as_told() {
        assert_eq!(retry_delay(1, None), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay(2, None), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(1, Some("5")), Some(Duration::from_secs(5)));
        assert_eq!(retry_delay(1, Some("120")), None);
        // An HTTP date is not worth parsing for this; the normal backoff applies.
        assert_eq!(
            retry_delay(1, Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            Some(Duration::from_secs(1))
        );
    }
}
