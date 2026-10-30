use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use reqwest::blocking::Client;
use serde::Serialize;

use crate::{
    capability::{Capability, ProviderId},
    config::Config,
    error::AppError,
    imaging::{self, ImageSpec},
    keystore, pricing, project,
    providers::{self, redact, CallContext, Payload},
    request::Request,
    store::{NewCall, Store},
};

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
    fn new(
        provider: ProviderId,
        model: Option<&str>,
        status: AttemptStatus,
        reason: Option<String>,
    ) -> Self {
        Self {
            provider,
            model: model.map(str::to_string),
            status,
            reason,
        }
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
    /// Final pixel size, for images.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub cost_usd: f64,
    pub cost_is_estimate: bool,
    pub fallback_used: bool,
    pub dry_run: bool,
    pub call_id: Option<i64>,
    pub attempts: Vec<Attempt>,
    pub warnings: Vec<String>,
    /// The project style file applied to the prompt, set by the front end that applied it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

#[derive(Debug)]
pub struct RunFailure {
    pub error: AppError,
    pub attempts: Vec<Attempt>,
}

impl From<AppError> for RunFailure {
    fn from(error: AppError) -> Self {
        Self {
            error,
            attempts: Vec::new(),
        }
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
        let http = providers::client(Duration::from_secs(config.http_timeout_secs))
            .map_err(AppError::Internal)?;
        Ok(Self {
            config,
            store,
            http,
            project,
        })
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
        request.validate()?;
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
                    attempts.push(Attempt::new(
                        provider_id,
                        None,
                        AttemptStatus::Skipped,
                        Some(reason),
                    ));
                    continue;
                }
                Err(error) => {
                    attempts.push(Attempt::new(
                        provider_id,
                        None,
                        AttemptStatus::Skipped,
                        Some(error.to_string()),
                    ));
                    continue;
                }
            };
            let Some(model) = opts
                .model
                .clone()
                .or_else(|| self.config.models.default_for(provider_id, capability))
            else {
                let reason = format!("no default {capability} model configured");
                attempts.push(Attempt::new(
                    provider_id,
                    None,
                    AttemptStatus::Skipped,
                    Some(reason),
                ));
                continue;
            };

            let price = pricing::lookup(&self.config.pricing, provider_id, &model);
            let estimate = pricing::estimate(price.as_ref(), &request.full_prompt());
            let mut warnings = Vec::new();
            let fallback_used = !attempts.is_empty();
            // Providers without alpha output draw on a flat backdrop that `imaging` removes.
            let use_backdrop = matches!(request, Request::Image(r) if r.transparent)
                && !provider.native_transparency();
            let backdrop_request;
            let provider_request = if use_backdrop {
                backdrop_request = request.with_backdrop();
                &backdrop_request
            } else {
                request
            };

            if opts.dry_run {
                attempts.push(Attempt::new(
                    provider_id,
                    Some(&model),
                    AttemptStatus::Planned,
                    None,
                ));
                return Ok(RunResult {
                    capability,
                    provider: provider_id,
                    model,
                    output: out_path.as_deref().map(project::display_path),
                    text: None,
                    width: None,
                    height: None,
                    cost_usd: estimate,
                    cost_is_estimate: true,
                    fallback_used,
                    dry_run: true,
                    call_id: None,
                    attempts,
                    warnings,
                    style: None,
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
            let ctx = CallContext {
                http: &self.http,
                api_key: &key.value,
                model: &model,
                models: &self.config.models,
            };
            let result = provider.execute(&ctx, provider_request);
            let duration_ms = elapsed_ms(started);

            let output = match result {
                Ok(output) => output,
                Err(message) => {
                    let message = redact(&message, &key.value);
                    if let Err(error) = self.store.finish_failure(call_id, &message, duration_ms) {
                        return Err(RunFailure { error, attempts });
                    }
                    attempts.push(Attempt::new(
                        provider_id,
                        Some(&model),
                        AttemptStatus::Failed,
                        Some(message.clone()),
                    ));
                    last_error = Some(message);
                    if opts.no_fallback {
                        break;
                    }
                    continue;
                }
            };

            let (cost_usd, cost_is_estimate) =
                pricing::actual(price.as_ref(), output.usage.as_ref(), estimate);
            warnings.extend(output.warnings);
            let finished = finish_payload(
                output.payload,
                request,
                use_backdrop,
                out_path.as_deref(),
                &mut warnings,
            );
            // The provider has billed us either way, so the cost is recorded even if writing failed.
            let logged_path = finished.as_ref().ok().and_then(|f| f.output.as_deref());
            if let Err(error) = self.store.finish_success(
                call_id,
                cost_usd,
                cost_is_estimate,
                logged_path,
                duration_ms,
            ) {
                return Err(RunFailure { error, attempts });
            }
            attempts.push(Attempt::new(
                provider_id,
                Some(&model),
                AttemptStatus::Succeeded,
                None,
            ));
            let finished = match finished {
                Ok(finished) => finished,
                Err(error) => return Err(RunFailure { error, attempts }),
            };

            return Ok(RunResult {
                capability,
                provider: provider_id,
                model,
                output: finished.output,
                text: finished.text,
                width: finished.width,
                height: finished.height,
                cost_usd,
                cost_is_estimate,
                fallback_used,
                dry_run: false,
                call_id: Some(call_id),
                attempts,
                warnings,
                style: None,
            });
        }

        let error = match last_error {
            Some(message) if opts.no_fallback => AppError::Provider(message),
            Some(message) => {
                AppError::Provider(format!("all providers failed; last error: {message}"))
            }
            None => AppError::NoProvider(format!(
                "no usable provider for {capability}; see attempts for details"
            )),
        };
        Err(RunFailure { error, attempts })
    }

    fn candidates(
        &self,
        request: &Request,
        opts: &RunOptions,
    ) -> Result<Vec<ProviderId>, AppError> {
        let capability = request.capability();
        let explicit = opts
            .provider
            .or_else(|| opts.model.as_deref().and_then(ProviderId::infer_from_model));
        if let Some(provider) = explicit {
            if !providers::get(provider).supports(capability) {
                return Err(AppError::Usage(format!(
                    "{provider} does not support {capability}"
                )));
            }
            return Ok(vec![provider]);
        }
        if opts.model.is_some() {
            return Err(AppError::Usage(
                "cannot infer the provider from --model; pass --provider as well".into(),
            ));
        }

        let list: Vec<ProviderId> = self
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
    project::resolve_output(out, opts.overwrite).map(Some)
}

struct Finished {
    output: Option<String>,
    text: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

/// Post-processes images into the requested asset and writes every payload to its destination.
fn finish_payload(
    payload: Payload,
    request: &Request,
    remove_backdrop: bool,
    out: Option<&Path>,
    warnings: &mut Vec<String>,
) -> Result<Finished, AppError> {
    let required = || AppError::Usage("--out is required".into());
    match (payload, request) {
        (Payload::Bytes { data, .. }, Request::Image(r)) => {
            let out = out.ok_or_else(required)?;
            let spec = ImageSpec {
                size: r.size,
                fit: r.effective_fit(),
                remove_background: remove_backdrop,
                trim: r.trim,
                format: r.format,
            };
            let processed = imaging::process(&data, &spec)?;
            warnings.extend(processed.warnings);
            project::write_file(out, &processed.data)?;
            Ok(Finished {
                output: Some(project::display_path(out)),
                text: None,
                width: Some(processed.width),
                height: Some(processed.height),
            })
        }
        (Payload::Bytes { data, mime }, _) => {
            let out = out.ok_or_else(required)?;
            if let Some(expected) = request
                .expected_mime()
                .filter(|e| !mime.eq_ignore_ascii_case(e))
            {
                warnings.push(format!(
                    "provider returned {mime} but the output extension expects {expected}; the file was written without conversion"
                ));
            }
            project::write_file(out, &data)?;
            Ok(Finished {
                output: Some(project::display_path(out)),
                text: None,
                width: None,
                height: None,
            })
        }
        (Payload::Text(text), _) => {
            let output = match out {
                Some(out) => {
                    project::write_file(out, text.as_bytes())?;
                    Some(project::display_path(out))
                }
                None => None,
            };
            Ok(Finished {
                output,
                text: Some(text),
                width: None,
                height: None,
            })
        }
    }
}

fn elapsed_ms(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        imaging::BACKDROP_PROMPT,
        request::{ImageFormat, ImageRequest},
    };

    fn engine() -> Engine {
        Engine::new(
            Config::default(),
            Store::open_in_memory().unwrap(),
            "test".into(),
        )
        .unwrap()
    }

    fn image(transparent: bool) -> Request {
        Request::Image(ImageRequest {
            prompt: "icon".into(),
            size: None,
            transparent,
            quality: None,
            format: ImageFormat::Png,
            fit: None,
            trim: false,
            reference: None,
        })
    }

    #[test]
    fn routing_order_is_kept() {
        let mut config = Config::default();
        config.routing.image = vec![ProviderId::Gemini, ProviderId::Openai];
        let engine = Engine::new(config, Store::open_in_memory().unwrap(), "t".into()).unwrap();
        let order = engine
            .candidates(&image(true), &RunOptions::default())
            .unwrap();
        assert_eq!(order, vec![ProviderId::Gemini, ProviderId::Openai]);
    }

    #[test]
    fn backdrop_prompt_is_added_for_transparency() {
        let Request::Image(keyed) = image(true).with_backdrop() else {
            unreachable!()
        };
        assert!(keyed.prompt.starts_with("icon "));
        assert!(keyed.prompt.ends_with(BACKDROP_PROMPT));
    }

    #[test]
    fn transparent_jpeg_is_rejected_before_spending() {
        let mut engine = engine();
        let Request::Image(mut request) = image(true) else {
            unreachable!()
        };
        request.format = ImageFormat::Jpeg;
        let opts = RunOptions {
            out: Some("a.jpg".into()),
            dry_run: true,
            ..Default::default()
        };
        let failure = engine.run(&Request::Image(request), &opts).unwrap_err();
        assert_eq!(failure.error.code(), "usage");
    }

    #[test]
    fn model_selects_its_provider() {
        let opts = RunOptions {
            model: Some("gemini-2.5-pro".into()),
            ..Default::default()
        };
        assert_eq!(
            engine().candidates(&image(false), &opts).unwrap(),
            vec![ProviderId::Gemini]
        );
    }

    #[test]
    fn rejects_provider_without_capability() {
        let opts = RunOptions {
            provider: Some(ProviderId::Elevenlabs),
            ..Default::default()
        };
        let err = engine().candidates(&image(false), &opts).unwrap_err();
        assert_eq!(err.code(), "usage");
    }
}
