//! Complete icon sets (every size and container a platform expects) from one square master image.

use std::{fmt, path::Path, str::FromStr};

use image::{
    codecs::ico::{IcoEncoder, IcoFrame},
    imageops::{self, FilterType},
    ExtendedColorType, Rgba, RgbaImage,
};
use serde::Serialize;

use crate::{
    error::AppError,
    imaging::{self, Fit},
    project,
    request::ImageFormat,
};

/// Size the master image is generated at; every preset size is derived from it.
pub const MASTER_SIZE: u32 = 1024;
const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

const WEB_PNGS: &[(&str, u32)] = &[
    ("favicon-16x16.png", 16),
    ("favicon-32x32.png", 32),
    ("apple-touch-icon.png", 180),
    ("android-chrome-192x192.png", 192),
    ("android-chrome-512x512.png", 512),
];
const WEB_ICO: (&str, &[u32]) = ("favicon.ico", &[16, 32, 48]);
const WEB_MANIFEST: &str = "site.webmanifest";
/// iOS shows transparent pixels in home-screen icons as black, so this one is flattened.
const OPAQUE_ICONS: &[&str] = &["apple-touch-icon.png"];

const APP_PNGS: &[(&str, u32)] = &[
    ("icon-32.png", 32),
    ("icon-64.png", 64),
    ("icon-128.png", 128),
    ("icon-256.png", 256),
    ("icon-512.png", 512),
    ("icon-1024.png", 1024),
];
const APP_ICO: (&str, &[u32]) = ("icon.ico", &[16, 24, 32, 48, 64, 128, 256]);

const WEB_HTML: &str = r#"<link rel="icon" href="/favicon.ico" sizes="48x48">
<link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png">
<link rel="icon" type="image/png" sizes="16x16" href="/favicon-16x16.png">
<link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png">
<link rel="manifest" href="/site.webmanifest">"#;

const WEB_MANIFEST_JSON: &str = r##"{
  "icons": [
    { "src": "/android-chrome-192x192.png", "sizes": "192x192", "type": "image/png" },
    { "src": "/android-chrome-512x512.png", "sizes": "512x512", "type": "image/png" }
  ],
  "theme_color": "#ffffff",
  "background_color": "#ffffff",
  "display": "standalone"
}
"##;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IconPreset {
    /// Favicons, Apple touch icon and web app manifest icons.
    Web,
    /// Desktop application icons: PNG sizes up to 1024 and a multi-size Windows `.ico`.
    App,
}

impl IconPreset {
    fn pngs(self) -> &'static [(&'static str, u32)] {
        match self {
            Self::Web => WEB_PNGS,
            Self::App => APP_PNGS,
        }
    }

    fn ico(self) -> (&'static str, &'static [u32]) {
        match self {
            Self::Web => WEB_ICO,
            Self::App => APP_ICO,
        }
    }

    pub fn file_names(self) -> Vec<&'static str> {
        let mut names: Vec<&str> = self.pngs().iter().map(|(name, _)| *name).collect();
        names.push(self.ico().0);
        if self == Self::Web {
            names.push(WEB_MANIFEST);
        }
        names
    }

    /// Largest size in the set; a smaller source has to be upscaled.
    fn largest(self) -> u32 {
        self.pngs()
            .iter()
            .map(|(_, size)| *size)
            .max()
            .unwrap_or(MASTER_SIZE)
    }
}

impl fmt::Display for IconPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Web => "web",
            Self::App => "app",
        })
    }
}

