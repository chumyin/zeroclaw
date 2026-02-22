use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn zeroclaw_bin() -> String {
    if let Some(path) = option_env!("CARGO_BIN_EXE_zeroclaw") {
        return path.to_string();
    }
    std::env::var("CARGO_BIN_EXE_zeroclaw").expect(
        "CARGO_BIN_EXE_zeroclaw should be set (compile-time or runtime) for integration tests",
    )
}

fn run_json_command(config_dir: &Path, args: &[&str]) -> (Value, String, String) {
    let output = run_command(config_dir, args);

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid UTF-8");
    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        stdout,
        stderr
    );

    let json = serde_json::from_str::<Value>(&stdout).unwrap_or_else(|error| {
        panic!(
            "stdout should be pure JSON for args `{}`: {error}\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            stdout,
            stderr
        )
    });
    (json, stdout, stderr)
}

fn run_command(config_dir: &Path, args: &[&str]) -> Output {
    Command::new(zeroclaw_bin())
        .arg("--config-dir")
        .arg(config_dir)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to execute zeroclaw")
}

fn assert_report_contract(json: &Value, expected_report_type: &str) {
    assert_eq!(
        json.get("schema_version").and_then(Value::as_u64),
        Some(1),
        "missing/invalid schema_version: {json}"
    );
    assert_eq!(
        json.get("report_type").and_then(Value::as_str),
        Some(expected_report_type),
        "missing/invalid report_type: {json}"
    );
}

#[test]
fn onboard_dry_run_json_is_parseable_and_has_contract_fields() {
    let tmp = tempdir().expect("tempdir");
    let (json, _stdout, _stderr) = run_json_command(
        tmp.path(),
        &[
            "onboard",
            "--intent",
            "need unattended browser automation with updates",
            "--dry-run",
            "--json",
        ],
    );
    assert_report_contract(&json, "onboard.quick_dry_run");
    assert_eq!(
        json.get("mode").and_then(Value::as_str),
        Some("quick_dry_run")
    );
    assert!(
        json.get("consent_reason_keys").is_some(),
        "expected consent_reason_keys field in onboard json report"
    );
    assert!(
        json.get("warning_keys").is_some(),
        "expected warning_keys field in onboard json report"
    );
}

#[test]
fn preset_intent_json_is_parseable_and_has_contract_fields() {
    let tmp = tempdir().expect("tempdir");
    let (json, _stdout, _stderr) = run_json_command(
        tmp.path(),
        &[
            "preset",
            "intent",
            "need unattended browser automation",
            "--json",
        ],
    );
    assert_report_contract(&json, "preset.intent_orchestration");
    let next_commands = json
        .get("next_commands")
        .and_then(Value::as_array)
        .expect("expected next_commands array");
    assert!(
        !next_commands.is_empty(),
        "next_commands should include at least one generated command"
    );
    assert!(
        next_commands
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some("preset.apply")),
        "expected preset.apply in next_commands"
    );
    assert!(
        next_commands
            .iter()
            .filter(|entry| {
                entry
                    .get("requires_explicit_consent")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .all(|entry| {
                entry
                    .get("consent_reason_keys")
                    .and_then(Value::as_array)
                    .map(|keys| !keys.is_empty())
                    .unwrap_or(false)
            }),
        "commands that require explicit consent should include non-empty consent_reason_keys"
    );
}

#[test]
fn preset_validate_json_is_parseable_and_has_contract_fields() {
    let tmp = tempdir().expect("tempdir");
    let preset_path = tmp.path().join("good.preset.json");
    std::fs::write(
        &preset_path,
        r#"{
  "schema_version": 1,
  "id": "good-preset",
  "title": "Good Preset",
  "description": "Valid payload for machine-json contract test",
  "packs": ["core-agent", "browser-native"],
  "config_overrides": {},
  "metadata": { "author": "test" }
}"#,
    )
    .expect("write preset fixture");

    let preset_path_str = preset_path
        .to_str()
        .expect("preset path should be valid UTF-8")
        .to_string();
    let (json, _stdout, _stderr) = run_json_command(
        tmp.path(),
        &["preset", "validate", &preset_path_str, "--json"],
    );
    assert_report_contract(&json, "preset.validation");
    assert_eq!(json.get("files_checked").and_then(Value::as_u64), Some(1));
    assert_eq!(json.get("files_failed").and_then(Value::as_u64), Some(0));
}

#[test]
fn preset_export_json_is_parseable_and_has_contract_fields() {
    let tmp = tempdir().expect("tempdir");
    let export_path = tmp.path().join("exported.preset.json");
    let export_path_str = export_path
        .to_str()
        .expect("export path should be valid UTF-8")
        .to_string();
    let (json, _stdout, _stderr) = run_json_command(
        tmp.path(),
        &["preset", "export", &export_path_str, "--json"],
    );
    assert_report_contract(&json, "preset.export");
    assert_eq!(
        json.get("target_path").and_then(Value::as_str),
        Some(export_path_str.as_str())
    );
    assert_eq!(
        json.get("write_performed").and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        export_path.exists(),
        "expected exported preset payload at {}",
        export_path.display()
    );

    let payload = std::fs::read(&export_path).expect("read export payload");
    let expected_sha = format!("{:x}", Sha256::digest(&payload));
    let actual_sha = json
        .get("payload_sha256")
        .and_then(Value::as_str)
        .expect("payload_sha256 must be present");
    assert_eq!(actual_sha, expected_sha, "sha mismatch");
    assert_eq!(
        json.get("bytes_written").and_then(Value::as_u64),
        Some(payload.len() as u64)
    );
}

