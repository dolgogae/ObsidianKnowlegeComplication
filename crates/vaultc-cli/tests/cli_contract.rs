use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use vaultc::plan::DraftPlan;
use vaultc::{SourceSpec, VaultCompiler};

const EXIT_USAGE: i32 = 2;
const EXIT_INPUT: i32 = 3;
const EXIT_DECISION: i32 = 4;
const EXIT_PROVIDER: i32 = 5;
const EXIT_OUTPUT: i32 = 6;
const EXIT_VERIFY: i32 = 7;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_vaultc")
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn run(arguments: &[&str]) -> Output {
    Command::new(binary())
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run vaultc {arguments:?}: {error}"))
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn as_utf8(path: &Path) -> &str {
    path.to_str()
        .expect("temporary and fixture paths are UTF-8")
}

#[cfg(unix)]
fn wait_for_file(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        path.is_file(),
        "file was not created in time: {}",
        path.display()
    );
}

#[cfg(unix)]
fn assert_process_reaped(pid: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let alive = Command::new("/bin/kill")
            .args(["-0", pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !alive {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "provider process {pid} was not reaped"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn cli_plan_matches_sdk_and_full_artifact_lifecycle() {
    let temporary = tempfile::tempdir().expect("temporary CLI workspace");
    let source = fixture("basic_vault");
    let source_argument = format!("basic={}", source.display());
    let workspace = temporary.path().join("workspace.sqlite");
    let plan_path = temporary.path().join("plan.json");

    let output = run(&[
        "--workspace",
        as_utf8(&workspace),
        "plan",
        &source_argument,
        "--out",
        as_utf8(&plan_path),
        "--format",
        "json",
    ]);
    assert_exit(&output, 0);

    let cli_plan: DraftPlan =
        serde_json::from_slice(&fs::read(&plan_path).expect("read plan emitted by CLI"))
            .expect("decode CLI plan");
    let compiler = VaultCompiler::builder()
        .build()
        .expect("build SDK compiler");
    let sdk_inspection = compiler
        .inspect([SourceSpec::directory("basic", &source).expect("SDK source descriptor")])
        .expect("SDK inspection");
    let sdk_plan = compiler.plan(&sdk_inspection).expect("SDK plan");
    assert_eq!(cli_plan, sdk_plan, "CLI and SDK must seal the same plan");

    let decisions_path = temporary.path().join("decisions.json");
    fs::write(
        &decisions_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "plan_id": cli_plan.plan_id.to_string(),
            "decisions": [],
            "conflicts": []
        }))
        .expect("encode decisions"),
    )
    .expect("write decisions");
    let approved_path = temporary.path().join("approved.json");
    let output = run(&[
        "approve",
        as_utf8(&plan_path),
        "--decisions",
        as_utf8(&decisions_path),
        "--out",
        as_utf8(&approved_path),
    ]);
    assert_exit(&output, 0);

    let compiled_vault = temporary.path().join("compiled");
    let pack = temporary.path().join("compiled.vaultpack");
    let output = run(&[
        "compile",
        as_utf8(&approved_path),
        "--output",
        as_utf8(&compiled_vault),
        "--pack",
        as_utf8(&pack),
        "--format",
        "json",
    ]);
    assert_exit(&output, 0);
    assert!(compiled_vault.join(".vaultc/manifest.json").is_file());
    assert!(pack.is_file());

    for artifact in [&compiled_vault, &pack] {
        let output = run(&["verify", as_utf8(artifact), "--format", "json"]);
        assert_exit(&output, 0);
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("decode verification JSON");
        assert_eq!(report["valid"], true);
        assert_eq!(report["artifact_path"], as_utf8(artifact));

        let output = run(&[
            "explain",
            as_utf8(artifact),
            "knowledge/Index.md",
            "--format",
            "json",
        ]);
        assert_exit(&output, 0);
        let explanation: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("decode provenance JSON");
        assert_eq!(explanation["output_path"], "knowledge/Index.md");
        assert_eq!(explanation["records"].as_array().map(Vec::len), Some(1));
    }

    let output = run(&[
        "compile",
        as_utf8(&approved_path),
        "--output",
        as_utf8(&compiled_vault),
    ]);
    assert_exit(&output, EXIT_OUTPUT);
}

#[test]
fn cli_exit_codes_follow_the_public_contract() {
    let temporary = tempfile::tempdir().expect("temporary exit-code workspace");

    assert_exit(&run(&[]), EXIT_USAGE);

    let missing_source = temporary.path().join("missing-vault");
    let source_argument = format!("missing={}", missing_source.display());
    assert_exit(&run(&["inspect", &source_argument]), EXIT_INPUT);

    let missing_plan = temporary.path().join("missing-plan.json");
    let augmentation = temporary.path().join("augmentation.jsonl");
    assert_exit(
        &run(&[
            "augment",
            as_utf8(&missing_plan),
            "--provider-cmd",
            "/bin/false",
            "--all-documents",
            "--out",
            as_utf8(&augmentation),
        ]),
        EXIT_PROVIDER,
    );

    let invalid_artifact = temporary.path().join("not-an-artifact");
    fs::write(&invalid_artifact, b"not a vaultpack").expect("write invalid artifact");
    assert_exit(&run(&["verify", as_utf8(&invalid_artifact)]), EXIT_VERIFY);
}

#[test]
fn cli_reports_unresolved_required_conflicts_as_decision_exit() {
    let temporary = tempfile::tempdir().expect("temporary decision workspace");
    let source = temporary.path().join("ambiguous-vault");
    fs::create_dir_all(source.join("one")).expect("create first topic directory");
    fs::create_dir_all(source.join("two")).expect("create second topic directory");
    fs::write(source.join("Index.md"), "Read [[Topic]].\n").expect("write ambiguous link");
    fs::write(source.join("one/Topic.md"), "# First Topic\n").expect("write first topic");
    fs::write(source.join("two/Topic.md"), "# Second Topic\n").expect("write second topic");
    let source_argument = format!("ambiguous={}", source.display());
    let plan_path = temporary.path().join("plan.json");
    assert_exit(
        &run(&["plan", &source_argument, "--out", as_utf8(&plan_path)]),
        EXIT_DECISION,
    );

    let plan: DraftPlan =
        serde_json::from_slice(&fs::read(&plan_path).expect("read plan requiring a decision"))
            .expect("decode plan requiring a decision");
    assert_eq!(plan.unresolved_required_conflicts().count(), 1);

    let conflict = plan
        .unresolved_required_conflicts()
        .next()
        .expect("required conflict");
    let decisions = temporary.path().join("conflict-decisions.json");
    fs::write(
        &decisions,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "plan_id": plan.plan_id.to_string(),
            "decisions": [],
            "conflicts": [{
                "plan_id": plan.plan_id.to_string(),
                "conflict_id": conflict.conflict_id,
                "conflict_content_hash": conflict.content_hash,
                "resolution": "waived_by_policy",
                "resolver": "cli-integration-test",
                "policy_version": "test-v1",
                "rationale": "preserve the unresolved source link under explicit test policy"
            }]
        }))
        .expect("encode conflict decisions"),
    )
    .expect("write conflict decisions");
    let approved_path = temporary.path().join("conflict-approved.json");
    assert_exit(
        &run(&[
            "approve",
            as_utf8(&plan_path),
            "--decisions",
            as_utf8(&decisions),
            "--out",
            as_utf8(&approved_path),
        ]),
        0,
    );
    let approved: vaultc::ApprovedPlan =
        serde_json::from_slice(&fs::read(&approved_path).expect("read conflict-approved plan"))
            .expect("decode conflict-approved plan");
    assert_eq!(approved.plan.plan_id, plan.plan_id);
    assert_eq!(approved.plan.conflicts, plan.conflicts);
    assert_eq!(approved.conflict_decisions.len(), 1);
    assert!(approved.conflict_decisions_complete);
}

