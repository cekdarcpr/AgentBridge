use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use reqwest::blocking::Client;
use serde::Serialize;

use crate::{
    capability::{Capability, ProviderId},
    config::Config,
    error::AppError,
    keystore, pricing, project,
    providers::{self, CallContext, Payload},
    request::Request,
    store::{NewCall, Store},
};

/// Short strings are not redacted: they could match ordinary words in an error message.
const MIN_REDACT_LEN: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub no_fallback: bool,
    pub dry_run: bool,
    pub out: Option<PathBuf>,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Skipped,
    Failed,
    Succeeded,
    Planned,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attempt {
    pub provider: ProviderId,
    pub model: Option<String>,
    pub status: AttemptStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Attempt {
    fn new(provider: ProviderId, model: Option<&str>, status: AttemptStatus, reason: Option<String>) -> Self {
        Self { provider, model: model.map(str::to_string), status, reason }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    pub capability: Capability,
    pub provider: ProviderId,
    pub model: String,
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub cost_usd: f64,
    pub cost_is_estimate: bool,
    pub fallback_used: bool,
    pub dry_run: bool,
    pub call_id: Option<i64>,
    pub attempts: Vec<Attempt>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct RunFailure {
    pub error: AppError,
    pub attempts: Vec<Attempt>,
}

impl From<AppError> for RunFailure {
    fn from(error: AppError) -> Self {
        Self { error, attempts: Vec::new() }
    }
}

pub struct Engine {
    config: Config,
    store: Store,
    http: Client,
    project: String,
}

impl Engine {
    pub fn new(config: Config, store: Store, project: String) -> Result<Self, AppError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(config.http_timeout_secs))
            .user_agent(concat!("agentbridge/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AppError::Internal(format!("could not build HTTP client: {e}")))?;
        Ok(Self { config, store, http, project })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    /// Tries candidate providers in order. Missing keys and provider errors move on to the next
    /// candidate; budget rejections stop immediately because another provider would not change them.
    pub fn run(&mut self, request: &Request, opts: &RunOptions) -> Result<RunResult, RunFailure> {
        let capability = request.capability();
        let out_path = prepare_output(capability, opts)?;
        let candidates = self.candidates(request, opts)?;
        let mut attempts = Vec::new();
        let mut last_error = None;

        for provider_id in candidates {
            let provider = providers::get(provider_id);
            let key = match keystore::get(provider_id) {
                Ok(Some(key)) => key,
                Ok(None) => {
                    let reason = format!(
                        "no API key; run `agb keys set {provider_id}` or set {}",
                        provider_id.env_var()
                    );
                    attempts.push(Attempt::new(provider_id, None, AttemptStatus::Skipped, Some(reason)));
                    continue;
                }
                Err(error) => {
                    attempts.push(Attempt::new(provider_id, None, AttemptStatus::Skipped, Some(error.to_string())));
                    continue;
                }
            };
            let Some(model) = opts.model.clone().or_else(|| self.config.models.default_for(provider_id, capability)) else {
                let reason = format!("no default {capability} model configured");
                attempts.push(Attempt::new(provider_id, None, AttemptStatus::Skipped, Some(reason)));
                continue;
            };

            let price = pricing::lookup(&self.config.pricing, provider_id, &model);
            let estimate = pricing::estimate(price.as_ref(), request.prompt());
            let mut warnings = provider.limitations(request);
            let fallback_used = !attempts.is_empty();

            if opts.dry_run {
                attempts.push(Attempt::new(provider_id, Some(&model), AttemptStatus::Planned, None));
                return Ok(RunResult {
                    capability,
                    provider: provider_id,
                    model,
                    output: out_path.as_deref().map(project::display_path),
                    text: None,
                    cost_usd: estimate,
                    cost_is_estimate: true,
                    fallback_used,
                    dry_run: true,
                    call_id: None,
                    attempts,
                    warnings,
                });
            }

            let new_call = NewCall {
                project: &self.project,
                capability: capability.as_str(),
                provider: provider_id.as_str(),
                model: &model,
                prompt: request.prompt(),
                estimated_usd: estimate,
            };
            let call_id = match self.store.reserve(&new_call, &self.config.limits) {
                Ok(id) => id,
                Err(error) => return Err(RunFailure { error, attempts }),
            };

            let started = Instant::now();
            let ctx = CallContext { http: &self.http, api_key: &key.value, model: &model, models: &self.config.models };
            let result = provider.execute(&ctx, request);
            let duration_ms = elapsed_ms(started);

            let output = match result {
                Ok(output) => output,
                Err(message) => {
                    let message = redact(&message, &key.value);
                    if let Err(error) = self.store.finish_failure(call_id, &message, duration_ms) {
                        return Err(RunFailure { error, attempts });
                    }
                    attempts.push(Attempt::new(provider_id, Some(&model), AttemptStatus::Failed, Some(message.clone())));
                    last_error = Some(message);
                    if opts.no_fallback {
                        break;
                    }
                    continue;
                }
            };

            let (cost_usd, cost_is_estimate) = pricing::actual(price.as_ref(), output.usage.as_ref(), estimate);
            warnings.extend(output.warnings);
            let written = write_payload(output.payload, out_path.as_deref(), request.expected_mime(), &mut warnings);
            // The provider has billed us either way, so the cost is recorded even if writing failed.
            let logged_path = written.as_ref().ok().and_then(|(path, _)| path.as_deref());
            if let Err(error) = self.store.finish_success(call_id, cost_usd, cost_is_estimate, logged_path, duration_ms) {
                return Err(RunFailure { error, attempts });
            }
            attempts.push(Attempt::new(provider_id, Some(&model), AttemptStatus::Succeeded, None));
            let (output, text) = match written {
                Ok(written) => written,
                Err(error) => return Err(RunFailure { error, attempts }),
            };

            return Ok(RunResult {
                capability,
                provider: provider_id,
                model,
                output,
                text,
                cost_usd,
                cost_is_estimate,
                fallback_used,
                dry_run: false,
                call_id: Some(call_id),
                attempts,
                warnings,
            });
        }

        let error = match last_error {
            Some(message) if opts.no_fallback => AppError::Provider(message),
            Some(message) => AppError::Provider(format!("all providers failed; last error: {message}")),
            None => AppError::NoProvider(format!("no usable provider for {capability}; see attempts for details")),
        };
        Err(RunFailure { error, attempts })
    }

    fn candidates(&self, request: &Request, opts: &RunOptions) -> Result<Vec<ProviderId>, AppError> {
        let capability = request.capability();
        let explicit = opts.provider.or_else(|| opts.model.as_deref().and_then(ProviderId::infer_from_model));
        if let Some(provider) = explicit {
            if !providers::get(provider).supports(capability) {
                return Err(AppError::Usage(format!("{provider} does not support {capability}")));
            }
            return Ok(vec![provider]);
        }
        if opts.model.is_some() {
            return Err(AppError::Usage("cannot infer the provider from --model; pass --provider as well".into()));
        }

        let mut list: Vec<ProviderId> = self
            .config
            .routing
            .for_capability(capability)
            .iter()
            .copied()
            .filter(|p| providers::get(*p).supports(capability))
            .collect();
        if list.is_empty() {
            return Err(AppError::NoProvider(format!(
                "routing.{capability} in the config lists no provider that supports {capability}"
            )));
        }
        // Providers that cannot honour the request go last rather than being dropped,
        // so the call still succeeds when they are the only option. The sort is stable.
        list.sort_by_key(|p| !providers::get(*p).limitations(request).is_empty());
        Ok(list)
    }
}

fn prepare_output(capability: Capability, opts: &RunOptions) -> Result<Option<PathBuf>, AppError> {
    let Some(out) = &opts.out else {
        if capability == Capability::Text {
            return Ok(None);
        }
        return Err(AppError::Usage("--out is required".into()));
    };
    let absolute = if out.is_absolute() { out.clone() } else { env::current_dir()?.join(out) };
    if absolute.exists() && !opts.overwrite {
        return Err(AppError::Output(format!(
            "{} already exists; pass --overwrite to replace it",
            project::display_path(&absolute)
        )));
    }
    Ok(Some(absolute))
}

/// Returns `(output path, text)`.
fn write_payload(
    payload: Payload,
    out: Option<&Path>,
    expected_mime: Option<&str>,
    warnings: &mut Vec<String>,
) -> Result<(Option<String>, Option<String>), AppError> {
    match payload {
        Payload::Bytes { data, mime } => {
            let out = out.ok_or_else(|| AppError::Usage("--out is required".into()))?;
            if let Some(expected) = expected_mime.filter(|e| !mime.eq_ignore_ascii_case(e)) {
                warnings.push(format!(
                    "provider returned {mime} but the output extension expects {expected}; the file was written without conversion"
                ));
            }
            write_file(out, &data)?;
            Ok((Some(project::display_path(out)), None))
        }
        Payload::Text(text) => match out {
            Some(out) => {
                write_file(out, text.as_bytes())?;
                Ok((Some(project::display_path(out)), Some(text)))
            }
            None => Ok((None, Some(text))),
        },
    }
}

fn write_file(path: &Path, data: &[u8]) -> Result<(), AppError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, data)
        .map_err(|e| AppError::Output(format!("could not write {}: {e}", project::display_path(path))))
}

fn elapsed_ms(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

fn redact(message: &str, secret: &str) -> String {
    if secret.len() < MIN_REDACT_LEN {
        return message.to_string();
    }
    message.replace(secret, "[redacted]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{ImageFormat, ImageRequest};

    fn engine() -> Engine {
        Engine::new(Config::default(), Store::open_in_memory().unwrap(), "test".into()).unwrap()
    }

    fn image(transparent: bool) -> Request {
        Request::Image(ImageRequest {
            prompt: "icon".into(),
            size: None,
            transparent,
            quality: None,
            format: ImageFormat::Png,
        })
    }

    #[test]
    fn transparency_moves_incapable_providers_last() {
        let mut config = Config::default();
        config.routing.image = vec![ProviderId::Gemini, ProviderId::Openai];
        let engine = Engine::new(config, Store::open_in_memory().unwrap(), "t".into()).unwrap();
        let order = engine.candidates(&image(true), &RunOptions::default()).unwrap();
        assert_eq!(order, vec![ProviderId::Openai, ProviderId::Gemini]);
        let order = engine.candidates(&image(false), &RunOptions::default()).unwrap();
        assert_eq!(order, vec![ProviderId::Gemini, ProviderId::Openai]);
    }

    #[test]
    fn model_selects_its_provider() {
        let opts = RunOptions { model: Some("gemini-2.5-pro".into()), ..Default::default() };
        assert_eq!(engine().candidates(&image(false), &opts).unwrap(), vec![ProviderId::Gemini]);
    }

    #[test]
    fn rejects_provider_without_capability() {
        let opts = RunOptions { provider: Some(ProviderId::Elevenlabs), ..Default::default() };
        let err = engine().candidates(&image(false), &opts).unwrap_err();
        assert_eq!(err.code(), "usage");
    }

    #[test]
    fn redacts_keys_from_messages() {
        assert_eq!(redact("bad key sk-12345678", "sk-12345678"), "bad key [redacted]");
        assert_eq!(redact("short", "abc"), "short");
    }
}
