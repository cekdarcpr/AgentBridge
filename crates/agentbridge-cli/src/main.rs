mod agent;
mod assets;
mod commands;
mod output;

use std::{path::PathBuf, process::ExitCode};

use agentbridge_core::{icons::IconPreset, AppError, Fit, ProviderId, RunFailure};
use clap::{error::ErrorKind, Args, Parser, Subcommand, ValueEnum};

const DEFAULT_USAGE_DAYS: u32 = 7;
const DEFAULT_LOG_LIMIT: u32 = 20;
const MIN_DURATION_SECS: f32 = 0.5;
const MAX_DURATION_SECS: f32 = 30.0;

#[derive(Parser)]
#[command(
    name = "agb",
    version,
    about = "Ready-to-use images, icons, sound effects and speech for coding agents.",
    long_about = "Ready-to-use images, icons, sound effects and speech for coding agents.\n\
                  Generates through your own AI provider keys, then sizes, converts and cleans up the result.\n\
                  Built to be called by coding agents: pass --json for machine-readable output.\n\
                  Exit codes: 0 ok, 2 usage/config, 3 budget exceeded, 4 no usable provider, 5 provider error, 6 output error."
)]
struct Cli {
    /// Print machine-readable JSON to stdout
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct GenOptions {
    /// Use only this provider (openai, gemini, elevenlabs, openrouter)
    #[arg(long, value_parser = parse_provider)]
    provider: Option<ProviderId>,
    /// Model name; the provider is inferred from it when --provider is omitted
    #[arg(long)]
    model: Option<String>,
    /// Do not try the next provider when the first one fails
    #[arg(long)]
    no_fallback: bool,
    /// Show which provider, model and estimated cost would be used, without calling it
    #[arg(long)]
    dry_run: bool,
    /// Replace the output file if it already exists
    #[arg(long)]
    overwrite: bool,
    /// Project name for per-project budgets (default: git root of the current directory)
    #[arg(long)]
    project: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Generate an image (.png, .jpg, .webp)
    Image {
        prompt: String,
        /// Output file; the extension selects the format
        #[arg(long)]
        out: PathBuf,
        /// Exact output size as WIDTHxHEIGHT, e.g. 64x64
        #[arg(long)]
        size: Option<String>,
        /// How the image fills --size: cover (crop), contain (pad) or stretch [default: contain with --transparent, otherwise cover]
        #[arg(long, value_parser = parse_fit)]
        fit: Option<Fit>,
        /// Transparent background (.png/.webp); works with every provider
        #[arg(long)]
        transparent: bool,
        /// Crop transparent margins so the subject fills the frame (needs --transparent)
        #[arg(long)]
        trim: bool,
        #[arg(long, value_enum)]
        quality: Option<Quality>,
        /// Ignore the project style in agentbridge.toml for this image
        #[arg(long)]
        no_style: bool,
        #[command(flatten)]
        gen: GenOptions,
    },
    /// Generate a complete icon set (favicons or app icons) from a prompt or an existing image
    IconSet {
        /// What the icon should show; omit when using --from
        #[arg(required_unless_present = "from")]
        prompt: Option<String>,
        /// Folder the icons are written to
        #[arg(long)]
        out_dir: PathBuf,
        /// web: favicon.ico, PNG favicons, Apple touch icon, manifest icons; app: PNG sizes up to 1024 and icon.ico
        #[arg(long, value_parser = parse_preset, default_value = "web")]
        preset: IconPreset,
        /// Build the set from this image instead of generating one (free)
        #[arg(long, conflicts_with = "prompt")]
        from: Option<PathBuf>,
        /// Transparent background around the generated icon
        #[arg(long, conflicts_with = "from")]
        transparent: bool,
        /// Ignore the project style in agentbridge.toml for the generated icon
        #[arg(long, conflicts_with = "from")]
        no_style: bool,
        #[command(flatten)]
        gen: GenOptions,
    },
    /// Resize, convert, trim or remove the background of an existing image (local, free)
    Process {
        /// Image to process (.png, .jpg, .webp)
        input: PathBuf,
        /// Output file; the extension selects the format
        #[arg(long)]
        out: PathBuf,
        /// Exact output size as WIDTHxHEIGHT
        #[arg(long)]
        size: Option<String>,
        /// How the image fills --size: cover, contain or stretch [default: contain with --transparent, otherwise cover]
        #[arg(long, value_parser = parse_fit)]
        fit: Option<Fit>,
        /// Remove a flat, single-color background
        #[arg(long)]
        transparent: bool,
        /// Crop transparent margins
        #[arg(long)]
        trim: bool,
        /// Replace the output file if it already exists
        #[arg(long)]
        overwrite: bool,
    },
    /// Generate a sound effect (.mp3, .wav)
    Audio {
        prompt: String,
        #[arg(long)]
        out: PathBuf,
        /// Length in seconds (0.5-30); the provider decides when omitted
        #[arg(long, value_parser = parse_duration)]
        duration: Option<f32>,
        #[command(flatten)]
        gen: GenOptions,
    },
    /// Turn text into speech (.mp3, .wav)
    Speech {
        text: String,
        #[arg(long)]
        out: PathBuf,
        /// Provider-specific voice name or id
        #[arg(long)]
        voice: Option<String>,
        #[command(flatten)]
        gen: GenOptions,
    },
    /// Ask a text model; prints the answer or writes it to --out
    Ask {
        prompt: String,
        /// Text file to include in the prompt (repeatable)
        #[arg(long = "file")]
        files: Vec<PathBuf>,
        /// System instruction
        #[arg(long)]
        system: Option<String>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[command(flatten)]
        gen: GenOptions,
    },
    /// Show or create the project style that keeps generated images consistent
    Style {
        #[command(subcommand)]
        action: StyleAction,
    },
    /// Teach coding agents to use agb through CLAUDE.md or AGENTS.md
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Manage API keys in the OS keychain
    Keys {
        #[command(subcommand)]
        action: KeysAction,
    },
    /// List providers, their capabilities and key status
    Providers,
    /// Show spending against the configured limits
    Usage {
        #[arg(long, default_value_t = DEFAULT_USAGE_DAYS)]
        days: u32,
        #[arg(long)]
        project: Option<String>,
    },
    /// Show recent calls
    Log {
        #[arg(long, default_value_t = DEFAULT_LOG_LIMIT)]
        limit: u32,
    },
    /// Inspect or create the config file
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum KeysAction {
    /// Store a key read from stdin (never pass keys as arguments: they end up in shell history)
    Set {
        #[arg(value_parser = parse_provider)]
        provider: ProviderId,
    },
    /// Delete a stored key
    Remove {
        #[arg(value_parser = parse_provider)]
        provider: ProviderId,
    },
    /// Show where each provider's key comes from (values are never printed)
    List,
    /// Check a key with a free read-only request
    Test {
        #[arg(value_parser = parse_provider)]
        provider: ProviderId,
    },
}

#[derive(Subcommand)]
enum StyleAction {
    /// Print the style that applies in the current directory
    Show,
    /// Write an agentbridge.toml with an example style at the project root
    Init {
        /// Replace an existing agentbridge.toml
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum AgentAction {
    /// Add or update the agb instructions in the project's CLAUDE.md and AGENTS.md (AGENTS.md is
    /// created when neither exists)
    Init {
        /// Instruction file to write instead, relative to the project root (repeatable)
        #[arg(long = "file", value_name = "FILE")]
        files: Vec<PathBuf>,
    },
    /// Print the instructions, to paste into another agent's instruction file
    Instructions,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print the config and database locations
    Path,
    /// Print the effective config
    Show,
    /// Write a config file with the defaults
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Change one setting, e.g. `routing.image openrouter,gemini` or `limits.daily_usd 3`
    Set { key: String, value: String },
}

fn parse_provider(value: &str) -> Result<ProviderId, String> {
    value.parse()
}

fn parse_fit(value: &str) -> Result<Fit, String> {
    value.parse()
}

fn parse_preset(value: &str) -> Result<IconPreset, String> {
    value.parse()
}

fn parse_duration(value: &str) -> Result<f32, String> {
    let secs: f32 = value
        .parse()
        .map_err(|_| format!("'{value}' is not a number"))?;
    if !(MIN_DURATION_SECS..=MAX_DURATION_SECS).contains(&secs) {
        return Err(format!(
            "duration must be between {MIN_DURATION_SECS} and {MAX_DURATION_SECS} seconds"
        ));
    }
    Ok(secs)
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return handle_parse_error(error),
    };
    let json = cli.json;
    match commands::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            output::print_failure(&failure, json);
            ExitCode::from(failure.error.exit_code())
        }
    }
}

/// Agents that asked for JSON get JSON even when the arguments themselves are wrong.
fn handle_parse_error(error: clap::Error) -> ExitCode {
    let wants_json = std::env::args().any(|arg| arg == "--json");
    let is_info = matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    );
    if !wants_json || is_info {
        error.exit();
    }
    let failure = RunFailure::from(AppError::Usage(
        error.render().to_string().trim().to_string(),
    ));
    output::print_failure(&failure, true);
    ExitCode::from(failure.error.exit_code())
}
