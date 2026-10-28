//! Asset commands built on top of generation: icon sets and local image processing.

use std::{fs, path::PathBuf};

use agentbridge_core::{
    icons::{self, IconPreset, IconSet, MASTER_SIZE},
    imaging::{self, ImageSpec},
    parse_size, project, AppError, Fit, ImageFormat, ImageRequest, Paths, Request, RunFailure,
    RunResult,
};
use serde::Serialize;

use crate::{
    commands::{apply_style, run_engine},
    output, GenOptions,
};

/// The generated master is kept next to the icons so the set can be rebuilt for free with `--from`.
const MASTER_FILE: &str = "icon-source.png";
/// Small icons are unreadable with fine detail or lettering, so generated masters ask for neither.
const ICON_PROMPT_SUFFIX: &str =
    "Designed as an app icon: simple bold shapes, centered, clearly readable at 16 pixels, no text.";

pub struct ProcessArgs {
    pub input: PathBuf,
    pub out: PathBuf,
    pub size: Option<String>,
    pub fit: Option<Fit>,
    pub transparent: bool,
    pub trim: bool,
    pub overwrite: bool,
}

#[derive(Serialize)]
pub struct ProcessResult {
    pub input: String,
    pub output: String,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<String>,
}

pub fn process(args: ProcessArgs, json: bool) -> Result<(), AppError> {
    let format = ImageFormat::from_path(&args.out)?;
    if args.transparent && !format.supports_alpha() {
        return Err(AppError::Usage(
            "JPEG cannot be transparent; use a .png or .webp output".into(),
        ));
    }
    let size = args.size.as_deref().map(parse_size).transpose()?;
    let input = project::absolute(&args.input)?;
    let data = fs::read(&input).map_err(|e| {
        AppError::Usage(format!(
            "cannot read {}: {e}",
            project::display_path(&input)
        ))
    })?;
    // Resolved after reading, so processing a file in place with --overwrite works.
    let out = project::resolve_output(&args.out, args.overwrite)?;

    let spec = ImageSpec {
        size,
        fit: args
            .fit
            .unwrap_or_else(|| Fit::default_for(args.transparent)),
        remove_background: args.transparent,
        trim: args.trim,
        format,
    };
    let processed = imaging::process(&data, &spec)?;
    project::write_file(&out, &processed.data)?;

    output::print_process(
        &ProcessResult {
            input: project::display_path(&input),
            output: project::display_path(&out),
            width: processed.width,
            height: processed.height,
            warnings: processed.warnings,
        },
        json,
    );
    Ok(())
}

pub struct IconSetArgs {
    pub prompt: Option<String>,
    pub out_dir: PathBuf,
    pub preset: IconPreset,
    pub from: Option<PathBuf>,
    pub transparent: bool,
    pub no_style: bool,
}

#[derive(Serialize)]
pub struct IconSetResult {
    #[serde(flatten)]
    pub set: IconSet,
    pub source: String,
    /// The generation call, when the master image was generated rather than given with `--from`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<RunResult>,
}

#[derive(Serialize)]
pub struct IconSetPlan {
    pub dry_run: bool,
    pub preset: IconPreset,
    pub out_dir: String,
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<RunResult>,
}

pub fn icon_set(
    paths: &Paths,
    args: IconSetArgs,
    gen: GenOptions,
    json: bool,
) -> Result<(), RunFailure> {
    let out_dir = project::absolute(&args.out_dir)?;
    icons::check_destination(&out_dir, args.preset, gen.overwrite)?;

    let plan = |generation: Option<RunResult>| IconSetPlan {
        dry_run: true,
        preset: args.preset,
        out_dir: project::display_path(&out_dir),
        files: args
            .preset
            .file_names()
            .iter()
            .map(|name| project::display_path(&out_dir.join(name)))
            .collect(),
        generation,
    };

    let (source, generation) = match (&args.from, &args.prompt) {
        (Some(from), _) => {
            if gen.dry_run {
                output::print_icon_plan(&plan(None), json);
                return Ok(());
            }
            (project::absolute(from)?, None)
        }
        (None, Some(prompt)) => {
            let master = out_dir.join(MASTER_FILE);
            let styled = apply_style(prompt.clone(), args.no_style)?;
            let request = Request::Image(ImageRequest {
                prompt: format!("{} {ICON_PROMPT_SUFFIX}", styled.prompt),
                size: Some((MASTER_SIZE, MASTER_SIZE)),
                transparent: args.transparent,
                quality: None,
                format: ImageFormat::Png,
                fit: Some(Fit::Contain),
                trim: false,
                reference: styled.reference,
            });
            let mut result = run_engine(paths, &request, Some(master.clone()), &gen)?;
            result.style = styled.file;
            if result.dry_run {
                output::print_icon_plan(&plan(Some(result)), json);
                return Ok(());
            }
            (master, Some(result))
        }
        (None, None) => {
            return Err(AppError::Usage("give a prompt or --from <image>".into()).into())
        }
    };

    let data = fs::read(&source).map_err(|e| {
        AppError::Usage(format!(
            "cannot read {}: {e}",
            project::display_path(&source)
        ))
    })?;
    let set = icons::build(&data, &out_dir, args.preset)?;
    output::print_icon_set(
        &IconSetResult {
            set,
            source: project::display_path(&source),
            generation,
        },
        json,
    );
    Ok(())
}
