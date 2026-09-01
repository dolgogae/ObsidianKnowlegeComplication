use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use vaultc::config::CompilerPolicy;
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

fn run_compile(approved: &Path, output: &Path, pack: Option<&Path>) -> Output {
    let mut command = Command::new(binary());
    command
        .arg("compile")
        .arg(approved)
        .arg("--output")
        .arg(output);
    if let Some(pack) = pack {
        command.arg("--pack").arg(pack);
    }
    command
        .output()
        .unwrap_or_else(|error| panic!("run vaultc compile: {error}"))
}

fn staging_entries(parent: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(parent)
        .unwrap_or_else(|error| panic!("read publication parent {}: {error}", parent.display()))
        .map(|entry| entry.expect("read publication-parent entry").path())
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(".vaultc-staging-") || name.starts_with(".vaultc-pack-")
            })
        })
        .collect();
    entries.sort();
    entries
}

fn assert_no_staging_entries(parent: &Path) {
    assert_eq!(
        staging_entries(parent),
        Vec::<PathBuf>::new(),
        "caught publication failures must not leave default-policy staging entries"
    );
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

fn write_basic_approved_plan(parent: &Path) -> PathBuf {
    let compiler = VaultCompiler::builder()
        .build()
        .expect("build approved-plan fixture compiler");
    let inspection = compiler
        .inspect([SourceSpec::directory("basic", fixture("basic_vault"))
            .expect("approved-plan fixture source")])
        .expect("inspect approved-plan fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan approved-plan fixture");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve approved-plan fixture");
    let path = parent.join("approved.json");
    fs::write(
        &path,
        vaultc::canonical::to_canonical_json_pretty(&approved)
            .expect("encode approved-plan fixture"),
    )
    .expect("write approved-plan fixture");
    path
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

#[cfg(unix)]
fn augmentation_payload_hash(payload: &serde_json::Value, plan: &DraftPlan) -> String {
    let mut hydrated = payload.clone();
    let documents = hydrated["documents"]
        .as_array_mut()
        .expect("augmentation documents");
    for projection in documents {
        let document_id = projection["document"]["id"]
            .as_str()
            .expect("projected document ID");
        let document = plan
            .workspace
            .documents
            .values()
            .find(|document| document.document_id.to_string() == document_id)
            .expect("projected document exists in plan");
        for projected_block in projection["selected_blocks"]
            .as_array_mut()
            .expect("projected blocks")
        {
            let block_id = projected_block["block"]["id"]
                .as_str()
                .expect("projected block ID");
            let block = document
                .blocks
                .iter()
                .find(|block| block.block_id.to_string() == block_id)
                .expect("projected block exists in plan");
            projected_block["text"] = serde_json::Value::String(block.comparison_text.clone());
        }
    }
    vaultc::canonical::canonical_hash("vaultc:provider-transcript-payload:v1\0", &hydrated)
        .expect("hash hydrated augmentation request")
        .hex()
}

#[test]
#[allow(clippy::too_many_lines)]
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
    let compile_json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("decode compile success JSON");
    assert_eq!(compile_json["artifact"]["path"], as_utf8(&compiled_vault));
    assert_eq!(compile_json["vaultpack"], as_utf8(&pack));
    assert!(compiled_vault.join(".vaultc/manifest.json").is_file());
    assert!(pack.is_file());

    let sdk_pack = temporary.path().join("sdk-published.vaultpack");
    vaultc::pack::create_pack(
        &compiled_vault,
        &sdk_pack,
        compiler.policy().output.zstd_level,
    )
    .expect("publish SDK comparison pack");
    assert_eq!(
        fs::read(&pack).expect("read CLI VaultPack"),
        fs::read(&sdk_pack).expect("read SDK VaultPack"),
        "CLI and SDK must use the same deterministic pack writer"
    );

    let human_compiled = temporary.path().join("human-compiled");
    let human_pack = temporary.path().join("human-compiled.vaultpack");
    let human = run(&[
        "compile",
        as_utf8(&approved_path),
        "--output",
        as_utf8(&human_compiled),
        "--pack",
        as_utf8(&human_pack),
    ]);
    assert_exit(&human, 0);
    let human_stdout = String::from_utf8_lossy(&human.stdout);
    assert!(human_stdout.contains(&format!("path: {}", human_compiled.display())));
    assert!(human_stdout.contains(&format!("vaultpack: {}", human_pack.display())));
    assert_eq!(
        fs::read(&pack).expect("read JSON-mode VaultPack"),
        fs::read(&human_pack).expect("read human-mode VaultPack"),
        "output format must not affect pack bytes"
    );

    let mut inner_pages = Vec::new();
    let mut inner_page_json = Vec::new();
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
            "--limit",
            "4096",
            "--format",
            "json",
        ]);
        assert_exit(&output, 0);
        inner_page_json.push(output.stdout.clone());
        let page: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("decode provenance JSON");
        assert_eq!(page["schema_version"], 1);
        assert!(page["graph_hash"].as_str().is_some());
        assert_eq!(
            page["subject"],
            serde_json::json!({
                "type": "artifact_path",
                "path": "knowledge/Index.md"
            })
        );
        assert!(
            page["records"]
                .as_array()
                .is_some_and(|records| !records.is_empty())
        );
        assert_eq!(page["next_cursor"], serde_json::Value::Null);
        assert_eq!(page["complete"], true);
        inner_pages.push(page);
    }
    assert_eq!(
        inner_pages[0], inner_pages[1],
        "inner-path provenance pages must be identical for a directory and its VaultPack"
    );
    assert_eq!(
        inner_page_json[0], inner_page_json[1],
        "inner-path JSON bytes must be identical for a directory and its VaultPack"
    );

    let first_page = run(&[
        "explain",
        as_utf8(&compiled_vault),
        "knowledge/Index.md",
        "--limit",
        "1",
        "--format",
        "json",
    ]);
    assert_exit(&first_page, 0);
    let first_page: serde_json::Value =
        serde_json::from_slice(&first_page.stdout).expect("decode first provenance page");
    assert_eq!(first_page["records"].as_array().map(Vec::len), Some(1));
    assert_eq!(first_page["complete"], false);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("incomplete page must return a cursor");

    let second_page = run(&[
        "explain",
        as_utf8(&compiled_vault),
        "knowledge/Index.md",
        "--limit",
        "1",
        "--cursor",
        cursor,
        "--format",
        "json",
    ]);
    assert_exit(&second_page, 0);
    let second_page: serde_json::Value =
        serde_json::from_slice(&second_page.stdout).expect("decode resumed provenance page");
    assert_eq!(second_page["records"].as_array().map(Vec::len), Some(1));
    assert_ne!(first_page["records"], second_page["records"]);

    let human_page = run(&[
        "explain",
        as_utf8(&compiled_vault),
        "knowledge/Index.md",
        "--limit",
        "1",
    ]);
    assert_exit(&human_page, 0);
    let human_page = String::from_utf8_lossy(&human_page.stdout);
    assert!(human_page.contains("complete: false"));
    assert!(human_page.contains("next cursor: cursor_"));

    let package_page = run(&[
        "explain",
        as_utf8(&pack),
        "--package",
        "--limit",
        "4096",
        "--format",
        "json",
    ]);
    assert_exit(&package_page, 0);
    let package_page: serde_json::Value =
        serde_json::from_slice(&package_page.stdout).expect("decode package provenance page");
    assert_eq!(package_page["schema_version"], 1);
    assert!(
        package_page["records"]
            .as_array()
            .is_some_and(|records| !records.is_empty())
    );
    assert_eq!(
        package_page["subject"],
        serde_json::json!({ "type": "package" })
    );
    let package_records = package_page["records"]
        .as_array()
        .expect("package provenance records");
    assert!(package_records.iter().any(|record| {
        record["kind"]["type"] == "operation" && record["kind"]["value"]["type"] == "package"
    }));
    assert!(package_records.iter().any(|record| {
        record["kind"]["type"] == "output"
            && record["kind"]["value"]["storage"] == "virtual_package"
    }));

    assert_exit(
        &run(&["explain", as_utf8(&compiled_vault), "--package"]),
        EXIT_VERIFY,
    );
    assert_exit(
        &run(&[
            "explain",
            as_utf8(&compiled_vault),
            "knowledge/Index.md",
            "--cursor",
            "not-a-valid-cursor",
        ]),
        EXIT_VERIFY,
    );
    assert_exit(
        &run(&["explain", as_utf8(&pack), "--package", "--cursor", cursor]),
        EXIT_VERIFY,
    );

    let output = run(&[
        "compile",
        as_utf8(&approved_path),
        "--output",
        as_utf8(&compiled_vault),
    ]);
    assert_exit(&output, EXIT_OUTPUT);

    fs::write(
        compiled_vault.join(".vaultc/provenance.jsonl"),
        b"not a provenance record\n",
    )
    .expect("tamper provenance graph");
    assert_exit(
        &run(&["explain", as_utf8(&compiled_vault), "knowledge/Index.md"]),
        EXIT_VERIFY,
    );
}

