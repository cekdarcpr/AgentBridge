use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use toml::{Table, Value};

use crate::{
    capability::{Capability, ProviderId},
    error::AppError,
    pricing::PriceEntry,
    providers,
};

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 180;
/// The tables `agb config set` can change; `[pricing]` is keyed by free-form model ids instead.
const SETTABLE_SECTIONS: [&str; 3] = ["routing", "limits", "models"];

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
        Self::parse(&fs::read_to_string(path)?, path)
    }

    fn parse(text: &str, path: &Path) -> Result<Self, AppError> {
        let config: Self = toml::from_str(text)
            .map_err(|e| AppError::Config(format!("invalid config {}: {e}", path.display())))?;
        config.checked()
    }

    fn checked(mut self) -> Result<Self, AppError> {
        self.models.replace_retired();
        self.validate()?;
        Ok(self)
    }

    /// Changes one setting, such as `routing.image` or `limits.daily_usd`, and returns the result.
    /// Only that setting is written: whatever else the file leaves out keeps following the defaults.
    pub fn set_value(path: &Path, key: &str, value: &str) -> Result<Self, AppError> {
        let mut file: Table = if path.exists() {
            toml::from_str(&fs::read_to_string(path)?)
                .map_err(|e| AppError::Config(format!("invalid config {}: {e}", path.display())))?
        } else {
            Table::new()
        };
        let defaults = Table::try_from(Self::default())
            .map_err(|e| AppError::Internal(format!("could not serialize config: {e}")))?;
        let (section, field) = match key.split_once('.') {
            Some((section, field)) => (Some(section), field),
            None => (None, key),
        };
        let current = match section {
            Some(section) if SETTABLE_SECTIONS.contains(&section) => {
                defaults.get(section).and_then(|table| table.get(field))
            }
            None => defaults.get(field).filter(|v| !v.is_table()),
            Some(_) => None,
        }
        .ok_or_else(|| {
            AppError::Usage(format!(
                "unknown setting `{key}`; run `agb config show` to see the names"
            ))
        })?;
        let parsed = parse_like(current, value)?;
        let target = match section {
            Some(section) => file
                .entry(section)
                .or_insert_with(|| Value::Table(Table::new()))
                .as_table_mut()
                .ok_or_else(|| {
                    AppError::Config(format!("`{section}` in the config is not a table"))
                })?,
            None => &mut file,
        };
        target.insert(field.to_string(), parsed);
        let text = toml::to_string_pretty(&file)
            .map_err(|e| AppError::Internal(format!("could not serialize config: {e}")))?;
        let config = toml::from_str::<Self>(&text)
            .map_err(|e| AppError::Config(format!("invalid value for `{key}`: {}", e.message())))?
            .checked()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, text)?;
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

/// Reads a command-line value as the type the setting already has; lists are comma separated.
fn parse_like(current: &Value, raw: &str) -> Result<Value, AppError> {
    let raw = raw.trim();
    let expected = |kind: &str| AppError::Usage(format!("expected {kind}, got `{raw}`"));
    match current {
        Value::Array(_) => Ok(Value::Array(
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.to_string()))
                .collect(),
        )),
        Value::Float(_) => raw
            .parse()
            .map(Value::Float)
            .map_err(|_| expected("a number")),
        Value::Integer(_) => raw
            .parse()
            .map(Value::Integer)
            .map_err(|_| expected("a whole number")),
        _ => Ok(Value::String(raw.to_string())),
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
    fn set_value_writes_only_that_setting() {
        let dir = std::env::temp_dir().join(format!("agb-config-set-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);

        let config = Config::set_value(&path, "routing.image", "openrouter, gemini").unwrap();
        assert_eq!(
            config.routing.image,
            vec![ProviderId::Openrouter, ProviderId::Gemini]
        );
        let config = Config::set_value(&path, "limits.daily_usd", "3").unwrap();
        assert_eq!(config.limits.daily_usd, 3.0);
        assert_eq!(config.routing.image[0], ProviderId::Openrouter);
        Config::set_value(&path, "http_timeout_secs", "60").unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(
            !written.contains("[models]"),
            "defaults must not be pinned:
{written}"
        );

        let unchanged = written.clone();
        for (key, value) in [
            ("routing.audio", "openai"),
            ("routing.image", "openai,openai"),
            ("routing.image", "dall-e"),
            ("limits.daily_usd", "a lot"),
            ("limits.weekly_usd", "1"),
            ("pricing.x", "1"),
            ("routing", "openai"),
        ] {
            assert!(
                Config::set_value(&path, key, value).is_err(),
                "{key} = {value}"
            );
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), unchanged);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let parsed: Config = toml::from_str("[limits]\ndaily_usd = 1.5\n").unwrap();
        assert_eq!(parsed.limits.daily_usd, 1.5);
        assert_eq!(parsed.limits.max_call_usd, Limits::default().max_call_usd);
        assert_eq!(parsed.routing.audio, vec![ProviderId::Elevenlabs]);
    }
}
