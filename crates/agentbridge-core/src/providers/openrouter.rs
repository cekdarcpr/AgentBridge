use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};

use super::{
    gemini::nearest_aspect_ratio, http, unsupported, CallContext, Payload, Provider,
    ProviderOutput, Usage,
};
use crate::{
    capability::{Capability, ProviderId},
    request::{ImageRequest, Request, TextRequest},
};

const BASE_URL: &str = "https://openrouter.ai/api/v1";
/// Optional attribution headers OpenRouter uses to show which app sent the traffic.
const APP_URL: &str = "https://github.com/cekdarcpr/AgentBridge";
const APP_TITLE: &str = "agentBridge";
/// Prefix of the error http::ensure_success builds for a rejected request.
const BAD_REQUEST: &str = "HTTP 400:";

/// One key for hundreds of models, addressed as `vendor/model` (for example `google/gemini-2.5-flash`).
pub struct OpenRouter;

impl Provider for OpenRouter {
    fn id(&self) -> ProviderId {
        ProviderId::Openrouter
    }

    fn supports(&self, capability: Capability) -> bool {
        matches!(capability, Capability::Image | Capability::Text)
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
        http.get(format!("{BASE_URL}/key")).bearer_auth(api_key)
    }
}

fn chat(ctx: &CallContext, mut body: Value) -> Result<Value, String> {
    // Makes OpenRouter return the billed amount, so costs are exact instead of estimated.
    body["usage"] = json!({"include": true});
    let response = ctx
        .http
        .post(format!("{BASE_URL}/chat/completions"))
        .bearer_auth(ctx.api_key)
        .header("HTTP-Referer", APP_URL)
        .header("X-Title", APP_TITLE)
        .json(&body)
        .send()
        .map_err(http::network_error)?;
    let value = http::read_json(http::ensure_success(response)?)?;
    // OpenRouter can report upstream failures inside a 200 response.
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
        return Err(format!("upstream error: {message}"));
    }
    Ok(value)
}

fn usage(value: &Value) -> Option<Usage> {
    let usage = value.get("usage")?;
    Some(Usage {
        input_tokens: usage
            .get("prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: usage
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cost_usd: usage.get("cost").and_then(Value::as_f64),
    })
}

fn message_text(value: &Value) -> String {
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn image(ctx: &CallContext, r: &ImageRequest) -> Result<ProviderOutput, String> {
    let mut body = json!({"model": ctx.model, "prompt": r.prompt});
    // The closest aspect ratio keeps cropping to a minimum when `imaging` fits the exact size.
    if let Some((w, h)) = r.size {
        body["aspect_ratio"] = json!(nearest_aspect_ratio(w, h));
    }
    let mut warnings = Vec::new();
    let value = match images(ctx, &body) {
        // Not every model accepts `aspect_ratio` (HTTP 400); the image is fitted locally either way.
        Err(message) if message.starts_with(BAD_REQUEST) && body.get("aspect_ratio").is_some() => {
            body.as_object_mut()
                .expect("body is an object")
                .remove("aspect_ratio");
            warnings.push(format!(
                "`{}` rejected the aspect ratio ({message}); generated at its default shape",
                ctx.model
            ));
            images(ctx, &body)?
        }
        result => result?,
    };
    let data = decode_image(&value, ctx.model)?;
    // `media_type` is optional; the image decoder detects the format from the bytes anyway.
    let mime = value
        .pointer("/data/0/media_type")
        .and_then(Value::as_str)
        .unwrap_or("image/png")
        .to_string();
    Ok(ProviderOutput {
        payload: Payload::Bytes { data, mime },
        usage: usage(&value),
        warnings,
    })
}

/// The Image API serves every OpenRouter image model, including image-only ones like Qwen Image
/// that `chat/completions` rejects.
fn images(ctx: &CallContext, body: &Value) -> Result<Value, String> {
    let response = ctx
        .http
        .post(format!("{BASE_URL}/images"))
        .bearer_auth(ctx.api_key)
        .header("HTTP-Referer", APP_URL)
        .header("X-Title", APP_TITLE)
        .json(body)
        .send()
        .map_err(http::network_error)?;
    http::read_json(http::ensure_success(response)?)
}

fn decode_image(value: &Value, model: &str) -> Result<Vec<u8>, String> {
    let encoded = value
        .pointer("/data/0/b64_json")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("response contained no image (is `{model}` an image model?)"))?;
    STANDARD
        .decode(encoded)
        .map_err(|e| format!("invalid base64 image data: {e}"))
}
fn text(ctx: &CallContext, r: &TextRequest) -> Result<ProviderOutput, String> {
    let mut messages = Vec::new();
    if let Some(system) = &r.system {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.push(json!({"role": "user", "content": r.prompt}));
    let value = chat(ctx, json!({"model": ctx.model, "messages": messages}))?;
    let content = message_text(&value);
    if content.is_empty() {
        return Err("response contained no text".into());
    }
    Ok(ProviderOutput {
        payload: Payload::Text(content),
        usage: usage(&value),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_billed_cost() {
        let value =
            json!({"usage": {"prompt_tokens": 12, "completion_tokens": 1290, "cost": 0.0391}});
        let usage = usage(&value).unwrap();
        assert_eq!(usage.cost_usd, Some(0.0391));
        assert_eq!(usage.output_tokens, 1290);
    }

    #[test]
    fn decodes_image_api_responses() {
        let value = json!({"created": 1, "data": [{"b64_json": "aGk="}], "usage": {"cost": 0.003}});
        assert_eq!(
            decode_image(&value, "qwen/qwen-image-2.1-turbo").unwrap(),
            b"hi"
        );
        assert!(decode_image(&json!({"data": []}), "x/y")
            .unwrap_err()
            .contains("x/y"));
        assert!(decode_image(&json!({"data": [{"b64_json": "%%"}]}), "x/y").is_err());
    }
}
