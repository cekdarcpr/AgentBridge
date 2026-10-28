use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    capability::{Capability, ProviderId},
    error::AppError,
    pricing::PriceEntry,
    providers,
};

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 180;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub http_timeout_secs: u64,
    pub routing: Routing,
    pub limits: Limits,
    pub models: Models,
    /// Overrides the built-in price table, keyed by `provider/model`.
    pub pricing: BTreeMap<String, PriceEntry>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http_timeout_secs: DEFAULT_HTTP_TIMEOUT_SECS,
            routing: Routing::default(),
            limits: Limits::default(),
            models: Models::default(),
            pricing: BTreeMap::new(),
        }
    }
}

impl Config {
    /// A missing file means defaults; the file is only written on an explicit save such as `config init`.
    pub fn load(path: &Path) -> Result<Self, AppError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&text)
            .map_err(|e| AppError::Config(format!("invalid config {}: {e}", path.display())))?;
        config.models.replace_retired();
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        self.validate()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.to_toml()?)?;
        Ok(())
    }

    pub fn to_toml(&self) -> Result<String, AppError> {
        toml::to_string_pretty(self)
            .map_err(|e| AppError::Internal(format!("could not serialize config: {e}")))
    }

    fn validate(&self) -> Result<(), AppError> {
        let limits = [
            ("daily_usd", self.limits.daily_usd),
            ("project_daily_usd", self.limits.project_daily_usd),
            ("max_call_usd", self.limits.max_call_usd),
        ];
        for (name, value) in limits {
            if !value.is_finite() || value < 0.0 {
                return Err(AppError::Config(format!(
                    "limits.{name} must be a non-negative number"
                )));
            }
        }
        if self.http_timeout_secs == 0 {
            return Err(AppError::Config(
                "http_timeout_secs must be greater than zero".into(),
            ));
        }
        for capability in Capability::ALL {
            let order = self.routing.for_capability(capability);
            for (index, provider) in order.iter().enumerate() {
                if !providers::get(*provider).supports(capability) {
                    return Err(AppError::Config(format!(
                        "routing.{capability}: {provider} does not support {capability}"
                    )));
                }
                if order[..index].contains(provider) {
                    return Err(AppError::Config(format!(
                        "routing.{capability}: {provider} is listed twice"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Provider preference per capability; earlier entries are tried first.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Routing {
    pub image: Vec<ProviderId>,
    pub audio: Vec<ProviderId>,
    pub speech: Vec<ProviderId>,
    pub text: Vec<ProviderId>,
}

impl Default for Routing {
    fn default() -> Self {
        Self {
            image: vec![
                ProviderId::Openai,
                ProviderId::Gemini,
                ProviderId::Openrouter,
            ],
            audio: vec![ProviderId::Elevenlabs],
            speech: vec![
                ProviderId::Elevenlabs,
                ProviderId::Openai,
                ProviderId::Gemini,
                ProviderId::Openrouter,
            ],
            text: vec![
                ProviderId::Gemini,
                ProviderId::Openai,
                ProviderId::Openrouter,
            ],
        }
    }
}

impl Routing {
    pub fn for_capability(&self, capability: Capability) -> &[ProviderId] {
        match capability {
            Capability::Image => &self.image,
            Capability::Audio => &self.audio,
            Capability::Speech => &self.speech,
            Capability::Text => &self.text,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub daily_usd: f64,
    pub project_daily_usd: f64,
    pub max_call_usd: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            daily_usd: 5.0,
            project_daily_usd: 2.0,
            max_call_usd: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Models {
    pub openai_image: String,
    pub openai_text: String,
    pub openai_speech: String,
    pub openai_voice: String,
    pub gemini_image: String,
    pub gemini_text: String,
    pub elevenlabs_speech: String,
    pub elevenlabs_voice: String,
    /// `pcm_*` rate requested for `.wav` output; higher rates may need a paid ElevenLabs tier.
    pub elevenlabs_wav_sample_rate: u32,
    pub gemini_speech: String,
    pub gemini_voice: String,
    pub openrouter_image: String,
    pub openrouter_text: String,
    pub openrouter_speech: String,
    /// Voices belong to the model behind OpenRouter, e.g. `Kore` for Gemini TTS, `alloy` for OpenAI.
    pub openrouter_voice: String,
}

/// Models their provider has shut down, with the replacement. A config saved while one was the
/// default would otherwise fail every call after the shutdown date.
const RETIRED_MODELS: [(&str, &str); 2] = [
    ("gpt-image-1", "gpt-image-2.5-flare"),
    ("openai/gpt-image-1", "openai/gpt-image-2.5-flare"),
];

/// ElevenLabs' sound-effect endpoint has no model parameter; this name only labels logs and prices.
pub const ELEVENLABS_SFX_MODEL: &str = "sound-generation";

impl Default for Models {
    fn default() -> Self {
        Self {
            openai_image: "gpt-image-2.5-flare".into(),
            openai_text: "gpt-4.1-mini".into(),
            openai_speech: "gpt-4o-mini-tts".into(),
            openai_voice: "alloy".into(),
            gemini_image: "gemini-2.5-flash-image".into(),
            gemini_text: "gemini-2.5-flash".into(),
            elevenlabs_speech: "eleven_multilingual_v2".into(),
            elevenlabs_voice: "21m00Tcm4TlvDq8ikWAM".into(),
            elevenlabs_wav_sample_rate: 44_100,
            gemini_speech: "gemini-3.8-flash-tts".into(),
            gemini_voice: "Kore".into(),
            openrouter_image: "google/gemini-2.5-flash-image".into(),
            openrouter_text: "google/gemini-2.5-flash".into(),
            openrouter_speech: "google/gemini-3.8-flash-tts".into(),
            openrouter_voice: "Kore".into(),
        }
    }
}

impl Models {
    pub fn default_for(&self, provider: ProviderId, capability: Capability) -> Option<String> {
        let model = match (provider, capability) {
            (ProviderId::Openai, Capability::Image) => &self.openai_image,
            (ProviderId::Openai, Capability::Text) => &self.openai_text,
            (ProviderId::Openai, Capability::Speech) => &self.openai_speech,
            (ProviderId::Gemini, Capability::Image) => &self.gemini_image,
            (ProviderId::Gemini, Capability::Text) => &self.gemini_text,
            (ProviderId::Gemini, Capability::Speech) => &self.gemini_speech,
            (ProviderId::Elevenlabs, Capability::Speech) => &self.elevenlabs_speech,
            (ProviderId::Openrouter, Capability::Image) => &self.openrouter_image,
            (ProviderId::Openrouter, Capability::Text) => &self.openrouter_text,
            (ProviderId::Openrouter, Capability::Speech) => &self.openrouter_speech,
            (ProviderId::Elevenlabs, Capability::Audio) => {
                return Some(ELEVENLABS_SFX_MODEL.into())
            }
            _ => return None,
        };
        Some(model.clone())
    }

    fn replace_retired(&mut self) {
        let fields = [
            &mut self.openai_image,
            &mut self.openai_text,
            &mut self.openai_speech,
            &mut self.gemini_image,
            &mut self.gemini_text,
            &mut self.gemini_speech,
            &mut self.openrouter_image,
            &mut self.openrouter_text,
            &mut self.openrouter_speech,
        ];
        for field in fields {
            if let Some((_, replacement)) = RETIRED_MODELS.iter().find(|(old, _)| old == field) {
                *field = (*replacement).to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips_through_toml() {
        let text = Config::default().to_toml().unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.routing.image, Routing::default().image);
        assert_eq!(parsed.models.openai_image, "gpt-image-2.5-flare");
    }

    #[test]
    fn replaces_retired_models() {
        let mut models = Models {
            openai_image: "gpt-image-1".into(),
            openrouter_image: "openai/gpt-image-1".into(),
            gemini_text: "my-custom-model".into(),
            ..Models::default()
        };
        models.replace_retired();
        assert_eq!(models.openai_image, "gpt-image-2.5-flare");
        assert_eq!(models.openrouter_image, "openai/gpt-image-2.5-flare");
        assert_eq!(models.gemini_text, "my-custom-model");
    }

    #[test]
    fn rejects_unsupported_or_duplicate_routing() {
        let mut config = Config::default();
        config.routing.audio = vec![ProviderId::Openai];
        assert_eq!(config.validate().unwrap_err().code(), "config_error");

        let mut config = Config::default();
        config.routing.image = vec![ProviderId::Openai, ProviderId::Openai];
        assert!(config.validate().is_err());
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let parsed: Config = toml::from_str("[limits]\ndaily_usd = 1.5\n").unwrap();
        assert_eq!(parsed.limits.daily_usd, 1.5);
        assert_eq!(parsed.limits.max_call_usd, Limits::default().max_call_usd);
        assert_eq!(parsed.routing.audio, vec![ProviderId::Elevenlabs]);
    }
}