#[test]
fn cli_compile_preserves_existing_file_and_directories() {
    let temporary = tempfile::tempdir().expect("temporary existing-output workspace");
    let approved = write_basic_approved_plan(temporary.path());

    let existing_file = temporary.path().join("existing-file");
    let file_sentinel = b"owned regular file\n";
    fs::write(&existing_file, file_sentinel).expect("write existing-file sentinel");
    assert_exit(&run_compile(&approved, &existing_file, None), EXIT_OUTPUT);
    assert_eq!(
        fs::read(&existing_file).expect("read preserved regular file"),
        file_sentinel
    );

    let empty_directory = temporary.path().join("existing-empty-directory");
    fs::create_dir(&empty_directory).expect("create empty destination directory");
    assert_exit(&run_compile(&approved, &empty_directory, None), EXIT_OUTPUT);
    assert_eq!(
        fs::read_dir(&empty_directory)
            .expect("read preserved empty directory")
            .count(),
        0
    );

    let nonempty_directory = temporary.path().join("existing-nonempty-directory");
    fs::create_dir(&nonempty_directory).expect("create nonempty destination directory");
    let directory_sentinel = b"owned directory entry\n";
    fs::write(nonempty_directory.join("sentinel.txt"), directory_sentinel)
        .expect("write directory sentinel");
    assert_exit(
        &run_compile(&approved, &nonempty_directory, None),
        EXIT_OUTPUT,
    );
    assert_eq!(
        fs::read(nonempty_directory.join("sentinel.txt"))
            .expect("read preserved directory sentinel"),
        directory_sentinel
    );
    assert_no_staging_entries(temporary.path());
}

