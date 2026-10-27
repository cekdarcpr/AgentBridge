use agentbridge_core::{RunFailure, RunResult};
use serde::Serialize;
use serde_json::{json, Value};

pub fn print_json(value: &impl Serialize) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("error: could not serialize output: {e}"),
    }
}

/// Adds `"ok": true` so every JSON response shares one envelope.
pub fn print_ok(value: &impl Serialize) {
    let mut value = serde_json::to_value(value).unwrap_or(Value::Null);
    match value.as_object_mut() {
        Some(map) => {
            map.insert("ok".into(), Value::Bool(true));
        }
        None => value = json!({"ok": true, "data": value}),
    }
    print_json(&value);
}

pub fn print_result(result: &RunResult, json: bool) {
    if json {
        print_ok(result);
        return;
    }
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }
    let approx = if result.cost_is_estimate { "~" } else { "" };
    if result.dry_run {
        eprintln!(
            "dry run: would use {}/{}, estimated {approx}${:.3}",
            result.provider, result.model, result.cost_usd
        );
        if let Some(path) = &result.output {
            println!("{path}");
        }
        return;
    }
    match (&result.output, &result.text) {
        (None, Some(text)) => println!("{text}"),
        (Some(path), _) => println!("{path}"),
        (None, None) => {}
    }
    let fallback = if result.fallback_used { " (fallback)" } else { "" };
    eprintln!("{}/{}{fallback} · {approx}${:.3}", result.provider, result.model, result.cost_usd);
}

pub fn print_failure(failure: &RunFailure, json: bool) {
    if json {
        print_json(&json!({
            "ok": false,
            "error": {"code": failure.error.code(), "message": failure.error.to_string()},
            "attempts": failure.attempts,
        }));
        return;
    }
    eprintln!("error: {}", failure.error);
    for attempt in &failure.attempts {
        let model = attempt.model.as_deref().map(|m| format!("/{m}")).unwrap_or_default();
        let reason = attempt.reason.as_deref().unwrap_or("");
        eprintln!("  {}{model}: {:?} {reason}", attempt.provider, attempt.status);
    }
}