#[cfg(unix)]
#[test]
fn cli_round_trips_a_redacted_command_provider_transcript() {
    let temporary = tempfile::tempdir().expect("temporary provider workspace");
    let source = fixture("basic_vault");
    let source_argument = format!("basic={}", source.display());
    let plan_path = temporary.path().join("plan.json");
    assert_exit(
        &run(&["plan", &source_argument, "--out", as_utf8(&plan_path)]),
        0,
    );

    let provider = temporary.path().join("provider.sh");
    fs::write(
        &provider,
        r#"#!/bin/sh
IFS= read -r capabilities_request || exit 20
printf '%s\n' '{"protocol_version":1,"request_id":"capabilities-1","message_type":"capabilities_response","payload":{"provider":{"provider":"fixture","model":"empty","version":"1"},"protocol_versions":[1],"operations":["knowledge_augmentation"],"max_input_bytes":10485760,"max_output_bytes":10485760,"structured_output":true,"streaming":false,"deterministic_controls":true,"data_boundary":{"kind":"local"}}}'
IFS= read -r augmentation_request || exit 21
printf '%s\n' '{"protocol_version":1,"request_id":"augmentation-1","message_type":"augmentation_response","payload":{"proposals":[]}}'
"#,
    )
    .expect("write provider fixture");

    let augmentation = temporary.path().join("augmentation.jsonl");
    let output = run(&[
        "augment",
        as_utf8(&plan_path),
        "--provider-cmd",
        "/bin/sh",
        "--provider-arg",
        as_utf8(&provider),
        "--all-documents",
        "--out",
        as_utf8(&augmentation),
    ]);
    assert_exit(&output, 0);
    let augmentation_text =
        fs::read_to_string(&augmentation).expect("read augmentation transcript");
    assert!(augmentation_text.contains("[redacted]"));
    assert!(!augmentation_text.contains("Welcome to the fixture"));

    let plan: DraftPlan =
        serde_json::from_slice(&fs::read(&plan_path).expect("read provider plan"))
            .expect("decode provider plan");
    let decisions = temporary.path().join("decisions.json");
    fs::write(
        &decisions,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "plan_id": plan.plan_id.to_string(),
            "decisions": [],
            "conflicts": []
        }))
        .expect("encode provider decisions"),
    )
    .expect("write provider decisions");
    let approved = temporary.path().join("approved.json");
    assert_exit(
        &run(&[
            "approve",
            as_utf8(&plan_path),
            "--decisions",
            as_utf8(&decisions),
            "--proposals",
            as_utf8(&augmentation),
            "--out",
            as_utf8(&approved),
        ]),
        0,
    );
}