#[cfg(unix)]
#[test]
fn cli_compile_preserves_live_and_dangling_output_symlinks() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary output-symlink workspace");
    let approved = write_basic_approved_plan(temporary.path());

    let live_target = temporary.path().join("live-target");
    fs::create_dir(&live_target).expect("create live output-link target");
    let live_sentinel = b"owned link referent\n";
    fs::write(live_target.join("sentinel.txt"), live_sentinel).expect("write live-link sentinel");
    let live_link = temporary.path().join("live-output-link");
    symlink(&live_target, &live_link).expect("create live output symlink");

    assert_exit(&run_compile(&approved, &live_link, None), EXIT_OUTPUT);
    assert!(
        fs::symlink_metadata(&live_link)
            .expect("live output link remains")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(live_target.join("sentinel.txt")).expect("read live-link sentinel"),
        live_sentinel
    );

    let dangling_target = temporary.path().join("must-not-be-created");
    let dangling_link = temporary.path().join("dangling-output-link");
    symlink(&dangling_target, &dangling_link).expect("create dangling output symlink");

    assert_exit(&run_compile(&approved, &dangling_link, None), EXIT_OUTPUT);
    assert!(
        fs::symlink_metadata(&dangling_link)
            .expect("dangling output link remains")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&dangling_link).expect("read dangling output link"),
        dangling_target
    );
    assert!(!dangling_target.exists());
    assert_no_staging_entries(temporary.path());
}

#[cfg(windows)]
#[test]
fn cli_compile_preserves_live_and_dangling_output_symlinks() {
    use std::io::ErrorKind;
    use std::os::windows::fs::symlink_dir;

    let temporary = tempfile::tempdir().expect("temporary output-symlink workspace");
    let approved = write_basic_approved_plan(temporary.path());
    let live_target = temporary.path().join("live-target");
    fs::create_dir(&live_target).expect("create live output-link target");
    let live_sentinel = b"owned link referent\n";
    fs::write(live_target.join("sentinel.txt"), live_sentinel).expect("write live-link sentinel");
    let live_link = temporary.path().join("live-output-link");
    if let Err(error) = symlink_dir(&live_target, &live_link) {
        if error.kind() == ErrorKind::PermissionDenied {
            eprintln!(
                "skipping Windows output-symlink contract: runner cannot create symlinks: {error}"
            );
            return;
        }
        panic!("create live output symlink: {error}");
    }

    assert_exit(&run_compile(&approved, &live_link, None), EXIT_OUTPUT);
    assert!(
        fs::symlink_metadata(&live_link)
            .expect("live output link remains")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(live_target.join("sentinel.txt")).expect("read live-link sentinel"),
        live_sentinel
    );

    let dangling_target = temporary.path().join("must-not-be-created");
    let dangling_link = temporary.path().join("dangling-output-link");
    symlink_dir(&dangling_target, &dangling_link).expect("create dangling output symlink");
    assert_exit(&run_compile(&approved, &dangling_link, None), EXIT_OUTPUT);
    assert!(
        fs::symlink_metadata(&dangling_link)
            .expect("dangling output link remains")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&dangling_link).expect("read dangling output link"),
        dangling_target
    );
    assert!(!dangling_target.exists());
    assert_no_staging_entries(temporary.path());
}

#[test]
fn cli_directory_concurrent_creators_have_exactly_one_verified_winner() {
    const CREATORS: usize = 4;

    let temporary = tempfile::tempdir().expect("temporary concurrent-output workspace");
    let approved = write_basic_approved_plan(temporary.path());
    let output = temporary.path().join("concurrent-output");
    let barrier = Arc::new(Barrier::new(CREATORS));
    let workers: Vec<_> = (0..CREATORS)
        .map(|_| {
            let approved = approved.clone();
            let output = output.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                run_compile(&approved, &output, None)
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("join concurrent compiler"))
        .collect();

    assert_eq!(
        results
            .iter()
            .filter(|result| result.status.code() == Some(0))
            .count(),
        1,
        "exactly one CLI publisher must win: {results:#?}"
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result.status.code() == Some(EXIT_OUTPUT))
            .count(),
        CREATORS - 1,
        "all CLI race losers must report output exit 6: {results:#?}"
    );
    assert_exit(&run(&["verify", as_utf8(&output)]), 0);
    assert_no_staging_entries(temporary.path());
}

