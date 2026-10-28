use std::{
    collections::BTreeMap,
    fs,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

use agentbridge_core::{
    keystore, parse_size, project, providers, request::Attachment, style, AppError, AudioFormat,
    AudioRequest, Config, Engine, ImageFormat, ImageRequest, Paths, ProviderId, Reference, Request,
    RunFailure, RunOptions, RunResult, SpeechRequest, Store, TextRequest,
};
use serde::Serialize;
use serde_json::json;

use crate::{assets, output, Cli, Command, ConfigAction, GenOptions, KeysAction, StyleAction};

/// Guards against an agent accidentally attaching a binary or a huge log.
const MAX_ASK_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub fn run(cli: Cli) -> Result<(), RunFailure> {
    let json = cli.json;
    let paths = Paths::resolve()?;
    match cli.command {
        Command::Image {
            prompt,
            out,
            size,
            fit,
            transparent,
            trim,
            quality,
            no_style,
            gen,
        } => {
            let styled = apply_style(prompt, no_style)?;
            let request = Request::Image(ImageRequest {
                prompt: styled.prompt,
                size: size.as_deref().map(parse_size).transpose()?,
                transparent,
                quality: quality.map(|q| q.as_str().to_string()),
                format: ImageFormat::from_path(&out)?,
                fit,
                trim,
                reference: styled.reference,
            });
            let mut result = run_engine(&paths, &request, Some(out), &gen)?;
            result.style = styled.file;
            output::print_result(&result, json);
            Ok(())
        }
        Command::IconSet {
            prompt,
            out_dir,
            preset,
            from,
            transparent,
            no_style,
            gen,
        } => assets::icon_set(
            &paths,
            assets::IconSetArgs {
                prompt,
                out_dir,
                preset,
                from,
                transparent,
                no_style,
            },
            gen,
            json,
        ),
        Command::Process {
            input,
            out,
            size,
            fit,
            transparent,
            trim,
            overwrite,
        } => {
            let args = assets::ProcessArgs {
                input,
                out,
                size,
                fit,
                transparent,
                trim,
                overwrite,
            };
            Ok(assets::process(args, json)?)
        }
        Command::Audio {
            prompt,
            out,
            duration,
            gen,
        } => {
            let request = Request::Audio(AudioRequest {
                prompt,
                duration_secs: duration,
                format: AudioFormat::from_path(&out)?,
            });
            generate(&paths, request, Some(out), gen, json)
        }
        Command::Speech {
            text,
            out,
            voice,
            gen,
        } => {
            let request = Request::Speech(SpeechRequest {
                text,
                voice,
                format: AudioFormat::from_path(&out)?,
            });
            generate(&paths, request, Some(out), gen, json)
        }
        Command::Ask {
            prompt,
            files,
            system,
            out,
            gen,
        } => {
            let request = Request::Text(TextRequest {
                prompt,
                system,
                files: read_attachments(&files)?,
            });
            generate(&paths, request, out, gen, json)
        }
        Command::Style { action } => Ok(style_command(action, json)?),
        Command::Keys { action } => Ok(keys(action, json)?),
        Command::Providers => Ok(list_providers(&paths, json)?),
        Command::Usage { days, project } => Ok(usage(&paths, days, project.as_deref(), json)?),
        Command::Log { limit } => Ok(log(&paths, limit, json)?),
        Command::Config { action } => Ok(config(&paths, action, json)?),
    }
}

fn generate(
    paths: &Paths,
    request: Request,
    out: Option<PathBuf>,
    gen: GenOptions,
    json: bool,
) -> Result<(), RunFailure> {
    let result = run_engine(paths, &request, out, &gen)?;
    output::print_result(&result, json);
    Ok(())
}

pub fn run_engine(
    paths: &Paths,
    request: &Request,
    out: Option<PathBuf>,
    gen: &GenOptions,
) -> Result<RunResult, RunFailure> {
    let config = Config::load(&paths.config_file)?;
    let store = Store::open(&paths.db_file)?;
    let mut engine = Engine::new(config, store, project::resolve(gen.project.as_deref()))?;
    let opts = RunOptions {
        provider: gen.provider,
        model: gen.model.clone(),
        no_fallback: gen.no_fallback,
        dry_run: gen.dry_run,
        out,
        overwrite: gen.overwrite,
    };
    engine.run(request, &opts)
}

/// A prompt with the project style applied, the style's reference image and the file it came from.
pub struct Styled {
    pub prompt: String,
    pub reference: Option<Reference>,
    pub file: Option<String>,
}

pub fn apply_style(prompt: String, no_style: bool) -> Result<Styled, AppError> {
    let profile = if no_style { None } else { style::current()? };
    let Some(profile) = profile else {
        return Ok(Styled {
            prompt,
            reference: None,
            file: None,
        });
    };
    Ok(Styled {
        prompt: profile.apply(&prompt),
        reference: profile.load_reference()?,
        file: Some(project::display_path(&profile.file)),
    })
}

fn style_command(action: StyleAction, json: bool) -> Result<(), AppError> {
    match action {
        StyleAction::Show => {
            let profile = style::current()?;
            let file = profile.as_ref().map(|p| project::display_path(&p.file));
            let reference = profile
                .as_ref()
                .and_then(|p| p.reference_path.as_deref())
                .map(project::display_path);
            if json {
                output::print_ok(&json!({
                    "file": file,
                    "style": profile.as_ref().map(|p| &p.style),
                    "reference": reference,
                }));
                return Ok(());
            }
            let Some(profile) = profile else {
                println!(
                    "no project style; run `agb style init` to create {}",
                    style::FILE_NAME
                );
                return Ok(());
            };
            let style = &profile.style;
            println!("file:        {}", file.unwrap_or_default());
            println!("description: {}", style.description);
            println!("palette:     {}", style.palette.join(", "));
            println!("avoid:       {}", style.avoid);
            println!("reference:   {}", reference.unwrap_or_else(|| "-".into()));
        }
        StyleAction::Init { force } => {
            let file = style::default_file();
            if file.exists() && !force {
                return Err(AppError::Output(format!(
                    "{} already exists; pass --force to overwrite it",
                    project::display_path(&file)
                )));
            }
            project::write_file(&file, style::TEMPLATE.as_bytes())?;
            let path = project::display_path(&file);
            if json {
                output::print_ok(&json!({"file": path}));
            } else {
                println!("{path}");
            }
        }
    }
    Ok(())
}

fn read_attachments(files: &[PathBuf]) -> Result<Vec<Attachment>, AppError> {
    let mut attachments = Vec::new();
    for path in files {
        let size = fs::metadata(path)
            .map_err(|e| AppError::Usage(format!("cannot read {}: {e}", path.display())))?
            .len();
        if size > MAX_ASK_FILE_BYTES {
            return Err(AppError::Usage(format!(
                "{} is {size} bytes; the limit for --file is {MAX_ASK_FILE_BYTES} bytes",
                path.display()
            )));
        }
        let bytes = fs::read(path)?;
        attachments.push(Attachment {
            name: path.display().to_string(),
            content: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
    Ok(attachments)
}

fn key_status(provider: ProviderId) -> &'static str {
    keystore::status(provider).as_str()
}

fn keys(action: KeysAction, json: bool) -> Result<(), AppError> {
    match action {
        KeysAction::Test { provider } => {
            let key = keystore::get(provider)?.ok_or_else(|| {
                AppError::NoProvider(format!(
                    "no {} key stored or in {}",
                    provider.display_name(),
                    provider.env_var()
                ))
            })?;
            providers::verify_key(provider, &key.value).map_err(AppError::Provider)?;
            if json {
                output::print_ok(
                    &json!({"provider": provider, "valid": true, "source": key.source}),
                );
            } else {
                eprintln!("{} key is valid", provider.display_name());
            }
        }
        KeysAction::Set { provider } => {
            let key = read_secret(provider)?;
            keystore::set(provider, &key)?;
            if json {
                output::print_ok(&json!({"provider": provider, "stored": "keychain"}));
            } else {
                eprintln!("{} key stored in the OS keychain", provider.display_name());
            }
        }
        KeysAction::Remove { provider } => {
            let removed = keystore::remove(provider)?;
            if json {
                output::print_ok(&json!({"provider": provider, "removed": removed}));
            } else if removed {
                eprintln!("{} key removed", provider.display_name());
            } else {
                eprintln!("no stored {} key", provider.display_name());
            }
        }
        KeysAction::List => {
            let rows: Vec<_> = ProviderId::ALL
                .iter()
                .map(|p| json!({"provider": p, "key": key_status(*p), "env_var": p.env_var()}))
                .collect();
            if json {
                output::print_ok(&json!({"keys": rows}));
            } else {
                for p in ProviderId::ALL {
                    println!("{:<12} {}", p.as_str(), key_status(p));
                }
            }
        }
    }
    Ok(())
}

fn read_secret(provider: ProviderId) -> Result<String, AppError> {
    let stdin = io::stdin();
    let mut value = String::new();
    if stdin.is_terminal() {
        eprintln!(
            "Paste the {} API key and press Enter (the input is visible, so check nobody is watching):",
            provider.display_name()
        );
        stdin.read_line(&mut value)?;
    } else {
        stdin.lock().read_to_string(&mut value)?;
    }
    Ok(value.trim().to_string())
}

#[derive(Serialize)]
struct ProviderInfo {
    provider: ProviderId,
    name: &'static str,
    key: &'static str,
    models: BTreeMap<String, String>,
}

fn list_providers(paths: &Paths, json: bool) -> Result<(), AppError> {
    let config = Config::load(&paths.config_file)?;
    let infos: Vec<ProviderInfo> = ProviderId::ALL
        .iter()
        .map(|p| ProviderInfo {
            provider: *p,
            name: p.display_name(),
            key: key_status(*p),
            models: providers::capabilities(*p)
                .into_iter()
                .filter_map(|c| config.models.default_for(*p, c).map(|m| (c.to_string(), m)))
                .collect(),
        })
        .collect();
    if json {
        output::print_ok(&json!({"providers": infos}));
        return Ok(());
    }
    for info in infos {
        let caps: Vec<String> = info
            .models
            .iter()
            .map(|(c, m)| format!("{c}={m}"))
            .collect();
        println!(
            "{:<12} key:{:<9} {}",
            info.provider.as_str(),
            info.key,
            caps.join("  ")
        );
    }
    Ok(())
}

fn usage(paths: &Paths, days: u32, project_name: Option<&str>, json: bool) -> Result<(), AppError> {
    let config = Config::load(&paths.config_file)?;
    let store = Store::open(&paths.db_file)?;
    let current_project = project::resolve(project_name);
    let today_total = store.spent_today(None)?;
    let today_project = store.spent_today(Some(&current_project))?;
    let history = store.usage_by_day(days, project_name)?;
    let limits = &config.limits;

    if json {
        output::print_ok(&json!({
            "project": current_project,
            "today": {"spent_usd": today_total, "project_spent_usd": today_project},
            "limits": limits,
            "remaining": {
                "daily_usd": (limits.daily_usd - today_total).max(0.0),
                "project_daily_usd": (limits.project_daily_usd - today_project).max(0.0),
            },
            "days": history,
        }));
        return Ok(());
    }
    println!(
        "today (all projects): ${today_total:.3} / ${:.2}",
        limits.daily_usd
    );
    println!(
        "today ({current_project}): ${today_project:.3} / ${:.2}",
        limits.project_daily_usd
    );
    println!("per call limit: ${:.2}", limits.max_call_usd);
    for day in history {
        println!("{}  {:>4} calls  ${:.3}", day.day, day.calls, day.cost_usd);
    }
    Ok(())
}

fn log(paths: &Paths, limit: u32, json: bool) -> Result<(), AppError> {
    let calls = Store::open(&paths.db_file)?.recent(limit)?;
    if json {
        output::print_ok(&json!({"calls": calls}));
        return Ok(());
    }
    for call in calls {
        let cost = call
            .cost_usd
            .map(|c| format!("${c:.3}"))
            .unwrap_or_else(|| "-".into());
        let target = call.output_path.or(call.error).unwrap_or_default();
        println!(
            "#{:<5} {} {:<7} {}/{} {:<9} {cost:>7}  {target}",
            call.id, call.created_at, call.capability, call.provider, call.model, call.status
        );
    }
    Ok(())
}

fn config(paths: &Paths, action: ConfigAction, json: bool) -> Result<(), AppError> {
    match action {
        ConfigAction::Path => {
            let config_file = project::display_path(&paths.config_file);
            let db_file = project::display_path(&paths.db_file);
            if json {
                output::print_ok(
                    &json!({"config_file": config_file, "db_file": db_file, "config_exists": paths.config_file.exists()}),
                );
            } else {
                println!("config: {config_file}");
                println!("database: {db_file}");
            }
        }
        ConfigAction::Show => {
            let config = Config::load(&paths.config_file)?;
            if json {
                output::print_ok(&json!({"config": config}));
            } else {
                print!("{}", config.to_toml()?);
            }
        }
        ConfigAction::Init { force } => {
            write_default_config(&paths.config_file, force)?;
            let path = project::display_path(&paths.config_file);
            if json {
                output::print_ok(&json!({"config_file": path}));
            } else {
                println!("{path}");
            }
        }
    }
    Ok(())
}

fn write_default_config(path: &Path, force: bool) -> Result<(), AppError> {
    if path.exists() && !force {
        return Err(AppError::Output(format!(
            "{} already exists; pass --force to overwrite it",
            project::display_path(path)
        )));
    }
    Config::default().save(path)
}
