#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use vaultc::{DraftPlan, RecordedAugmentation, VaultCompiler};

const EXIT_PROVIDER: i32 = 5;
const EXIT_OUTPUT: i32 = 6;
const LOCAL_CAPABILITIES_RESPONSE: &str = r#"{"protocol_version":1,"request_id":"capabilities-1","message_type":"capabilities_response","payload":{"provider":{"provider":"cli-replay-fixture","model":"strict-wire","version":"1"},"protocol_versions":[1],"operations":["knowledge_augmentation"],"max_input_bytes":10485760,"max_output_bytes":10485760,"structured_output":true,"streaming":false,"deterministic_controls":true,"data_boundary":{"kind":"local"}}}"#;

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

fn provider_with_response(response: &str) -> String {
    format!(
        "#!/bin/sh\nIFS= read -r capabilities_request || exit 20\nprintf '%s\\n' '{LOCAL_CAPABILITIES_RESPONSE}'\nIFS= read -r augmentation_request || exit 21\nprintf '%s\\n' '{response}'\n"
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn cli_replay_matches_sdk_and_never_invokes_a_provider() {
    let temporary = tempfile::tempdir().expect("temporary replay workspace");
    let source = fixture("augmentation_replay");
    let source_argument = format!("replay={}", source.display());
    let workspace = temporary.path().join("workspace.sqlite");
    let plan_path = temporary.path().join("plan.json");
    assert_exit(
        &run(&[
            "--workspace",
            as_utf8(&workspace),
            "plan",
            &source_argument,
            "--out",
            as_utf8(&plan_path),
        ]),
        0,
    );

    let provider = temporary.path().join("local-provider.sh");
    fs::write(
        &provider,
        r#"#!/bin/sh
IFS= read -r capabilities_request || exit 20
printf '%s\n' '{"protocol_version":1,"request_id":"capabilities-1","message_type":"capabilities_response","payload":{"provider":{"provider":"cli-replay-fixture","model":"local-zero","version":"1"},"protocol_versions":[1],"operations":["knowledge_augmentation"],"max_input_bytes":10485760,"max_output_bytes":10485760,"structured_output":true,"streaming":false,"deterministic_controls":true,"data_boundary":{"kind":"local"}}}'
IFS= read -r augmentation_request || exit 21
printf '%s\n' invoked > "$1"
printf '%s\n' '{"protocol_version":1,"request_id":"augmentation-1","message_type":"augmentation_response","payload":{"proposals":[]}}'
"#,
    )
    .expect("write local provider fixture");
    let marker = temporary.path().join("provider-invoked");
    let augmentation = temporary.path().join("augmentation.jsonl");
    assert_exit(
        &run(&[
            "augment",
            as_utf8(&plan_path),
            "--provider-cmd",
            "/bin/sh",
            "--provider-arg",
            as_utf8(&provider),
            "--provider-arg",
            as_utf8(&marker),
            "--all-documents",
            "--out",
            as_utf8(&augmentation),
        ]),
        0,
    );
    assert_eq!(
        fs::read_to_string(&marker).expect("provider marker"),
        "invoked\n"
    );

    let input = fs::read(&augmentation).expect("read CLI recording");
    let plan: DraftPlan =
        serde_json::from_slice(&fs::read(&plan_path).expect("read plan for SDK replay parity"))
            .expect("decode plan for SDK replay parity");
    let compiler = VaultCompiler::builder()
        .policy(plan.policy.clone())
        .build()
        .expect("build SDK replay compiler");
    let decoded = RecordedAugmentation::from_canonical_jsonl(&input)
        .expect("decode CLI canonical recording through the SDK");
    let sdk_bytes = compiler
        .replay_augmentation(&plan, &decoded)
        .expect("SDK offline replay")
        .to_canonical_jsonl()
        .expect("encode SDK replay");
    assert_eq!(sdk_bytes, input, "SDK replay must preserve CLI bytes");

    fs::remove_file(&provider).expect("remove provider before offline replay");
    let replayed = temporary.path().join("replayed.jsonl");
    assert_exit(
        &run(&[
            "replay",
            as_utf8(&plan_path),
            "--augmentation",
            as_utf8(&augmentation),
            "--out",
            as_utf8(&replayed),
        ]),
        0,
    );
    assert_eq!(fs::read(&replayed).expect("read replay output"), input);
    assert_eq!(
        fs::read_to_string(&marker).expect("provider marker"),
        "invoked\n"
    );

    assert_exit(
        &run(&[
            "replay",
            as_utf8(&plan_path),
            "--augmentation",
            as_utf8(&augmentation),
            "--out",
            as_utf8(&replayed),
        ]),
        EXIT_OUTPUT,
    );
    assert_eq!(fs::read(&replayed).expect("unchanged replay output"), input);

    let noncanonical = temporary.path().join("noncanonical.jsonl");
    let mut crlf = Vec::with_capacity(input.len() * 2);
    for byte in &input {
        if *byte == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(*byte);
    }
    fs::write(&noncanonical, crlf).expect("write noncanonical recording");
    let rejected = temporary.path().join("rejected.jsonl");
    assert_exit(
        &run(&[
            "replay",
            as_utf8(&plan_path),
            "--augmentation",
            as_utf8(&noncanonical),
            "--out",
            as_utf8(&rejected),
        ]),
        EXIT_PROVIDER,
    );
    assert!(!rejected.exists(), "failed replay must not publish output");

    let other_plan = temporary.path().join("other-plan.json");
    let other_source_argument = format!("other={}", source.display());
    assert_exit(
        &run(&[
            "--workspace",
            as_utf8(&temporary.path().join("other.sqlite")),
            "plan",
            &other_source_argument,
            "--out",
            as_utf8(&other_plan),
        ]),
        0,
    );
    let stale_output = temporary.path().join("stale.jsonl");
    assert_exit(
        &run(&[
            "replay",
            as_utf8(&other_plan),
            "--augmentation",
            as_utf8(&augmentation),
            "--out",
            as_utf8(&stale_output),
        ]),
        EXIT_PROVIDER,
    );
    assert!(
        !stale_output.exists(),
        "stale replay must not publish output"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn cli_remote_provider_requires_policy_and_runtime_consent() {
    let temporary = tempfile::tempdir().expect("temporary remote-consent workspace");
    let source = fixture("augmentation_replay");
    let source_argument = format!("remote={}", source.display());

    let default_plan = temporary.path().join("default-plan.json");
    assert_exit(
        &run(&[
            "--workspace",
            as_utf8(&temporary.path().join("default.sqlite")),
            "plan",
            &source_argument,
            "--out",
            as_utf8(&default_plan),
        ]),
        0,
    );

    let remote_policy = temporary.path().join("remote-policy.toml");
    fs::write(
        &remote_policy,
        "[augmentation]\nallow_remote_providers = true\n",
    )
    .expect("write remote-enabled policy");
    let remote_plan = temporary.path().join("remote-plan.json");
    assert_exit(
        &run(&[
            "--policy",
            as_utf8(&remote_policy),
            "--workspace",
            as_utf8(&temporary.path().join("remote.sqlite")),
            "plan",
            &source_argument,
            "--out",
            as_utf8(&remote_plan),
        ]),
        0,
    );

    let provider = temporary.path().join("remote-provider.sh");
    fs::write(
        &provider,
        r#"#!/bin/sh
IFS= read -r capabilities_request || exit 20
printf '%s\n' '{"protocol_version":1,"request_id":"capabilities-1","message_type":"capabilities_response","payload":{"provider":{"provider":"cli-replay-fixture","model":"remote-zero","version":"1"},"protocol_versions":[1],"operations":["knowledge_augmentation"],"max_input_bytes":10485760,"max_output_bytes":10485760,"structured_output":true,"streaming":false,"deterministic_controls":true,"data_boundary":{"kind":"remote","endpoint_label":"fixture-remote"}}}'
IFS= read -r augmentation_request || exit 21
printf '%s\n' disclosed > "$1"
printf '%s\n' '{"protocol_version":1,"request_id":"augmentation-1","message_type":"augmentation_response","payload":{"proposals":[]}}'
"#,
    )
    .expect("write remote provider fixture");

    let policy_denied_marker = temporary.path().join("policy-denied-marker");
    let policy_denied_output = temporary.path().join("policy-denied.jsonl");
    assert_exit(
        &run(&[
            "augment",
            as_utf8(&default_plan),
            "--provider-cmd",
            "/bin/sh",
            "--provider-arg",
            as_utf8(&provider),
            "--provider-arg",
            as_utf8(&policy_denied_marker),
            "--all-documents",
            "--allow-remote-provider",
            "--out",
            as_utf8(&policy_denied_output),
        ]),
        EXIT_PROVIDER,
    );
    assert!(!policy_denied_marker.exists());
    assert!(!policy_denied_output.exists());

    let consent_denied_marker = temporary.path().join("consent-denied-marker");
    let consent_denied_output = temporary.path().join("consent-denied.jsonl");
    assert_exit(
        &run(&[
            "augment",
            as_utf8(&remote_plan),
            "--provider-cmd",
            "/bin/sh",
            "--provider-arg",
            as_utf8(&provider),
            "--provider-arg",
            as_utf8(&consent_denied_marker),
            "--all-documents",
            "--out",
            as_utf8(&consent_denied_output),
        ]),
        EXIT_PROVIDER,
    );
    assert!(!consent_denied_marker.exists());
    assert!(!consent_denied_output.exists());

    let allowed_marker = temporary.path().join("allowed-marker");
    let allowed_output = temporary.path().join("allowed.jsonl");
    assert_exit(
        &run(&[
            "augment",
            as_utf8(&remote_plan),
            "--provider-cmd",
            "/bin/sh",
            "--provider-arg",
            as_utf8(&provider),
            "--provider-arg",
            as_utf8(&allowed_marker),
            "--all-documents",
            "--allow-remote-provider",
            "--out",
            as_utf8(&allowed_output),
        ]),
        0,
    );
    assert_eq!(
        fs::read_to_string(&allowed_marker).expect("disclosure marker"),
        "disclosed\n"
    );

    fs::remove_file(&provider).expect("remove remote provider before replay");
    let replayed = temporary.path().join("remote-replayed.jsonl");
    assert_exit(
        &run(&[
            "replay",
            as_utf8(&remote_plan),
            "--augmentation",
            as_utf8(&allowed_output),
            "--out",
            as_utf8(&replayed),
        ]),
        0,
    );
    assert_eq!(
        fs::read(&replayed).expect("read remote replay"),
        fs::read(&allowed_output).expect("read remote live recording")
    );
}

#[test]
fn cli_provider_rejects_nested_unknown_and_duplicate_json_fields() {
    let temporary = tempfile::tempdir().expect("temporary strict-wire workspace");
    let source = fixture("augmentation_replay");
    let source_argument = format!("strict-wire={}", source.display());
    let plan = temporary.path().join("plan.json");
    assert_exit(
        &run(&[
            "--workspace",
            as_utf8(&temporary.path().join("workspace.sqlite")),
            "plan",
            &source_argument,
            "--out",
            as_utf8(&plan),
        ]),
        0,
    );

    for (case, response) in [
        (
            "unknown",
            r#"{"protocol_version":1,"request_id":"augmentation-1","message_type":"augmentation_response","payload":{"proposals":[],"unexpected":true}}"#,
        ),
        (
            "duplicate",
            r#"{"protocol_version":1,"request_id":"augmentation-1","message_type":"augmentation_response","payload":{"proposals":[],"proposals":[]}}"#,
        ),
    ] {
        let provider = temporary.path().join(format!("{case}-provider.sh"));
        fs::write(&provider, provider_with_response(response))
            .expect("write strict-wire provider fixture");
        let output = temporary.path().join(format!("{case}.jsonl"));
        assert_exit(
            &run(&[
                "augment",
                as_utf8(&plan),
                "--provider-cmd",
                "/bin/sh",
                "--provider-arg",
                as_utf8(&provider),
                "--all-documents",
                "--out",
                as_utf8(&output),
            ]),
            EXIT_PROVIDER,
        );
        assert!(
            !output.exists(),
            "invalid {case} provider response must not publish a recording"
        );
    }
}
