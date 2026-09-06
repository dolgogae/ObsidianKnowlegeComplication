use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn okc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_okc"))
}

fn run(arguments: &[&str]) -> Output {
    okc().args(arguments).output().expect("run okc")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("UTF-8 stdout")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 stderr")
}

fn assert_help_has_command(help: &str, command: &str) {
    assert!(
        help.lines()
            .any(|line| line.trim_start().starts_with(&format!("{command} "))),
        "help must contain `{command}`:\n{help}"
    );
}

fn assert_help_lacks_command(help: &str, command: &str) {
    assert!(
        !help
            .lines()
            .any(|line| line.trim_start().starts_with(&format!("{command} "))),
        "help must not contain `{command}`:\n{help}"
    );
}

#[test]
fn root_help_exposes_only_the_current_command_surface() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let help = stdout(&output);
    for command in [
        "project",
        "provider",
        "integrate",
        "integration",
        "review",
        "tui",
        "compile",
        "verify",
        "explain",
        "doctor",
        "update",
    ] {
        assert_help_has_command(&help, command);
    }
    for retired in [
        "inspect", "plan", "augment", "replay", "validate", "approve",
    ] {
        assert_help_lacks_command(&help, retired);
    }
    assert!(!help.contains("--policy"));
    assert!(!help.contains("--workspace"));
}

#[test]
fn compile_explain_and_project_help_exclude_retired_shapes() {
    let compile = stdout(&run(&["compile", "--help"]));
    assert!(compile.contains("--integration-plan"));
    assert!(compile.contains("--output"));
    assert!(!compile.contains("APPROVED_PLAN"));
    assert!(!compile.contains("--pack"));

    let explain = stdout(&run(&["explain", "--help"]));
    assert!(explain.contains("<DIRECTORY> <OUTPUT_PATH>"));
    assert!(!explain.contains("--package"));
    assert!(!explain.contains("--limit"));
    assert!(!explain.contains("--cursor"));

    let project = stdout(&run(&["project", "--help"]));
    assert_help_lacks_command(&project, "upgrade");

    let provider_add = stdout(&run(&["provider", "add", "--help"]));
    assert!(!provider_add.contains("command"));
}

#[test]
fn retired_commands_and_options_are_parse_errors() {
    for arguments in [
        vec!["inspect"],
        vec!["plan"],
        vec!["augment"],
        vec!["replay"],
        vec!["validate"],
        vec!["approve"],
        vec!["project", "upgrade"],
        vec!["--policy", "policy.toml", "doctor"],
        vec!["--workspace", "build.sqlite", "doctor"],
        vec!["compile", "approved.json", "--output", "out"],
        vec!["compile", "--output", "out", "--pack", "out.okcpack"],
        vec!["explain", "artifact", "file.md", "--package"],
        vec![
            "provider",
            "add",
            "local",
            "--kind",
            "command",
            "--endpoint",
            "command://local",
            "--model",
            "local",
        ],
    ] {
        let output = run(&arguments);
        assert_eq!(output.status.code(), Some(2), "arguments: {arguments:?}");
    }
}

fn write_manifest(root: &Path, marker: &str, bytes: &[u8]) {
    fs::create_dir_all(root.join(marker)).expect("marker");
    fs::write(root.join(marker).join("manifest.json"), bytes).expect("manifest");
}

#[test]
fn recognizable_retired_artifacts_report_unsupported_schema() {
    let temporary = tempfile::tempdir().expect("temporary");
    let schema_one = temporary.path().join("schema-one");
    write_manifest(&schema_one, ".vaultc", b"{}");
    let schema_two = temporary.path().join("schema-two");
    write_manifest(
        &schema_two,
        ".okc",
        br#"{"format_family":"okc","schema_version":2}"#,
    );
    let schema_one_pack = temporary.path().join("schema-one.vaultpack");
    fs::write(&schema_one_pack, b"retired pack marker").expect("schema one pack");
    let schema_two_pack = temporary.path().join("schema-two.okcpack");
    fs::write(&schema_two_pack, b"retired pack marker").expect("schema two pack");

    for (artifact, schema) in [
        (&schema_one, 1),
        (&schema_one_pack, 1),
        (&schema_two, 2),
        (&schema_two_pack, 2),
    ] {
        for command in ["verify", "explain"] {
            let mut invocation = okc();
            invocation.arg(command).arg(artifact);
            if command == "explain" {
                invocation.arg("knowledge/Topic.md");
            }
            let output = invocation.output().expect("inspect retired artifact");
            assert_eq!(output.status.code(), Some(7));
            let message = stderr(&output);
            assert!(message.contains(&format!("artifact schema {schema}")));
            assert!(message.contains("supported schema is 3"));
        }
    }
}

#[test]
fn no_argument_non_tty_prints_help_and_returns_usage() {
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).contains("Usage: okc"));
}
