use reqwest::blocking::Response;
use serde_json::Value;

const MAX_ERROR_CHARS: usize = 500;

pub(crate) fn ensure_success(response: Response) -> Result<Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    Err(format!("HTTP {}: {}", status.as_u16(), truncate(&error_message(&body), MAX_ERROR_CHARS)))
}

pub(crate) fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "request timed out (raise http_timeout_secs in config if this keeps happening)".into()
    } else {
        format!("request failed: {error}")
    }
}

pub(crate) fn read_json(response: Response) -> Result<Value, String> {
    response.json::<Value>().map_err(|e| format!("invalid JSON response: {e}"))
}

pub(crate) fn read_bytes(response: Response) -> Result<Vec<u8>, String> {
    response.bytes().map(|b| b.to_vec()).map_err(|e| format!("could not read response body: {e}"))
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
    let message = candidates.into_iter().flatten().find_map(|v| v.as_str().map(str::to_string));
    message.unwrap_or_else(|| body.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nested_error_messages() {
        assert_eq!(error_message(r#"{"error":{"message":"bad key"}}"#), "bad key");
        assert_eq!(error_message(r#"{"detail":{"message":"quota"}}"#), "quota");
        assert_eq!(error_message(r#"{"detail":"nope"}"#), "nope");
        assert_eq!(error_message("plain text"), "plain text");
    }
}
