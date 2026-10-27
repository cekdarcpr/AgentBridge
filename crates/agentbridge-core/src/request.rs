use std::path::Path;

use crate::{capability::Capability, error::AppError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

impl ImageFormat {
    pub fn from_path(path: &Path) -> Result<Self, AppError> {
        match extension(path).as_deref() {
            Some("png") => Ok(Self::Png),
            Some("jpg" | "jpeg") => Ok(Self::Jpeg),
            Some("webp") => Ok(Self::Webp),
            _ => Err(AppError::Usage(format!(
                "unsupported image extension for '{}' (use .png, .jpg or .webp)",
                path.display()
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }

    pub fn supports_alpha(self) -> bool {
        !matches!(self, Self::Jpeg)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    Wav,
}

impl AudioFormat {
    pub fn from_path(path: &Path) -> Result<Self, AppError> {
        match extension(path).as_deref() {
            Some("mp3") => Ok(Self::Mp3),
            Some("wav") => Ok(Self::Wav),
            _ => Err(AppError::Usage(format!(
                "unsupported audio extension for '{}' (use .mp3 or .wav)",
                path.display()
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Wav => "audio/wav",
        }
    }
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

#[derive(Debug, Clone)]
pub struct ImageRequest {
    pub prompt: String,
    pub size: Option<(u32, u32)>,
    pub transparent: bool,
    pub quality: Option<String>,
    pub format: ImageFormat,
}

#[derive(Debug, Clone)]
pub struct AudioRequest {
    pub prompt: String,
    pub duration_secs: Option<f32>,
    pub format: AudioFormat,
}

#[derive(Debug, Clone)]
pub struct SpeechRequest {
    pub text: String,
    pub voice: Option<String>,
    pub format: AudioFormat,
}

#[derive(Debug, Clone)]
pub struct TextRequest {
    pub prompt: String,
    pub system: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Request {
    Image(ImageRequest),
    Audio(AudioRequest),
    Speech(SpeechRequest),
    Text(TextRequest),
}

impl Request {
    pub fn capability(&self) -> Capability {
        match self {
            Self::Image(_) => Capability::Image,
            Self::Audio(_) => Capability::Audio,
            Self::Speech(_) => Capability::Speech,
            Self::Text(_) => Capability::Text,
        }
    }

    pub fn prompt(&self) -> &str {
        match self {
            Self::Image(r) => &r.prompt,
            Self::Audio(r) => &r.prompt,
            Self::Speech(r) => &r.text,
            Self::Text(r) => &r.prompt,
        }
    }

    pub fn expected_mime(&self) -> Option<&'static str> {
        match self {
            Self::Image(r) => Some(r.format.mime()),
            Self::Audio(r) => Some(r.format.mime()),
            Self::Speech(r) => Some(r.format.mime()),
            Self::Text(_) => None,
        }
    }
}

/// Parses `WIDTHxHEIGHT`, e.g. `64x64`.
pub fn parse_size(value: &str) -> Result<(u32, u32), AppError> {
    let invalid = || AppError::Usage(format!("invalid size '{value}' (expected WIDTHxHEIGHT, e.g. 512x512)"));
    let lower = value.trim().to_ascii_lowercase();
    let (w, h) = lower.split_once('x').ok_or_else(invalid)?;
    let w: u32 = w.trim().parse().map_err(|_| invalid())?;
    let h: u32 = h.trim().parse().map_err(|_| invalid())?;
    if w == 0 || h == 0 {
        return Err(invalid());
    }
    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("64x64").unwrap(), (64, 64));
        assert_eq!(parse_size("1536X1024").unwrap(), (1536, 1024));
        assert!(parse_size("0x10").is_err());
        assert!(parse_size("big").is_err());
    }

    #[test]
    fn detects_formats_from_extension() {
        assert_eq!(ImageFormat::from_path(Path::new("a/b.JPG")).unwrap(), ImageFormat::Jpeg);
        assert_eq!(AudioFormat::from_path(Path::new("click.wav")).unwrap(), AudioFormat::Wav);
        assert!(ImageFormat::from_path(Path::new("x.gif")).is_err());
    }
}
