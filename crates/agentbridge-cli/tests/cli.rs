//! End-to-end checks that never reach a provider: dry runs, validation and budget
//! rejections all happen before any network request.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

use serde_json::Value;

const KEY_VARS: [&str; 3] = ["OPENAI_API_KEY", "GEMINI_API_KEY", "ELEVENLABS_API_KEY"];

struct Sandbox {
    home: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let home = std::env::temp_dir().join(format!("agentbridge-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        Self { home }
    }

    fn run(&self, args: &[&str], keys: &[(&str, &str)]) -> (Output, Value) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_agb"));
        cmd.args(args)
            .arg("--json")
            .current_dir(&self.home)
            .env("AGENTBRIDGE_HOME", &self.home)
            .env("AGENTBRIDGE_NO_KEYCHAIN", "1")
            .env("AGENTBRIDGE_PROJECT", "test-project");
        for var in KEY_VARS {
            cmd.env_remove(var);
        }
        for (var, value) in keys {
            cmd.env(var, value);
        }
        let output = cmd.output().unwrap();
        let json = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {}", String::from_utf8_lossy(&output.stdout)));
        (output, json)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[test]
fn dry_run_picks_first_provider_with_a_key() {
    let sandbox = Sandbox::new("dry-run");
    let (output, json) = sandbox.run(
        &["image", "sword icon", "--out", "assets/sword.png", "--dry-run"],
        &[("GEMINI_API_KEY", "test-gemini-key")],
    );
    assert!(output.status.success());
    assert_eq!(json["ok"], true);
    assert_eq!(json["provider"], "gemini");
    assert_eq!(json["fallback_used"], true);
    assert_eq!(json["attempts"][0]["status"], "skipped");
    assert!(json["output"].as_str().unwrap().ends_with("sword.png"));
}

#[test]
fn transparency_prefers_capable_provider_and_warns_otherwise() {
    let sandbox = Sandbox::new("transparent");
    let (_, json) = sandbox.run(
        &["image", "icon", "--out", "a.png", "--transparent", "--dry-run"],
        &[("GEMINI_API_KEY", "g-key-123456"), ("OPENAI_API_KEY", "o-key-123456")],
    );
    assert_eq!(json["provider"], "openai");

    let (_, json) = sandbox.run(
        &["image", "icon", "--out", "a.png", "--transparent", "--dry-run"],
        &[("GEMINI_API_KEY", "g-key-123456")],
    );
    assert_eq!(json["provider"], "gemini");
    assert!(json["warnings"][0].as_str().unwrap().contains("transparent"));
}

#[test]
fn missing_keys_report_no_provider() {
    let sandbox = Sandbox::new("no-keys");
    let (output, json) = sandbox.run(&["audio", "click", "--out", "click.wav"], &[]);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "no_provider");
    assert_eq!(json["attempts"][0]["provider"], "elevenlabs");
}

#[test]
fn budget_is_enforced_before_calling_the_provider() {
    let sandbox = Sandbox::new("budget");
    fs::write(sandbox.home.join("config.toml"), "[limits]\nmax_call_usd = 0.001\n").unwrap();
    let (output, json) = sandbox.run(
        &["image", "icon", "--out", "a.png"],
        &[("OPENAI_API_KEY", "o-key-123456")],
    );
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(json["error"]["code"], "budget_exceeded");
    assert!(!sandbox.home.join("a.png").exists());
}

#[test]
fn existing_output_requires_overwrite() {
    let sandbox = Sandbox::new("overwrite");
    fs::write(sandbox.home.join("a.png"), b"old").unwrap();
    let args = ["image", "icon", "--out", "a.png", "--dry-run"];
    let keys = [("OPENAI_API_KEY", "o-key-123456")];
    let (output, json) = sandbox.run(&args, &keys);
    assert_eq!(output.status.code(), Some(6));
    assert_eq!(json["error"]["code"], "output_error");

    let (output, _) = sandbox.run(&[&args[..], &["--overwrite"]].concat(), &keys);
    assert!(output.status.success());
}

#[test]
fn bad_arguments_still_produce_json() {
    let sandbox = Sandbox::new("bad-args");
    let (output, json) = sandbox.run(&["image", "icon", "--out", "a.gif"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(json["error"]["code"], "usage");

    let (output, json) = sandbox.run(&["audio", "x", "--out", "a.mp3", "--duration", "99"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(json["error"]["code"], "usage");
}

#[test]
fn providers_and_usage_report_state() {
    let sandbox = Sandbox::new("info");
    let (_, json) = sandbox.run(&["providers"], &[("OPENAI_API_KEY", "o-key-123456")]);
    let providers = json["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 3);
    assert_eq!(providers[0]["key"], "env");
    assert_eq!(providers[1]["key"], "missing");

    let (output, json) = sandbox.run(&["usage"], &[]);
    assert!(output.status.success());
    assert_eq!(json["today"]["spent_usd"], 0.0);
    assert_eq!(json["project"], "test-project");
}
