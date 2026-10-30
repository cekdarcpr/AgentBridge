use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::{json, Value};

use super::{http, unsupported, usage_from, CallContext, Payload, Provider, ProviderOutput};
use crate::{
    capability::{Capability, ProviderId},
    request::{ImageRequest, Request, SpeechRequest, TextRequest},
};

const BASE_URL: &str = "https://api.openai.com/v1";
const SQUARE: (u32, u32) = (1024, 1024);
const LANDSCAPE: (u32, u32) = (1536, 1024);
const PORTRAIT: (u32, u32) = (1024, 1536);
/// Aspect ratios beyond these thresholds map to the landscape/portrait sizes.
const LANDSCAPE_MIN_RATIO: f64 = 1.2;
const PORTRAIT_MAX_RATIO: f64 = 1.0 / 1.2;
const DEFAULT_QUALITY: &str = "medium";

pub struct OpenAi;

impl Provider for OpenAi {
    fn id(&self) -> ProviderId {
        ProviderId::Openai
    }

    fn supports(&self, capability: Capability) -> bool {
        matches!(
            capability,
            Capability::Image | Capability::Speech | Capability::Text
        )
    }

    fn native_transparency(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &CallContext, request: &Request) -> Result<ProviderOutput, String> {
        match request {
            Request::Image(r) => image(ctx, r),
            Request::Speech(r) => speech(ctx, r),
            Request::Text(r) => text(ctx, r),
            Request::Audio(_) => Err(unsupported(self.id(), Capability::Audio)),
        }
    }

    fn verify_request(&self, http: &Client, api_key: &str) -> RequestBuilder {
        http.get(format!("{BASE_URL}/models")).bearer_auth(api_key)
    }
}

fn post(ctx: &CallContext, path: &str, body: &Value) -> Result<Response, String> {
    let response = http::send(
        ctx.http
            .post(format!("{BASE_URL}{path}"))
            .bearer_auth(ctx.api_key)
            .json(body),
    )?;
    http::ensure_success(response)
}

fn post_multipart(
    ctx: &CallContext,
    path: &str,
    fields: &Value,
    png: &[u8],
) -> Result<Response, String> {
    let (boundary, body) = multipart_body(fields, png);
    let response = http::send(
        ctx.http
            .post(format!("{BASE_URL}{path}"))
            .bearer_auth(ctx.api_key)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body),
    )?;
    http::ensure_success(response)
}

/// Builds the form by hand: one PNG plus plain fields is all the edits endpoint needs, which does
/// not justify another HTTP feature or crate.
fn multipart_body(fields: &Value, png: &[u8]) -> (String, Vec<u8>) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let boundary = format!("agentbridge-{nanos:x}");
    let mut body = Vec::new();
    for (name, value) in fields.as_object().into_iter().flatten() {
        let text = value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_string);
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{text}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"image[]\"; filename=\"reference.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

pub(crate) fn pick_size(requested: Option<(u32, u32)>) -> (u32, u32) {
    let Some((w, h)) = requested else {
        return SQUARE;
    };
    let ratio = f64::from(w) / f64::from(h);
    if ratio >= LANDSCAPE_MIN_RATIO {
        LANDSCAPE
    } else if ratio <= PORTRAIT_MAX_RATIO {
        PORTRAIT
    } else {
        SQUARE
    }
}

/// Always asks for PNG: a lossless master that `imaging` resizes and converts to the requested format.
fn image(ctx: &CallContext, r: &ImageRequest) -> Result<ProviderOutput, String> {
    let (w, h) = pick_size(r.size);
    let mut body = json!({
        "model": ctx.model,
        "prompt": r.prompt,
        "n": 1,
        "size": format!("{w}x{h}"),
        "quality": r.quality.as_deref().unwrap_or(DEFAULT_QUALITY),
        "output_format": "png",
    });
    if r.transparent {
        body["background"] = json!("transparent");
    }

    let response = match &r.reference {
        // A reference image is only accepted by the edits endpoint, which takes multipart form data.
        Some(reference) => post_multipart(ctx, "/images/edits", &body, &reference.png)?,
        None => post(ctx, "/images/generations", &body)?,
    };
    let value = http::read_json(response)?;
    let encoded = value
        .pointer("/data/0/b64_json")
        .and_then(Value::as_str)
        .ok_or_else(|| "response contained no image data".to_string())?;
    let data = STANDARD
        .decode(encoded)
        .map_err(|e| format!("invalid base64 image data: {e}"))?;

    Ok(ProviderOutput {
        payload: Payload::Bytes {
            data,
            mime: "image/png".into(),
        },
        usage: usage_from(&value, "/usage/input_tokens", "/usage/output_tokens"),
        warnings: Vec::new(),
    })
}

fn speech(ctx: &CallContext, r: &SpeechRequest) -> Result<ProviderOutput, String> {
    let body = json!({
        "model": ctx.model,
        "input": r.text,
        "voice": r.voice.as_deref().unwrap_or(&ctx.models.openai_voice),
        "response_format": r.format.as_str(),
    });
    let data = http::read_bytes(post(ctx, "/audio/speech", &body)?)?;
    Ok(ProviderOutput {
        payload: Payload::Bytes {
            data,
            mime: r.format.mime().into(),
        },
        usage: None,
        warnings: Vec::new(),
    })
}

fn text(ctx: &CallContext, r: &TextRequest) -> Result<ProviderOutput, String> {
    let mut messages = Vec::new();
    if let Some(system) = &r.system {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.push(json!({"role": "user", "content": r.full_prompt()}));
    let body = json!({"model": ctx.model, "messages": messages});

    let value = http::read_json(post(ctx, "/chat/completions", &body)?)?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| "response contained no text".to_string())?;

    Ok(ProviderOutput {
        payload: Payload::Text(content.to_string()),
        usage: usage_from(&value, "/usage/prompt_tokens", "/usage/completion_tokens"),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_requested_sizes_to_supported_ones() {
        assert_eq!(pick_size(None), SQUARE);
        assert_eq!(pick_size(Some((64, 64))), SQUARE);
        assert_eq!(pick_size(Some((1920, 1080))), LANDSCAPE);
        assert_eq!(pick_size(Some((1080, 1920))), PORTRAIT);
    }

    #[test]
    fn builds_multipart_form() {
        let (boundary, body) = multipart_body(&json!({"prompt": "a cat", "n": 1}), b"PNGDATA");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"prompt\"\r\n\r\na cat\r\n"));
        assert!(text.contains("name=\"n\"\r\n\r\n1\r\n"));
        assert!(text
            .contains("filename=\"reference.png\"\r\nContent-Type: image/png\r\n\r\nPNGDATA\r\n"));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
    }
}
