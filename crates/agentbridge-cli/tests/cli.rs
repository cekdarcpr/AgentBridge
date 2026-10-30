//! End-to-end checks that never reach a provider: dry runs, validation and budget
//! rejections all happen before any network request.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

use serde_json::Value;

const KEY_VARS: [&str; 4] = [
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "ELEVENLABS_API_KEY",
    "OPENROUTER_API_KEY",
];

struct Sandbox {
    home: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let home =
            std::env::temp_dir().join(format!("agentbridge-test-{}-{name}", std::process::id()));
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
        let json = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
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
        &[
            "image",
            "sword icon",
            "--out",
            "assets/sword.png",
            "--dry-run",
        ],
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
fn transparency_works_with_every_image_provider() {
    let sandbox = Sandbox::new("transparent");
    let args = [
        "image",
        "icon",
        "--out",
        "a.png",
        "--transparent",
        "--trim",
        "--dry-run",
    ];
    let (_, json) = sandbox.run(
        &args,
        &[
            ("GEMINI_API_KEY", "g-key-123456"),
            ("OPENAI_API_KEY", "o-key-123456"),
        ],
    );
    assert_eq!(
        json["provider"], "openai",
        "routing order decides, not transparency"
    );

    let (output, json) = sandbox.run(&args, &[("GEMINI_API_KEY", "g-key-123456")]);
    assert!(output.status.success());
    assert_eq!(json["provider"], "gemini");
    assert_eq!(json["warnings"].as_array().unwrap().len(), 0);
}

#[test]
fn transparent_jpeg_and_trim_without_transparency_are_rejected() {
    let sandbox = Sandbox::new("jpeg");
    let keys = [("OPENAI_API_KEY", "o-key-123456")];
    let (output, json) = sandbox.run(
        &[
            "image",
            "icon",
            "--out",
            "a.jpg",
            "--transparent",
            "--dry-run",
        ],
        &keys,
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(json["error"]["message"].as_str().unwrap().contains("JPEG"));

    let (output, _) = sandbox.run(
        &["image", "icon", "--out", "a.png", "--trim", "--dry-run"],
        &keys,
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn openrouter_is_selected_by_vendor_model_ids() {
    let sandbox = Sandbox::new("openrouter");
    let (output, json) = sandbox.run(
        &[
            "image",
            "icon",
            "--out",
            "a.webp",
            "--model",
            "google/gemini-2.5-flash-image",
            "--dry-run",
        ],
        &[("OPENROUTER_API_KEY", "or-key-123456")],
    );
    assert!(output.status.success());
    assert_eq!(json["provider"], "openrouter");
    assert_eq!(json["cost_usd"], 0.039);

    // Without other keys, OpenRouter serves as the last fallback in the default routing.
    let (_, json) = sandbox.run(
        &["ask", "hello", "--dry-run"],
        &[("OPENROUTER_API_KEY", "or-key-123456")],
    );
    assert_eq!(json["provider"], "openrouter");
    assert_eq!(json["model"], "google/gemini-2.5-flash");
}

/// A 200x100 blue rectangle on a flat magenta backdrop, like a keyed provider output.
fn write_sprite(path: &std::path::Path) {
    let img = image::RgbaImage::from_fn(200, 100, |x, y| {
        let inside = (60..140).contains(&x) && (25..75).contains(&y);
        image::Rgba(if inside {
            [20, 40, 200, 255]
        } else {
            [255, 0, 255, 255]
        })
    });
    img.save(path).unwrap();
}

#[test]
fn process_resizes_converts_and_removes_backgrounds_locally() {
    let sandbox = Sandbox::new("process");
    write_sprite(&sandbox.home.join("raw.png"));
    let (output, json) = sandbox.run(
        &[
            "process",
            "raw.png",
            "--out",
            "out/sprite.webp",
            "--size",
            "64x64",
            "--transparent",
            "--trim",
        ],
        &[],
    );
    assert!(output.status.success(), "{json}");
    assert_eq!(
        (json["width"].as_u64(), json["height"].as_u64()),
        (Some(64), Some(64))
    );

    let result = image::open(sandbox.home.join("out/sprite.webp"))
        .unwrap()
        .to_rgba8();
    assert_eq!(result.dimensions(), (64, 64));
    assert_eq!(
        result.get_pixel(0, 0)[3],
        0,
        "corner is transparent padding"
    );
    assert_eq!(result.get_pixel(32, 32)[3], 255, "subject stays opaque");

    let (output, json) = sandbox.run(&["process", "raw.png", "--out", "out/sprite.webp"], &[]);
    assert_eq!(output.status.code(), Some(6), "{json}");
}

#[test]
fn icon_set_from_an_existing_image() {
    let sandbox = Sandbox::new("icons");
    write_sprite(&sandbox.home.join("logo.png"));
    let (output, json) = sandbox.run(
        &[
            "icon-set",
            "--from",
            "logo.png",
            "--out-dir",
            "public",
            "--preset",
            "web",
        ],
        &[],
    );
    assert!(output.status.success(), "{json}");
    assert_eq!(json["files"].as_array().unwrap().len(), 7);
    assert!(json["html"].as_str().unwrap().contains("favicon.ico"));
    assert!(json.get("generation").is_none());
    for name in [
        "favicon.ico",
        "favicon-32x32.png",
        "apple-touch-icon.png",
        "android-chrome-512x512.png",
        "site.webmanifest",
    ] {
        assert!(
            sandbox.home.join("public").join(name).exists(),
            "{name} missing"
        );
    }

    let (output, _) = sandbox.run(
        &["icon-set", "--from", "logo.png", "--out-dir", "public"],
        &[],
    );
    assert_eq!(
        output.status.code(),
        Some(6),
        "existing icons need --overwrite"
    );
}

#[test]
fn icon_set_dry_run_plans_the_generation() {
    let sandbox = Sandbox::new("icon-plan");
    let (output, json) = sandbox.run(
        &[
            "icon-set",
            "rocket",
            "--out-dir",
            "icons",
            "--preset",
            "app",
            "--dry-run",
        ],
        &[("OPENAI_API_KEY", "o-key-123456")],
    );
    assert!(output.status.success(), "{json}");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["generation"]["provider"], "openai");
    assert!(json["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str().unwrap().ends_with("icon.ico")));
    assert!(
        !sandbox.home.join("icons").exists(),
        "a dry run writes nothing"
    );
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
    fs::write(
        sandbox.home.join("config.toml"),
        "[limits]\nmax_call_usd = 0.001\n",
    )
    .unwrap();
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
    assert_eq!(providers.len(), 4);
    assert_eq!(providers[0]["key"], "env");
    assert_eq!(providers[1]["key"], "missing");

    let (output, json) = sandbox.run(&["usage"], &[]);
    assert!(output.status.success());
    assert_eq!(json["today"]["spent_usd"], 0.0);
    assert_eq!(json["project"], "test-project");
}

#[test]
fn project_style_is_applied_and_reported() {
    let sandbox = Sandbox::new("style");
    let (output, json) = sandbox.run(&["style", "show"], &[]);
    assert!(output.status.success());
    assert!(json["file"].is_null());

    let (output, _) = sandbox.run(&["style", "init"], &[]);
    assert!(output.status.success());
    let (output, json) = sandbox.run(&["style", "init"], &[]);
    assert_eq!(output.status.code(), Some(6));
    assert_eq!(json["error"]["code"], "output_error");

    let (_, json) = sandbox.run(&["style", "show"], &[]);
    assert!(json["style"]["description"]
        .as_str()
        .unwrap()
        .contains("flat vector"));

    let key = [("GEMINI_API_KEY", "test-gemini-key")];
    let (output, json) = sandbox.run(&["image", "a key", "--out", "key.png", "--dry-run"], &key);
    assert!(output.status.success());
    assert!(json["style"]
        .as_str()
        .unwrap()
        .ends_with("agentbridge.toml"));

    let args = [
        "image",
        "a key",
        "--out",
        "key.png",
        "--dry-run",
        "--no-style",
    ];
    let (_, json) = sandbox.run(&args, &key);
    assert!(json.get("style").is_none());
}

#[test]
fn speech_falls_back_to_openrouter() {
    let sandbox = Sandbox::new("speech-openrouter");
    let (output, json) = sandbox.run(
        &["speech", "Welcome back", "--out", "hello.wav", "--dry-run"],
        &[("OPENROUTER_API_KEY", "test-openrouter-key")],
    );
    assert!(output.status.success());
    assert_eq!(json["provider"], "openrouter");
    assert_eq!(json["model"], "google/gemini-3.8-flash-tts");
}

#[test]
fn config_set_changes_the_routing() {
    let sandbox = Sandbox::new("config-set");
    let keys = [
        ("OPENAI_API_KEY", "test-openai-key"),
        ("OPENROUTER_API_KEY", "test-openrouter-key"),
    ];
    let (output, json) = sandbox.run(
        &["config", "set", "routing.image", "openrouter,openai"],
        &[],
    );
    assert!(output.status.success(), "{json}");
    assert_eq!(json["config"]["routing"]["image"][0], "openrouter");

    let (_, json) = sandbox.run(
        &["image", "sword", "--out", "sword.png", "--dry-run"],
        &keys,
    );
    assert_eq!(json["provider"], "openrouter");

    let (output, json) = sandbox.run(&["config", "set", "routing.audio", "openai"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(json["error"]["code"], "config_error");
}

#[test]
fn agent_init_writes_the_instructions_once() {
    let sandbox = Sandbox::new("agent-init");
    fs::write(sandbox.home.join("CLAUDE.md"), "# Project notes\n").unwrap();

    let (output, json) = sandbox.run(&["agent", "init"], &[]);
    assert!(output.status.success(), "{json}");
    assert_eq!(json["files"][0]["change"], "added");
    assert_eq!(json["files"].as_array().unwrap().len(), 1);
    let written = fs::read_to_string(sandbox.home.join("CLAUDE.md")).unwrap();
    assert!(written.starts_with("# Project notes\n"));
    assert!(written.contains("agb image"));

    let (_, json) = sandbox.run(&["agent", "init"], &[]);
    assert_eq!(json["files"][0]["change"], "unchanged");
    assert_eq!(
        fs::read_to_string(sandbox.home.join("CLAUDE.md")).unwrap(),
        written
    );
}