#[test]
fn cli_directory_race_losers_do_not_publish_distinct_packs() {
    const CREATORS: usize = 4;

    let temporary = tempfile::tempdir().expect("temporary concurrent-pack workspace");
    let approved = write_basic_approved_plan(temporary.path());
    let output = temporary.path().join("concurrent-output-with-pack");
    let barrier = Arc::new(Barrier::new(CREATORS));
    let workers: Vec<_> = (0..CREATORS)
        .map(|index| {
            let approved = approved.clone();
            let output = output.clone();
            let pack = temporary
                .path()
                .join(format!("candidate-{index}.vaultpack"));
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let result = run_compile(&approved, &output, Some(&pack));
                (pack, result)
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("join concurrent pack compiler"))
        .collect();

    let winners: Vec<_> = results
        .iter()
        .filter(|(_, result)| result.status.code() == Some(0))
        .collect();
    assert_eq!(winners.len(), 1, "exactly one compile-plus-pack must win");
    for (pack, result) in &results {
        if result.status.code() == Some(0) {
            assert!(pack.is_file(), "winner must publish its requested pack");
            assert_exit(&run(&["verify", as_utf8(pack)]), 0);
        } else {
            assert_exit(result, EXIT_OUTPUT);
            assert!(
                fs::symlink_metadata(pack).is_err(),
                "directory race loser must not begin pack publication: {}",
                pack.display()
            );
        }
    }
    assert_exit(&run(&["verify", as_utf8(&output)]), 0);
    assert_no_staging_entries(temporary.path());
}

#[test]
fn cli_compile_rejects_source_overlapping_outputs_and_packs_without_staging() {
    let temporary = tempfile::tempdir().expect("temporary source-output workspace");
    let source = temporary.path().join("immutable-source");
    fs::create_dir(&source).expect("create immutable source");
    let source_bytes = b"# Immutable source\n";
    fs::write(source.join("Index.md"), source_bytes).expect("write immutable source note");

    let mut policy = CompilerPolicy::default();
    policy.output.retain_failed_staging = true;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build retained-stage compiler");
    let inspection = compiler
        .inspect(
            [SourceSpec::directory("immutable", &source).expect("immutable source descriptor")],
        )
        .expect("inspect immutable source");
    let plan = compiler.plan(&inspection).expect("plan immutable source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve immutable source plan");
    let approved_path = temporary.path().join("source-approved.json");
    fs::write(
        &approved_path,
        vaultc::canonical::to_canonical_json_pretty(&approved)
            .expect("encode source-overlap approved plan"),
    )
    .expect("write source-overlap approved plan");

    let output = source.join("nested-output");
    let pack = temporary.path().join("must-not-publish.vaultpack");
    assert_exit(
        &run_compile(&approved_path, &output, Some(&pack)),
        EXIT_OUTPUT,
    );
    assert!(fs::symlink_metadata(&output).is_err());
    assert!(fs::symlink_metadata(&pack).is_err());
    assert_eq!(
        fs::read(source.join("Index.md")).expect("read preserved immutable source"),
        source_bytes
    );

    let outside_output = temporary.path().join("outside-output");
    let pack_inside_source = source.join("nested.vaultpack");
    assert_exit(
        &run_compile(&approved_path, &outside_output, Some(&pack_inside_source)),
        EXIT_OUTPUT,
    );
    assert!(fs::symlink_metadata(&outside_output).is_err());
    assert!(fs::symlink_metadata(&pack_inside_source).is_err());
    assert_eq!(
        fs::read(source.join("Index.md")).expect("read immutable source after pack preflight"),
        source_bytes
    );
    assert_no_staging_entries(&source);
    assert_no_staging_entries(temporary.path());
}

#[test]
fn cli_compile_pack_extension_is_usage_error_before_control_file_open() {
    let temporary = tempfile::tempdir().expect("temporary pack-extension workspace");
    let output = temporary.path().join("must-not-exist");
    let invalid_pack = temporary.path().join("release.tar.zst");
    let missing_plan = temporary.path().join("missing-approved.json");

    assert_exit(
        &run(&[
            "compile",
            as_utf8(&missing_plan),
            "--output",
            as_utf8(&output),
            "--pack",
            as_utf8(&invalid_pack),
        ]),
        EXIT_USAGE,
    );
    assert!(!output.exists());
    assert!(!invalid_pack.exists());
}

#[test]
fn cli_compile_pack_preflight_rejects_existing_and_path_aliases_without_output() {
    let temporary = tempfile::tempdir().expect("temporary pack-preflight workspace");
    let approved = write_basic_approved_plan(temporary.path());

    let existing_pack = temporary.path().join("existing.vaultpack");
    let sentinel = b"owned by another publisher\n";
    fs::write(&existing_pack, sentinel).expect("write existing pack sentinel");
    let existing_output = temporary.path().join("existing-pack-output");
    assert_exit(
        &run(&[
            "compile",
            as_utf8(&approved),
            "--output",
            as_utf8(&existing_output),
            "--pack",
            as_utf8(&existing_pack),
        ]),
        EXIT_OUTPUT,
    );
    assert!(!existing_output.exists());
    assert_eq!(
        fs::read(&existing_pack).expect("read preserved pack sentinel"),
        sentinel
    );

    let cases = [
        (
            temporary.path().join("equal.vaultpack"),
            temporary.path().join("equal.vaultpack"),
        ),
        (
            temporary.path().join("direct-inside"),
            temporary
                .path()
                .join("direct-inside/nested/release.vaultpack"),
        ),
        (
            temporary.path().join("reserved.vaultpack/compiled"),
            temporary.path().join("reserved.vaultpack"),
        ),
        (
            temporary.path().join("StraßeVault"),
            temporary.path().join("STRASSEVAULT/release.vaultpack"),
        ),
        (
            temporary.path().join("CaféVault"),
            temporary.path().join("CAFE\u{301}VAULT/release.vaultpack"),
        ),
    ];
    for (output, pack) in cases {
        let result = run(&[
            "compile",
            as_utf8(&approved),
            "--output",
            as_utf8(&output),
            "--pack",
            as_utf8(&pack),
        ]);
        assert_exit(&result, EXIT_OUTPUT);
        assert!(
            !output.exists(),
            "pack preflight must precede output publication: {}",
            output.display()
        );
        assert!(
            fs::symlink_metadata(&pack).is_err(),
            "rejected pack destination must remain absent: {}",
            pack.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn cli_compile_pack_preflight_rejects_symlink_aliases_without_output() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary pack-symlink workspace");
    let approved = write_basic_approved_plan(temporary.path());
    let real_parent = temporary.path().join("real");
    fs::create_dir(&real_parent).expect("create real destination parent");
    let parent_alias = temporary.path().join("alias");
    symlink(&real_parent, &parent_alias).expect("create destination-parent alias");

    let output = real_parent.join("compiled");
    let pack = parent_alias.join("compiled/nested.vaultpack");
    assert_exit(
        &run(&[
            "compile",
            as_utf8(&approved),
            "--output",
            as_utf8(&output),
            "--pack",
            as_utf8(&pack),
        ]),
        EXIT_OUTPUT,
    );
    assert!(!output.exists());
    assert!(
        fs::symlink_metadata(&parent_alias)
            .expect("parent alias remains")
            .file_type()
            .is_symlink()
    );

    let dangling_target = temporary.path().join("must-not-be-created.vaultpack");
    let dangling_pack = temporary.path().join("dangling.vaultpack");
    symlink(&dangling_target, &dangling_pack).expect("create dangling pack leaf");
    let dangling_output = temporary.path().join("dangling-output");
    assert_exit(
        &run(&[
            "compile",
            as_utf8(&approved),
            "--output",
            as_utf8(&dangling_output),
            "--pack",
            as_utf8(&dangling_pack),
        ]),
        EXIT_OUTPUT,
    );
    assert!(!dangling_output.exists());
    assert!(!dangling_target.exists());
    assert!(
        fs::symlink_metadata(&dangling_pack)
            .expect("dangling pack leaf remains")
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn cli_runtime_pack_failure_leaves_verified_output_and_no_pack() {
    use std::os::unix::fs::PermissionsExt as _;

    let temporary = tempfile::tempdir().expect("temporary runtime-pack-failure workspace");
    let approved = write_basic_approved_plan(temporary.path());
    let output = temporary.path().join("compiled-after-pack-failure");
    let pack_parent = temporary.path().join("read-only-pack-parent");
    fs::create_dir(&pack_parent).expect("create pack parent");
    fs::set_permissions(&pack_parent, fs::Permissions::from_mode(0o500))
        .expect("make pack parent read-only");
    let pack = pack_parent.join("runtime-failure.vaultpack");

    let result = run(&[
        "compile",
        as_utf8(&approved),
        "--output",
        as_utf8(&output),
        "--pack",
        as_utf8(&pack),
    ]);
    fs::set_permissions(&pack_parent, fs::Permissions::from_mode(0o700))
        .expect("restore pack-parent permissions");

    assert_exit(&result, EXIT_OUTPUT);
    assert!(output.join(".vaultc/manifest.json").is_file());
    assert!(!pack.exists());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("remains published after VaultPack"),
        "runtime failure must expose the two-publication state: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_exit(&run(&["verify", as_utf8(&output)]), 0);
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
    assert_exit(
        &run(&[
            "explain",
            as_utf8(&invalid_artifact),
            "knowledge/Index.md",
            "--limit",
            "0",
        ]),
        EXIT_USAGE,
    );
    assert_exit(&run(&["verify", as_utf8(&invalid_artifact)]), EXIT_VERIFY);
}

#[test]
fn approve_rejects_pre_hash_markdown_plan_as_decision_error() {
    let temporary = tempfile::tempdir().expect("temporary old-plan workspace");
    let source = temporary.path().join("source");
    fs::create_dir(&source).expect("create Markdown source");
    fs::write(source.join("Index.md"), "Read [[Target]].\n").expect("write linking note");
    fs::write(source.join("Target.md"), "# Target\n").expect("write target note");

    let source_argument = format!("old-plan={}", source.display());
    let workspace = temporary.path().join("workspace.sqlite");
    let plan_path = temporary.path().join("plan.json");
    let output = run(&[
        "--workspace",
        as_utf8(&workspace),
        "plan",
        &source_argument,
        "--out",
        as_utf8(&plan_path),
    ]);
    assert_exit(&output, 0);

    let mut plan: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan_path).expect("read current plan"))
            .expect("decode current plan");
    let plan_id = plan["plan_id"].as_str().expect("plan ID").to_owned();
    let rewrite = plan["operations"]
        .as_array_mut()
        .expect("plan operations")
        .iter_mut()
        .find(|operation| operation["type"].as_str() == Some("rewrite_markdown"))
        .expect("fixture requires a Markdown rewrite");
    assert!(
        rewrite
            .as_object_mut()
            .expect("rewrite operation object")
            .remove("expected_output_hash")
            .is_some(),
        "current Markdown rewrite must contain expected_output_hash"
    );
    let old_plan_path = temporary.path().join("pre-hash-plan.json");
    fs::write(
        &old_plan_path,
        serde_json::to_vec_pretty(&plan).expect("encode pre-hash plan"),
    )
    .expect("write pre-hash plan");

    let decisions_path = temporary.path().join("decisions.json");
    fs::write(
        &decisions_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "plan_id": plan_id,
            "decisions": [],
            "conflicts": []
        }))
        .expect("encode decisions"),
    )
    .expect("write decisions");
    let approved_path = temporary.path().join("approved.json");
    let output = run(&[
        "approve",
        as_utf8(&old_plan_path),
        "--decisions",
        as_utf8(&decisions_path),
        "--out",
        as_utf8(&approved_path),
    ]);
    assert_exit(&output, EXIT_DECISION);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("expected_output_hash"),
        "stderr must identify the missing required plan field: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!approved_path.exists());
}

