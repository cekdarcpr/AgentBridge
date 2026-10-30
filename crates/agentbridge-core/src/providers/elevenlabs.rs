use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};

use super::{http, unsupported, CallContext, Payload, Provider, ProviderOutput};
use crate::{
    audio,
    capability::{Capability, ProviderId},
    request::{AudioFormat, AudioRequest, Request, SpeechRequest},
};

const BASE_URL: &str = "https://api.elevenlabs.io/v1";
const MP3_OUTPUT_FORMAT: &str = "mp3_44100_128";

pub struct ElevenLabs;

impl Provider for ElevenLabs {
    fn id(&self) -> ProviderId {
        ProviderId::Elevenlabs
    }

    fn supports(&self, capability: Capability) -> bool {
        matches!(capability, Capability::Audio | Capability::Speech)
    }

    fn execute(&self, ctx: &CallContext, request: &Request) -> Result<ProviderOutput, String> {
        match request {
            Request::Audio(r) => sound_effect(ctx, r),
            Request::Speech(r) => speech(ctx, r),
            Request::Image(_) => Err(unsupported(self.id(), Capability::Image)),
            Request::Text(_) => Err(unsupported(self.id(), Capability::Text)),
        }
    }

    fn verify_request(&self, http: &Client, api_key: &str) -> RequestBuilder {
        http.get(format!("{BASE_URL}/models"))
            .header("xi-api-key", api_key)
    }
}

fn sound_effect(ctx: &CallContext, r: &AudioRequest) -> Result<ProviderOutput, String> {
    let mut body = json!({"text": r.prompt});
    if let Some(duration) = r.duration_secs {
        body["duration_seconds"] = json!(duration);
    }
    post_audio(ctx, "/sound-generation", &body, r.format)
}

fn speech(ctx: &CallContext, r: &SpeechRequest) -> Result<ProviderOutput, String> {
    let voice = r.voice.as_deref().unwrap_or(&ctx.models.elevenlabs_voice);
    // The voice id becomes a URL path segment.
    if voice.is_empty() || !voice.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("invalid ElevenLabs voice id '{voice}'"));
    }
    let body = json!({"text": r.text, "model_id": ctx.model});
    post_audio(ctx, &format!("/text-to-speech/{voice}"), &body, r.format)
}

fn post_audio(
    ctx: &CallContext,
    path: &str,
    body: &Value,
    format: AudioFormat,
) -> Result<ProviderOutput, String> {
    let sample_rate = ctx.models.elevenlabs_wav_sample_rate;
    // ElevenLabs has no WAV output; raw PCM is requested and wrapped locally.
    let output_format = match format {
        AudioFormat::Mp3 => MP3_OUTPUT_FORMAT.to_string(),
        AudioFormat::Wav => format!("pcm_{sample_rate}"),
    };
    let response = http::send(
        ctx.http
            .post(format!("{BASE_URL}{path}?output_format={output_format}"))
            .header("xi-api-key", ctx.api_key)
            .json(body),
    )?;
    let bytes = http::read_bytes(http::ensure_success(response)?)?;
    let data = match format {
        AudioFormat::Mp3 => bytes,
        AudioFormat::Wav => audio::pcm16_mono_to_wav(&bytes, sample_rate),
    };
    Ok(ProviderOutput {
        payload: Payload::Bytes {
            data,
            mime: format.mime().into(),
        },
        usage: None,
        warnings: Vec::new(),
    })
}
