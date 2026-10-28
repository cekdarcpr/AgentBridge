//! Project style profiles: one look shared by every generated image in a project.
//!
//! The profile lives in `agentbridge.toml` next to the code, so it is versioned with the project and
//! every agent and teammate generating assets for it uses the same style.

use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use image::{imageops::FilterType, ImageFormat as RasterFormat};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, project, request::Reference};

pub const FILE_NAME: &str = "agentbridge.toml";
/// Large enough to carry the style, small enough to keep the request light.
const MAX_REFERENCE_SIDE: u32 = 1024;
const REFERENCE_PROMPT: &str = "Match the visual style of the attached reference image (palette, \
     rendering, line weight, lighting) but do not copy its subject or composition.";

pub const TEMPLATE: &str = r##"# agentBridge project settings. Commit this file so every agent and teammate
# generates assets in the same style. Remove a line to leave that part out.

[style]
# How every image in this project should look.
description = "flat vector illustration, bold rounded shapes, soft shading, clean outlines"
# Colors to stay within, as hex codes.
palette = ["#1E293B", "#6366F1", "#F59E0B", "#F8FAFC"]
# What to keep out of every image.
avoid = "text, watermarks, photorealism, gradients with banding"
# An existing asset whose style new images should match, relative to this file.
# reference = "assets/style-reference.png"
"##;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    pub description: String,
    pub palette: Vec<String>,
    pub avoid: String,
    /// Relative paths are resolved against the directory of `agentbridge.toml`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<PathBuf>,
}

impl Style {
    fn is_empty(&self) -> bool {
        self.description.trim().is_empty()
            && self.palette.is_empty()
            && self.avoid.trim().is_empty()
            && self.reference.is_none()
    }
}

/// Other sections may be added to the file later, so only `[style]` is read here.
#[derive(Deserialize)]
struct ProjectFile {
    #[serde(default)]
    style: Option<Style>,
}

#[derive(Debug, Clone)]
pub struct ProjectStyle {
    pub file: PathBuf,
    pub style: Style,
    /// Absolute path of the reference image, when one is set.
    pub reference_path: Option<PathBuf>,
}

impl ProjectStyle {
    /// Adds the style to a prompt. The subject comes first so the style refines it rather than
    /// replacing it.
    pub fn apply(&self, prompt: &str) -> String {
        let style = &self.style;
        let mut parts = vec![prompt.trim_end().to_string()];
        if !style.description.trim().is_empty() {
            parts.push(format!(
                "Style: {}.",
                style.description.trim().trim_end_matches('.')
            ));
        }
        if !style.palette.is_empty() {
            parts.push(format!(
                "Use this color palette: {}.",
                style.palette.join(", ")
            ));
        }
        if !style.avoid.trim().is_empty() {
            parts.push(format!(
                "Avoid: {}.",
                style.avoid.trim().trim_end_matches('.')
            ));
        }
        if self.reference_path.is_some() {
            parts.push(REFERENCE_PROMPT.to_string());
        }
        parts.join(" ")
    }

    pub fn load_reference(&self) -> Result<Option<Reference>, AppError> {
        self.reference_path
            .as_deref()
            .map(load_reference)
            .transpose()
    }
}

/// Where a new `agentbridge.toml` belongs: the project root.
pub fn default_file() -> PathBuf {
    project::root().join(FILE_NAME)
}

/// The nearest `agentbridge.toml` from the current directory up to the project root.
pub fn find() -> Option<PathBuf> {
    let root = project::root();
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .take_while(|dir| dir.starts_with(&root))
        .map(|dir| dir.join(FILE_NAME))
        .find(|file| file.is_file())
}

/// `Ok(None)` when there is no file or it has no style in it.
pub fn current() -> Result<Option<ProjectStyle>, AppError> {
    find()
        .map(|file| load(&file))
        .transpose()
        .map(Option::flatten)
}

pub fn load(file: &Path) -> Result<Option<ProjectStyle>, AppError> {
    let text = fs::read_to_string(file)?;
    let parsed: ProjectFile = toml::from_str(&text)
        .map_err(|e| AppError::Config(format!("invalid {}: {e}", project::display_path(file))))?;
    let Some(style) = parsed.style.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let reference_path = style
        .reference
        .as_deref()
        .map(|reference| resolve_reference(file, reference))
        .transpose()?;
    Ok(Some(ProjectStyle {
        file: file.to_path_buf(),
        style,
        reference_path,
    }))
}