#[test]
fn approve_rejects_pre_original_path_plan_as_decision_error() {
    let temporary = tempfile::tempdir().expect("temporary old path-schema workspace");
    let source = fixture("basic_vault");
    let source_argument = format!("old-path-schema={}", source.display());
    let plan_path = temporary.path().join("plan.json");
    assert_exit(
        &run(&["plan", &source_argument, "--out", as_utf8(&plan_path)]),
        0,
    );

    let mut plan: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan_path).expect("read current plan"))
            .expect("decode current plan");
    let plan_id = plan["plan_id"].as_str().expect("plan ID").to_owned();
    let source_file = plan["snapshots"][0]["files"][0]
        .as_object_mut()
        .expect("sealed source file");
    assert!(
        source_file.remove("original_path").is_some(),
        "current source file must contain required original_path"
    );
    let old_plan_path = temporary.path().join("pre-original-path-plan.json");
    fs::write(
        &old_plan_path,
        serde_json::to_vec_pretty(&plan).expect("encode pre-original-path plan"),
    )
    .expect("write pre-original-path plan");

    let decisions_path = temporary.path().join("decisions.json");
    fs::write(
        &decisions_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "plan_id": plan_id,
            "decisions": [],
            "conflicts": []
        }))
        .expect("encode decisions"),
    )
    .expect("write decisions");
    let approved_path = temporary.path().join("approved.json");
    let output = run(&[
        "approve",
        as_utf8(&old_plan_path),
        "--decisions",
        as_utf8(&decisions_path),
        "--out",
        as_utf8(&approved_path),
    ]);
    assert_exit(&output, EXIT_DECISION);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("original_path"),
        "stderr must identify the missing required source-path field: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!approved_path.exists());
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
#[allow(clippy::too_many_lines)]
fn cli_round_trips_a_redacted_command_provider_transcript() {
    let temporary = tempfile::tempdir().expect("temporary provider workspace");
    let source = fixture("basic_vault");
    let source_argument = format!("basic={}", source.display());
    let plan_path = temporary.path().join("plan.json");
    assert_exit(
        &run(&["plan", &source_argument, "--out", as_utf8(&plan_path)]),
        0,
    );
    let plan: DraftPlan =
        serde_json::from_slice(&fs::read(&plan_path).expect("read provider plan"))
            .expect("decode provider plan");
    let document = plan
        .workspace
        .documents
        .values()
        .next()
        .expect("provider fixture document");
    let snapshot_id = document.source_file.snapshot_id.to_string();
    let document_id = document.document_id.to_string();
    let document_hash = document.body_hash.hex();
    let block = document.blocks.first().expect("provider fixture block");
    let block_id = block.block_id.to_string();
    let block_hash = block.content_hash.hex();
    let plan_id = plan.plan_id.to_string();
    let projection_hash = plan.projection_hash.hex();

    let provider = temporary.path().join("provider.sh");
    fs::write(
        &provider,
        r##"#!/bin/sh
IFS= read -r capabilities_request || exit 20
printf '%s\n' '{"protocol_version":1,"request_id":"capabilities-1","message_type":"capabilities_response","payload":{"provider":{"provider":"fixture","model":"snapshot-bound","version":"1"},"protocol_versions":[1],"operations":["knowledge_augmentation"],"max_input_bytes":10485760,"max_output_bytes":10485760,"structured_output":true,"streaming":false,"deterministic_controls":true,"data_boundary":{"kind":"local"}}}'
IFS= read -r augmentation_request || exit 21
case "$augmentation_request" in *\"snapshot_id\":\"$1\"*) ;; *) exit 22 ;; esac
case "$augmentation_request" in *\"id\":\"$2\"*) ;; *) exit 23 ;; esac
case "$augmentation_request" in *\"content_hash\":\"$3\"*) ;; *) exit 24 ;; esac
case "$augmentation_request" in *\"id\":\"$6\"*) ;; *) exit 25 ;; esac
case "$augmentation_request" in *\"content_hash\":\"$7\"*) ;; *) exit 26 ;; esac
printf '%s\n' "{\"protocol_version\":1,\"request_id\":\"augmentation-1\",\"message_type\":\"augmentation_response\",\"payload\":{\"proposals\":[{\"schema_version\":1,\"proposal_id\":\"snapshot-bound-1\",\"plan_id\":\"$4\",\"projection_hash\":\"$5\",\"provider\":{\"provider\":\"fixture\",\"model\":\"snapshot-bound\",\"version\":\"1\"},\"kind\":{\"type\":\"create_generated_note\",\"title\":\"Snapshot bound\",\"markdown_body\":\"# Snapshot bound\\n\",\"suggested_path\":\"snapshot-bound.md\"},\"evidence\":[{\"snapshot_id\":\"$1\",\"document_id\":\"$2\",\"block_id\":null,\"byte_start\":null,\"byte_end\":null,\"content_hash\":\"$3\"},{\"snapshot_id\":\"$1\",\"document_id\":\"$2\",\"block_id\":\"$6\",\"byte_start\":null,\"byte_end\":null,\"content_hash\":\"$7\"}],\"uncertainty\":null,\"rationale\":\"snapshot projection contract\"}]}}"
"##,
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
        "--provider-arg",
        &snapshot_id,
        "--provider-arg",
        &document_id,
        "--provider-arg",
        &document_hash,
        "--provider-arg",
        &plan_id,
        "--provider-arg",
        &projection_hash,
        "--provider-arg",
        &block_id,
        "--provider-arg",
        &block_hash,
        "--all-documents",
        "--out",
        as_utf8(&augmentation),
    ]);
    assert_exit(&output, 0);
    let augmentation_text =
        fs::read_to_string(&augmentation).expect("read augmentation transcript");
    assert!(augmentation_text.contains("[redacted]"));
    assert!(!augmentation_text.contains("Welcome to the fixture"));
    assert!(augmentation_text.contains(&snapshot_id));

    let replayed_augmentation = temporary.path().join("replayed-augmentation.jsonl");
    assert_exit(
        &run(&[
            "replay",
            as_utf8(&plan_path),
            "--augmentation",
            as_utf8(&augmentation),
            "--out",
            as_utf8(&replayed_augmentation),
        ]),
        0,
    );
    assert_eq!(
        fs::read(&replayed_augmentation).expect("read proposal-bearing replay"),
        fs::read(&augmentation).expect("read live proposal recording")
    );

    let augmentation_records: Vec<serde_json::Value> = augmentation_text
        .lines()
        .map(|line| serde_json::from_str(line).expect("decode augmentation record"))
        .collect();
    let validation = augmentation_records
        .iter()
        .find(|record| record["type"] == "proposal")
        .and_then(|record| record.get("validation"))
        .expect("proposal validation record");
    assert_eq!(validation["valid"], true);
    assert_eq!(
        validation["proposal"]["evidence"][0]["snapshot_id"],
        snapshot_id
    );
    assert_eq!(validation["proposal"]["evidence"][1]["block_id"], block_id);
    let proposal_content_hash = validation["content_hash"]
        .as_str()
        .expect("proposal content hash");
    let decisions = temporary.path().join("decisions.json");
    fs::write(
        &decisions,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "plan_id": plan.plan_id.to_string(),
            "decisions": [{
                "plan_id": plan.plan_id.to_string(),
                "proposal_id": "snapshot-bound-1",
                "proposal_content_hash": proposal_content_hash,
                "approved": true,
                "approver": "cli-integration-test",
                "policy_version": "test-v1"
            }],
            "conflicts": []
        }))
        .expect("encode provider decisions"),
    )
    .expect("write provider decisions");

    let mut stale_augmentation_records = augmentation_records.clone();
    let stale_request = stale_augmentation_records
        .iter_mut()
        .find(|record| {
            record["type"] == "transcript"
                && record["record"]["message_type"] == "augmentation_request"
        })
        .expect("augmentation request record");
    stale_request["record"]["payload"]["documents"][0]["snapshot_id"] =
        serde_json::Value::String(format!("snap_{}", "0".repeat(64)));
    stale_request["record"]["canonical_payload_hash"] = serde_json::Value::String(
        augmentation_payload_hash(&stale_request["record"]["payload"], &plan),
    );
    let stale_augmentation = temporary.path().join("stale-augmentation.jsonl");
    let mut stale_jsonl = stale_augmentation_records
        .iter()
        .map(|record| serde_json::to_string(record).expect("encode stale augmentation record"))
        .collect::<Vec<_>>()
        .join("\n");
    stale_jsonl.push('\n');
    fs::write(&stale_augmentation, stale_jsonl).expect("write stale augmentation");
    let stale_approved = temporary.path().join("stale-approved.json");
    assert_exit(
        &run(&[
            "approve",
            as_utf8(&plan_path),
            "--decisions",
            as_utf8(&decisions),
            "--proposals",
            as_utf8(&stale_augmentation),
            "--out",
            as_utf8(&stale_approved),
        ]),
        EXIT_PROVIDER,
    );
    assert!(!stale_approved.exists());

    let approved = temporary.path().join("approved.json");
    assert_exit(
        &run(&[
            "approve",
            as_utf8(&plan_path),
            "--decisions",
            as_utf8(&decisions),
            "--proposals",
            as_utf8(&replayed_augmentation),
            "--out",
            as_utf8(&approved),
        ]),
        0,
    );

    let mut tampered: serde_json::Value =
        serde_json::from_slice(&fs::read(&approved).expect("read approved provider plan"))
            .expect("decode approved provider plan");
    let request_record = tampered["transcript"]
        .as_array_mut()
        .expect("approved transcript")
        .iter_mut()
        .find(|record| record["message_type"] == "augmentation_request")
        .expect("augmentation request transcript");
    request_record["payload"]["documents"][0]["snapshot_id"] =
        serde_json::Value::String(format!("snap_{}", "0".repeat(64)));
    request_record["canonical_payload_hash"] =
        serde_json::Value::String(augmentation_payload_hash(&request_record["payload"], &plan));
    let tampered_approved = temporary.path().join("tampered-approved.json");
    fs::write(
        &tampered_approved,
        serde_json::to_vec_pretty(&tampered).expect("encode tampered approved plan"),
    )
    .expect("write tampered approved plan");
    let rejected_output = temporary.path().join("rejected-snapshot-binding");
    assert_exit(
        &run(&[
            "compile",
            as_utf8(&tampered_approved),
            "--output",
            as_utf8(&rejected_output),
        ]),
        EXIT_OUTPUT,
    );
    assert!(!rejected_output.exists());

    let compiled = temporary.path().join("snapshot-bound-vault");
    assert_exit(
        &run(&[
            "compile",
            as_utf8(&approved),
            "--output",
            as_utf8(&compiled),
        ]),
        0,
    );
    assert_exit(&run(&["verify", as_utf8(&compiled)]), 0);
    let generated_path = "knowledge/_generated/snapshot-bound.md";
    let explanation = run(&[
        "explain",
        as_utf8(&compiled),
        generated_path,
        "--format",
        "json",
    ]);
    assert_exit(&explanation, 0);
    let explanation: serde_json::Value =
        serde_json::from_slice(&explanation.stdout).expect("decode provenance explanation");
    let evidence_records = explanation["records"]
        .as_array()
        .expect("typed provenance records")
        .iter()
        .filter(|record| {
            record["kind"]["type"] == "source" && record["kind"]["value"]["type"] == "evidence"
        })
        .map(|record| &record["kind"]["value"]["value"])
        .collect::<Vec<_>>();
    assert_eq!(evidence_records.len(), 2);
    let file_evidence = evidence_records
        .iter()
        .find(|record| record["block_id"].is_null())
        .expect("file-level evidence record");
    assert_eq!(file_evidence["snapshot_id"], snapshot_id);
    assert_eq!(file_evidence["byte_start"], serde_json::Value::Null);
    let block_evidence = evidence_records
        .iter()
        .find(|record| record["block_id"] == block_id)
        .expect("block evidence record");
    assert_eq!(block_evidence["snapshot_id"], snapshot_id);
    assert_eq!(block_evidence["byte_start"], serde_json::Value::Null);
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
