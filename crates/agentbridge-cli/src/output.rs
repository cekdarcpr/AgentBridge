use agentbridge_core::{RunFailure, RunResult};
use serde::Serialize;
use serde_json::{json, Value};

use crate::assets::{IconSetPlan, IconSetResult, ProcessResult};

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
    let fallback = if result.fallback_used {
        " (fallback)"
    } else {
        ""
    };
    let size = match (result.width, result.height) {
        (Some(w), Some(h)) => format!(" · {w}x{h}"),
        _ => String::new(),
    };
    eprintln!(
        "{}/{}{fallback} · {approx}${:.3}{size}",
        result.provider, result.model, result.cost_usd
    );
}

fn print_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

pub fn print_process(result: &ProcessResult, json: bool) {
    if json {
        print_ok(result);
        return;
    }
    print_warnings(&result.warnings);
    println!("{}", result.output);
    eprintln!("{}x{} · local, no cost", result.width, result.height);
}

pub fn print_icon_set(result: &IconSetResult, json: bool) {
    if json {
        print_ok(result);
        return;
    }
    if let Some(generation) = &result.generation {
        print_warnings(&generation.warnings);
        let approx = if generation.cost_is_estimate { "~" } else { "" };
        eprintln!(
            "generated with {}/{} · {approx}${:.3}",
            generation.provider, generation.model, generation.cost_usd
        );
    }
    print_warnings(&result.set.warnings);
    for file in &result.set.files {
        println!("{file}");
    }
    if let Some(html) = &result.set.html {
        eprintln!("\nAdd to the page <head>:");
        println!("{html}");
    }
}

pub fn print_icon_plan(plan: &IconSetPlan, json: bool) {
    if json {
        print_ok(plan);
        return;
    }
    match &plan.generation {
        Some(g) => eprintln!(
            "dry run: would generate the master with {}/{}, estimated ~${:.3}",
            g.provider, g.model, g.cost_usd
        ),
        None => eprintln!(
            "dry run: would build the {} icon set from the given image (no cost)",
            plan.preset
        ),
    }
    for file in &plan.files {
        println!("{file}");
    }
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
        let model = attempt
            .model
            .as_deref()
            .map(|m| format!("/{m}"))
            .unwrap_or_default();
        let reason = attempt.reason.as_deref().unwrap_or("");
        eprintln!(
            "  {}{model}: {:?} {reason}",
            attempt.provider, attempt.status
        );
    }
}