#[cfg(unix)]
#[test]
fn cli_cancels_blocked_provider_input_and_reaps_its_process_group() {
    let temporary = tempfile::tempdir().expect("temporary cancellation workspace");
    let source = temporary.path().join("large-vault");
    fs::create_dir(&source).expect("create large source Vault");
    let mut note = String::from("# Large projection\n\n");
    note.push_str(&"x".repeat(2 * 1024 * 1024));
    note.push('\n');
    fs::write(source.join("Large.md"), note).expect("write large source note");

    let plan = temporary.path().join("large-plan.json");
    let source_argument = format!("large={}", source.display());
    assert_exit(
        &run(&["plan", &source_argument, "--out", as_utf8(&plan)]),
        0,
    );

    let provider = temporary.path().join("blocked-provider.sh");
    fs::write(
        &provider,
        r#"#!/bin/sh
printf '%s\n' "$$" > "$1"
IFS= read -r capabilities_request || exit 20
printf '%s\n' '{"protocol_version":1,"request_id":"capabilities-1","message_type":"capabilities_response","payload":{"provider":{"provider":"fixture","model":"blocked","version":"1"},"protocol_versions":[1],"operations":["knowledge_augmentation"],"max_input_bytes":16777216,"max_output_bytes":1048576,"structured_output":true,"streaming":false,"deterministic_controls":true,"data_boundary":{"kind":"local"}}}'
sleep 30
"#,
    )
    .expect("write blocked provider fixture");
    let provider_pid_file = temporary.path().join("provider.pid");
    let augmentation = temporary.path().join("cancelled.jsonl");
    let started = Instant::now();
    let child = Command::new(binary())
        .args([
            "augment",
            as_utf8(&plan),
            "--provider-cmd",
            "/bin/sh",
            "--provider-arg",
            as_utf8(&provider),
            "--provider-arg",
            as_utf8(&provider_pid_file),
            "--provider-timeout-seconds",
            "5",
            "--all-documents",
            "--out",
            as_utf8(&augmentation),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cancellable vaultc");

    wait_for_file(&provider_pid_file, Duration::from_secs(2));
    let provider_pid = fs::read_to_string(&provider_pid_file)
        .expect("provider published its PID")
        .trim()
        .to_owned();
    thread::sleep(Duration::from_millis(200));
    let signal_status = Command::new("/bin/kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("signal vaultc");
    assert!(signal_status.success());

    let output = child.wait_with_output().expect("wait for cancelled vaultc");
    assert_exit(&output, EXIT_PROVIDER);
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "cancellation exceeded the bounded shutdown window: {:?}",
        started.elapsed()
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("cancelled"));

    assert_process_reaped(&provider_pid, Duration::from_secs(2));
    assert!(!augmentation.exists());

    let deadline_pid_file = temporary.path().join("deadline-provider.pid");
    let deadline_output = temporary.path().join("deadline.jsonl");
    let deadline_started = Instant::now();
    let output = run(&[
        "augment",
        as_utf8(&plan),
        "--provider-cmd",
        "/bin/sh",
        "--provider-arg",
        as_utf8(&provider),
        "--provider-arg",
        as_utf8(&deadline_pid_file),
        "--provider-timeout-seconds",
        "1",
        "--all-documents",
        "--out",
        as_utf8(&deadline_output),
    ]);
    assert_exit(&output, EXIT_PROVIDER);
    assert!(
        deadline_started.elapsed() < Duration::from_secs(3),
        "deadline exceeded the bounded shutdown window: {:?}",
        deadline_started.elapsed()
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("deadline"));
    wait_for_file(&deadline_pid_file, Duration::from_secs(2));
    let deadline_provider_pid = fs::read_to_string(&deadline_pid_file)
        .expect("deadline provider published its PID")
        .trim()
        .to_owned();
    assert_process_reaped(&deadline_provider_pid, Duration::from_secs(2));
    assert!(!deadline_output.exists());
}
