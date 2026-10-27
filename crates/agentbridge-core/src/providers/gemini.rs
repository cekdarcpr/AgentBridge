use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};

use super::{http, unsupported, usage_from, CallContext, Payload, Provider, ProviderOutput};
use crate::{
    capability::{Capability, ProviderId},
    request::{ImageRequest, Request, TextRequest},
};

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const MAX_REFUSAL_CHARS: usize = 300;
/// Aspect ratios accepted by `generationConfig.imageConfig.aspectRatio`.
const ASPECT_RATIOS: [(&str, f64); 10] = [
    ("1:1", 1.0),
    ("2:3", 2.0 / 3.0),
    ("3:2", 3.0 / 2.0),
    ("3:4", 3.0 / 4.0),
    ("4:3", 4.0 / 3.0),
    ("4:5", 4.0 / 5.0),
    ("5:4", 5.0 / 4.0),
    ("9:16", 9.0 / 16.0),
    ("16:9", 16.0 / 9.0),
    ("21:9", 21.0 / 9.0),
];

pub struct Gemini;

impl Provider for Gemini {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn supports(&self, capability: Capability) -> bool {
        matches!(capability, Capability::Image | Capability::Text)
    }

    fn limitations(&self, request: &Request) -> Vec<String> {
        match request {
            Request::Image(r) if r.transparent => {
                vec!["Gemini image models do not produce transparent backgrounds; the image will be opaque".into()]
            }
            _ => Vec::new(),
        }
    }

    fn execute(&self, ctx: &CallContext, request: &Request) -> Result<ProviderOutput, String> {
        match request {
            Request::Image(r) => image(ctx, r),
            Request::Text(r) => text(ctx, r),
            Request::Audio(_) => Err(unsupported(self.id(), Capability::Audio)),
            Request::Speech(_) => Err(unsupported(self.id(), Capability::Speech)),
        }
    }

    fn verify_request(&self, http: &Client, api_key: &str) -> RequestBuilder {
        http.get(BASE_URL).header("x-goog-api-key", api_key)
    }
}

fn generate(ctx: &CallContext, body: &Value) -> Result<Value, String> {
    let response = ctx
        .http
        .post(format!("{BASE_URL}/{}:generateContent", ctx.model))
        .header("x-goog-api-key", ctx.api_key)
        .json(body)
        .send()
        .map_err(http::network_error)?;
    http::read_json(http::ensure_success(response)?)
}

fn parts(value: &Value) -> Result<&Vec<Value>, String> {
    if let Some(reason) = value.pointer("/promptFeedback/blockReason").and_then(Value::as_str) {
        return Err(format!("prompt blocked by provider: {reason}"));
    }
    value.pointer("/candidates/0/content/parts").and_then(Value::as_array).ok_or_else(|| {
        let finish = value.pointer("/candidates/0/finishReason").and_then(Value::as_str).unwrap_or("unknown");
        format!("response contained no content (finishReason: {finish})")
    })
}

fn joined_text(parts: &[Value]) -> String {
    parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect()
}

fn usage(value: &Value) -> Option<super::Usage> {
    usage_from(value, "/usageMetadata/promptTokenCount", "/usageMetadata/candidatesTokenCount")
}

pub(crate) fn nearest_aspect_ratio(width: u32, height: u32) -> &'static str {
    let target = f64::from(width) / f64::from(height);
    ASPECT_RATIOS
        .iter()
        .min_by(|a, b| (a.1 - target).abs().total_cmp(&(b.1 - target).abs()))
        .map_or("1:1", |(name, _)| name)
}

fn image(ctx: &CallContext, r: &ImageRequest) -> Result<ProviderOutput, String> {
    let mut warnings = Vec::new();
    let mut body = json!({
        "contents": [{"parts": [{"text": r.prompt}]}],
        "generationConfig": {"responseModalities": ["TEXT", "IMAGE"]},
    });
    if let Some((w, h)) = r.size {
        let ratio = nearest_aspect_ratio(w, h);
        body["generationConfig"]["imageConfig"] = json!({"aspectRatio": ratio});
        warnings.push(format!(
            "requested {w}x{h}; Gemini chooses the pixel size itself (aspect ratio {ratio}), automatic resizing is not supported yet"
        ));
    }

    let value = generate(ctx, &body)?;
    let parts = parts(&value)?;
    let inline = parts
        .iter()
        .find_map(|p| p.get("inlineData"))
        .ok_or_else(|| {
            let said = http::truncate(&joined_text(parts), MAX_REFUSAL_CHARS);
            format!("response contained no image; model said: {said}")
        })?;
    let mime = inline.get("mimeType").and_then(Value::as_str).unwrap_or("image/png").to_string();
    let encoded = inline
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| "image part had no data".to_string())?;
    let data = STANDARD.decode(encoded).map_err(|e| format!("invalid base64 image data: {e}"))?;

    Ok(ProviderOutput { payload: Payload::Bytes { data, mime }, usage: usage(&value), warnings })
}

fn text(ctx: &CallContext, r: &TextRequest) -> Result<ProviderOutput, String> {
    let mut body = json!({"contents": [{"role": "user", "parts": [{"text": r.prompt}]}]});
    if let Some(system) = &r.system {
        body["systemInstruction"] = json!({"parts": [{"text": system}]});
    }
    let value = generate(ctx, &body)?;
    let content = joined_text(parts(&value)?);
    if content.is_empty() {
        return Err("response contained no text".into());
    }
    Ok(ProviderOutput { payload: Payload::Text(content), usage: usage(&value), warnings: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_nearest_aspect_ratio() {
        assert_eq!(nearest_aspect_ratio(64, 64), "1:1");
        assert_eq!(nearest_aspect_ratio(1920, 1080), "16:9");
        assert_eq!(nearest_aspect_ratio(1080, 1920), "9:16");
        assert_eq!(nearest_aspect_ratio(800, 600), "4:3");
    }
}