/// The reference image is uploaded to the provider, and `agentbridge.toml` arrives with whatever
/// repository was cloned. Keeping the reference inside that project stops a hostile file from
/// sending a private image from elsewhere on the machine. Canonical paths also catch `..` and
/// symbolic links that lead outside.
fn resolve_reference(file: &Path, reference: &Path) -> Result<PathBuf, AppError> {
    let invalid = |reason: &str| {
        AppError::Config(format!(
            "style reference {} in {} {reason}",
            reference.display(),
            project::display_path(file)
        ))
    };
    let base = file.parent().unwrap_or(Path::new("."));
    let root = base
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(base);
    let root = fs::canonicalize(root).map_err(|e| invalid(&e.to_string()))?;
    let path = fs::canonicalize(base.join(reference)).map_err(|_| invalid("does not exist"))?;
    if !path.starts_with(&root) {
        return Err(invalid("must be inside the project folder"));
    }
    if !path.is_file() {
        return Err(invalid("is not a file"));
    }
    Ok(PathBuf::from(project::display_path(&path)))
}

fn load_reference(path: &Path) -> Result<Reference, AppError> {
    let invalid = |e: String| {
        AppError::Config(format!(
            "cannot use style reference {}: {e}",
            project::display_path(path)
        ))
    };
    let bytes = fs::read(path).map_err(|e| invalid(e.to_string()))?;
    let mut image = image::load_from_memory(&bytes).map_err(|e| invalid(e.to_string()))?;
    if image.width().max(image.height()) > MAX_REFERENCE_SIDE {
        image = image.resize(MAX_REFERENCE_SIDE, MAX_REFERENCE_SIDE, FilterType::Lanczos3);
    }
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), RasterFormat::Png)
        .map_err(|e| invalid(e.to_string()))?;
    Ok(Reference { png })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(style: Style, reference: bool) -> ProjectStyle {
        ProjectStyle {
            file: PathBuf::from(FILE_NAME),
            style,
            reference_path: reference.then(|| PathBuf::from("ref.png")),
        }
    }

    #[test]
    fn template_parses_to_a_style() {
        let parsed: ProjectFile = toml::from_str(TEMPLATE).unwrap();
        let style = parsed.style.unwrap();
        assert!(!style.description.is_empty());
        assert_eq!(style.palette.len(), 4);
        assert!(style.reference.is_none());
    }

    #[test]
    fn applies_style_after_the_subject() {
        let style = Style {
            description: "pixel art, 16-bit.".into(),
            palette: vec!["#000000".into(), "#FFFFFF".into()],
            avoid: "text".into(),
            reference: None,
        };
        assert_eq!(
            profile(style.clone(), false).apply("a sword "),
            "a sword Style: pixel art, 16-bit. Use this color palette: #000000, #FFFFFF. Avoid: text."
        );
        assert!(profile(style, true)
            .apply("a sword")
            .ends_with(REFERENCE_PROMPT));
    }

    #[test]
    fn rejects_unknown_style_keys() {
        assert!(toml::from_str::<ProjectFile>("[style]\npallete = []\n").is_err());
        let parsed: ProjectFile = toml::from_str("[other]\nx = 1\n").unwrap();
        assert!(parsed.style.is_none());
    }

    #[test]
    fn loads_and_validates_files() {
        let dir = std::env::temp_dir().join(format!("agb-style-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join(FILE_NAME);

        fs::write(&file, "[style]\ndescription = \"\"\n").unwrap();
        assert!(load(&file).unwrap().is_none());

        fs::write(&file, "[style]\nreference = \"missing.png\"\n").unwrap();
        assert_eq!(load(&file).unwrap_err().code(), "config_error");

        let outside = dir.with_extension("outside.png");
        image::RgbaImage::new(4, 4).save(&outside).unwrap();
        let name = outside.file_name().unwrap().to_string_lossy();
        for escape in [
            format!("../{name}"),
            outside.display().to_string().replace('\\', "/"),
        ] {
            fs::write(&file, format!("[style]\nreference = \"{escape}\"\n")).unwrap();
            let error = load(&file).unwrap_err().to_string();
            assert!(error.contains("inside the project"), "{error}");
        }
        fs::remove_file(&outside).unwrap();

        let reference = image::RgbaImage::from_pixel(2048, 1024, image::Rgba([10, 20, 30, 255]));
        reference.save(dir.join("ref.png")).unwrap();
        fs::write(&file, "[style]\nreference = \"ref.png\"\n").unwrap();
        let loaded = load(&file).unwrap().unwrap();
        let png = loaded.load_reference().unwrap().unwrap().png;
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1024, 512));

        fs::remove_dir_all(&dir).unwrap();
    }
}