impl FromStr for IconPreset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "web" => Ok(Self::Web),
            "app" => Ok(Self::App),
            other => Err(format!(
                "unknown icon preset '{other}' (expected: web, app)"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IconSet {
    pub preset: IconPreset,
    pub out_dir: String,
    pub files: Vec<String>,
    /// Tags to paste into the page `<head>`, for the web preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    pub warnings: Vec<String>,
}

/// Fails before any work (or spending) when files would be replaced without `overwrite`.
pub fn check_destination(
    out_dir: &Path,
    preset: IconPreset,
    overwrite: bool,
) -> Result<(), AppError> {
    if overwrite {
        return Ok(());
    }
    let existing: Vec<&str> = preset
        .file_names()
        .into_iter()
        .filter(|name| out_dir.join(name).exists())
        .collect();
    if existing.is_empty() {
        return Ok(());
    }
    Err(AppError::Output(format!(
        "{} already contains {}; pass --overwrite to replace them",
        project::display_path(out_dir),
        existing.join(", ")
    )))
}

pub fn build(source: &[u8], out_dir: &Path, preset: IconPreset) -> Result<IconSet, AppError> {
    let mut warnings = Vec::new();
    let master = square(&imaging::decode(source)?);
    if master.width() < preset.largest() {
        warnings.push(format!(
            "source is {0}x{0}; icons up to {1}x{1} were upscaled and may look soft",
            master.width(),
            preset.largest()
        ));
    }

    let mut files = Vec::new();
    for &(name, size) in preset.pngs() {
        let mut icon = resize(&master, size);
        if OPAQUE_ICONS.contains(&name) {
            icon = on_white(&icon);
        }
        let path = out_dir.join(name);
        project::write_file(&path, &imaging::encode(&icon, ImageFormat::Png)?)?;
        files.push(project::display_path(&path));
    }

    let (ico_name, ico_sizes) = preset.ico();
    let ico_path = out_dir.join(ico_name);
    project::write_file(&ico_path, &ico(&master, ico_sizes)?)?;
    files.push(project::display_path(&ico_path));

    let html = (preset == IconPreset::Web).then(|| WEB_HTML.to_string());
    if preset == IconPreset::Web {
        let manifest = out_dir.join(WEB_MANIFEST);
        project::write_file(&manifest, WEB_MANIFEST_JSON.as_bytes())?;
        files.push(project::display_path(&manifest));
    }

    Ok(IconSet {
        preset,
        out_dir: project::display_path(out_dir),
        files,
        html,
        warnings,
    })
}

/// Pads a non-square source onto a transparent square so nothing gets cropped.
fn square(img: &RgbaImage) -> RgbaImage {
    let (width, height) = img.dimensions();
    if width == height {
        return img.clone();
    }
    let side = width.max(height);
    imaging::fit(img, side, side, Fit::Contain, true)
}

fn resize(img: &RgbaImage, size: u32) -> RgbaImage {
    if img.width() == size {
        return img.clone();
    }
    imageops::resize(img, size, size, FilterType::Lanczos3)
}

fn on_white(img: &RgbaImage) -> RgbaImage {
    let flat = imaging::flatten(img, WHITE);
    RgbaImage::from_fn(flat.width(), flat.height(), |x, y| {
        let [r, g, b] = flat.get_pixel(x, y).0;
        Rgba([r, g, b, 255])
    })
}

fn ico(master: &RgbaImage, sizes: &[u32]) -> Result<Vec<u8>, AppError> {
    let to_error =
        |e: image::ImageError| AppError::Output(format!("could not encode the .ico file: {e}"));
    let images: Vec<RgbaImage> = sizes.iter().map(|&size| resize(master, size)).collect();
    let frames = images
        .iter()
        .map(|img| {
            IcoFrame::as_png(
                img.as_raw(),
                img.width(),
                img.height(),
                ExtendedColorType::Rgba8,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_error)?;
    let mut out = Vec::new();
    IcoEncoder::new(&mut out)
        .encode_images(&frames)
        .map_err(to_error)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agb-icons-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn source(width: u32, height: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(width, height, |x, _| {
            if x < width / 2 {
                Rgba([200, 30, 30, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        imaging::encode(&img, ImageFormat::Png).unwrap()
    }

    #[test]
    fn builds_the_web_preset() {
        let dir = temp_dir("web");
        let set = build(&source(512, 256), &dir, IconPreset::Web).unwrap();
        assert_eq!(set.files.len(), IconPreset::Web.file_names().len());
        assert!(set.html.unwrap().contains("apple-touch-icon"));
        assert_eq!(set.warnings.len(), 0);

        let touch =
            imaging::decode(&std::fs::read(dir.join("apple-touch-icon.png")).unwrap()).unwrap();
        assert_eq!(touch.dimensions(), (180, 180));
        assert!(
            touch.pixels().all(|p| p[3] == 255),
            "apple-touch-icon must be opaque"
        );

        let ico = std::fs::read(dir.join("favicon.ico")).unwrap();
        assert_eq!(
            u16::from_le_bytes([ico[4], ico[5]]),
            3,
            "favicon.ico holds 16, 32 and 48"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn warns_when_the_source_is_small() {
        let dir = temp_dir("small");
        let set = build(&source(64, 64), &dir, IconPreset::App).unwrap();
        assert!(set.warnings[0].contains("upscaled"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_to_overwrite_without_permission() {
        let dir = temp_dir("existing");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("icon.ico"), b"old").unwrap();
        assert_eq!(
            check_destination(&dir, IconPreset::App, false)
                .unwrap_err()
                .code(),
            "output_error"
        );
        assert!(check_destination(&dir, IconPreset::App, true).is_ok());
        assert!(check_destination(&dir, IconPreset::Web, false).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