#[test]
fn preset_export_json_with_requested_preset_exposes_provenance() {
    let tmp = tempdir().expect("tempdir");
    let export_path = tmp.path().join("exported-official.preset.json");
    let export_path_str = export_path
        .to_str()
        .expect("export path should be valid UTF-8")
        .to_string();
    let (json, _stdout, _stderr) = run_json_command(
        tmp.path(),
        &[
            "preset",
            "export",
            &export_path_str,
            "--preset",
            "automation",
            "--json",
        ],
    );
    assert_report_contract(&json, "preset.export");
    assert_eq!(
        json.get("source_kind").and_then(Value::as_str),
        Some("official_preset")
    );
    assert_eq!(
        json.get("requested_preset").and_then(Value::as_str),
        Some("automation")
    );
}

#[test]
fn preset_apply_dry_run_json_is_parseable_and_has_contract_fields() {
    let tmp = tempdir().expect("tempdir");
    let (json, _stdout, _stderr) =
        run_json_command(tmp.path(), &["preset", "apply", "--dry-run", "--json"]);
    assert_report_contract(&json, "preset.apply_dry_run");
    assert!(
        json.get("selection_diff").is_some(),
        "expected selection_diff in preset apply dry-run report"
    );
}

#[test]
fn preset_import_dry_run_json_is_parseable_and_has_contract_fields() {
    let tmp = tempdir().expect("tempdir");
    let preset_path = tmp.path().join("importable.preset.json");
    std::fs::write(
        &preset_path,
        r#"{
  "schema_version": 1,
  "id": "importable-preset",
  "title": "Importable Preset",
  "description": "Valid payload for import dry-run contract test",
  "packs": ["core-agent", "browser-native"],
  "config_overrides": {},
  "metadata": { "author": "test" }
}"#,
    )
    .expect("write import fixture");

    let preset_path_str = preset_path
        .to_str()
        .expect("preset path should be valid UTF-8")
        .to_string();
    let (json, _stdout, _stderr) = run_json_command(
        tmp.path(),
        &[
            "preset",
            "import",
            &preset_path_str,
            "--mode",
            "merge",
            "--dry-run",
            "--json",
        ],
    );
    assert_report_contract(&json, "preset.import_dry_run");
    assert_eq!(
        json.get("import_mode").and_then(Value::as_str),
        Some("merge")
    );
}

#[test]
fn preset_apply_json_requires_dry_run() {
    let tmp = tempdir().expect("tempdir");
    let output = run_command(tmp.path(), &["preset", "apply", "--json"]);
    assert!(
        !output.status.success(),
        "command should fail without --dry-run"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid UTF-8");
    assert!(
        stderr.contains("`preset apply --json` requires `--dry-run`."),
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn preset_import_json_requires_dry_run() {
    let tmp = tempdir().expect("tempdir");
    let preset_path = tmp.path().join("importable.preset.json");
    std::fs::write(
        &preset_path,
        r#"{
  "schema_version": 1,
  "id": "importable-preset",
  "packs": ["core-agent"]
}"#,
    )
    .expect("write import fixture");
    let preset_path_str = preset_path
        .to_str()
        .expect("preset path should be valid UTF-8")
        .to_string();
    let output = run_command(
        tmp.path(),
        &["preset", "import", &preset_path_str, "--json"],
    );
    assert!(
        !output.status.success(),
        "command should fail without --dry-run"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid UTF-8");
    assert!(
        stderr.contains("`preset import --json` requires `--dry-run`."),
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn security_profile_set_json_with_export_diff_keeps_stdout_json() {
    let tmp = tempdir().expect("tempdir");
    let export_path = tmp.path().join("security-diff.json");
    let export_path_str = export_path
        .to_str()
        .expect("export path should be valid UTF-8")
        .to_string();

    let (json, _stdout, stderr) = run_json_command(
        tmp.path(),
        &[
            "security",
            "profile",
            "set",
            "balanced",
            "--dry-run",
            "--json",
            "--export-diff",
            &export_path_str,
        ],
    );
    assert_report_contract(&json, "security.profile_change");
    assert_eq!(json.get("dry_run").and_then(Value::as_bool), Some(true));
    assert!(
        export_path.exists(),
        "export diff file should be created at {}",
        export_path.display()
    );
    assert!(
        stderr.contains("Exported security diff:"),
        "expected export notice on stderr in json mode, got:\n{stderr}"
    );
}

#[test]
fn security_profile_recommend_json_is_parseable_and_has_contract_fields() {
    let tmp = tempdir().expect("tempdir");
    let (json, _stdout, _stderr) = run_json_command(
        tmp.path(),
        &[
            "security",
            "profile",
            "recommend",
            "need unattended browser automation",
            "--json",
        ],
    );
    assert_report_contract(&json, "security.profile_recommendation");
    assert!(
        json.get("apply_consent_reason_keys").is_some(),
        "expected apply_consent_reason_keys field in recommendation report"
    );
}
