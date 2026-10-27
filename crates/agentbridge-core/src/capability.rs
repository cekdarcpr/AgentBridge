use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Image,
    /// Sound effects.
    Audio,
    /// Text-to-speech.
    Speech,
    Text,
}

impl Capability {
    pub const ALL: [Capability; 4] = [Self::Image, Self::Audio, Self::Speech, Self::Text];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Speech => "speech",
            Self::Text => "text",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Openai,
    Gemini,
    Elevenlabs,
}

impl ProviderId {
    pub const ALL: [ProviderId; 3] = [Self::Openai, Self::Gemini, Self::Elevenlabs];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Gemini => "gemini",
            Self::Elevenlabs => "elevenlabs",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Openai => "OpenAI",
            Self::Gemini => "Google Gemini",
            Self::Elevenlabs => "ElevenLabs",
        }
    }

    /// The variable each provider's own SDKs read, so existing setups work unchanged.
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Openai => "OPENAI_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
            Self::Elevenlabs => "ELEVENLABS_API_KEY",
        }
    }

    /// Lets `--model` work without `--provider` for the common model families.
    pub fn infer_from_model(model: &str) -> Option<Self> {
        let model = model.to_ascii_lowercase();
        let openai_prefixes = ["gpt-", "o1", "o3", "o4", "tts-"];
        if openai_prefixes.iter().any(|p| model.starts_with(p)) {
            Some(Self::Openai)
        } else if model.starts_with("gemini-") {
            Some(Self::Gemini)
        } else if model.starts_with("eleven_") {
            Some(Self::Elevenlabs)
        } else {
            None
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProviderId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(Self::Openai),
            "gemini" | "google" => Ok(Self::Gemini),
            "elevenlabs" => Ok(Self::Elevenlabs),
            other => Err(format!(
                "unknown provider '{other}' (expected: openai, gemini, elevenlabs)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_provider_from_model_name() {
        assert_eq!(ProviderId::infer_from_model("gpt-image-1"), Some(ProviderId::Openai));
        assert_eq!(ProviderId::infer_from_model("Gemini-2.5-Pro"), Some(ProviderId::Gemini));
        assert_eq!(ProviderId::infer_from_model("eleven_v3"), Some(ProviderId::Elevenlabs));
        assert_eq!(ProviderId::infer_from_model("mystery-model"), None);
    }

    #[test]
    fn parses_provider_case_insensitively() {
        assert_eq!("OpenAI".parse::<ProviderId>(), Ok(ProviderId::Openai));
        assert_eq!("google".parse::<ProviderId>(), Ok(ProviderId::Gemini));
        assert!("nope".parse::<ProviderId>().is_err());
    }
}
