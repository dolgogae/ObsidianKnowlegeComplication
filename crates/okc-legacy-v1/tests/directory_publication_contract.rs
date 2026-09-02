#![allow(
    clippy::similar_names,
    reason = "compiler is the SDK facade while compiled names a published directory"
)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use vaultc::{ApprovedPlan, CompileOptions, SourceSpec, VaultCompiler, VaultcError};

fn approved_directory_source(source_id: &str, source: &Path) -> (VaultCompiler, ApprovedPlan) {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([
            SourceSpec::directory(source_id, source).expect("valid directory source descriptor")
        ])
        .expect("inspect directory-publication source");
    let plan = compiler
        .plan(&inspection)
        .expect("plan directory-publication source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve directory-publication plan");
    (compiler, approved)
}

fn approved_basic_plan() -> (VaultCompiler, ApprovedPlan) {
    approved_directory_source("directory-publication", &common::fixture("basic_vault"))
}

fn entry_names(parent: &Path) -> BTreeSet<OsString> {
    fs::read_dir(parent)
        .unwrap_or_else(|error| panic!("read {}: {error}", parent.display()))
        .map(|entry| entry.expect("publication parent entry").file_name())
        .collect()
}

fn entry_shape(root: &Path) -> BTreeSet<String> {
    fn visit(root: &Path, directory: &Path, output: &mut BTreeSet<String>) {
        let entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
        for entry in entries {
            let entry = entry.expect("directory-publication tree entry");
            let path = entry.path();
            let file_type = entry
                .file_type()
                .unwrap_or_else(|error| panic!("type {}: {error}", path.display()));
            let relative = path
                .strip_prefix(root)
                .expect("entry beneath observation root")
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let kind = if file_type.is_dir() {
                "directory"
            } else if file_type.is_symlink() {
                "symlink"
            } else if file_type.is_file() {
                "file"
            } else {
                "other"
            };
            output.insert(format!("{kind}:{relative}"));
            if file_type.is_dir() {
                visit(root, &path, output);
            }
        }
    }

    let mut output = BTreeSet::new();
    visit(root, root, &mut output);
    output
}

fn staging_paths(root: &Path) -> Vec<PathBuf> {
    fn visit(directory: &Path, output: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries {
            let entry = entry.expect("staging residue entry");
            let path = entry.path();
            let file_type = entry.file_type().expect("staging residue entry type");
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".vaultc-staging-")
            {
                output.push(path);
            } else if file_type.is_dir() {
                visit(&path, output);
            }
        }
    }

    let mut output = Vec::new();
    visit(root, &mut output);
    output.sort();
    output
}

fn assert_output_exists(error: &VaultcError, destination: &Path) {
    assert!(
        matches!(error, VaultcError::OutputExists(path) if path == destination),
        "existing destination must be classified without replacement: {error:?}"
    );
}

fn write_distinct_source(root: &Path, label: &str) {
    fs::create_dir_all(root.join("assets")).expect("create distinct source directories");
    fs::write(
        root.join("Index.md"),
        format!("# {label}\n\nThis artifact belongs to {label}.\n"),
    )
    .expect("write distinct source note");
    fs::write(root.join("assets/identity.bin"), label.as_bytes())
        .expect("write distinct source attachment");
}

fn assert_unsafe_source_output(
    observation_root: &Path,
    source: &Path,
    output: &Path,
    source_id: &str,
) {
    let (compiler, approved) = approved_directory_source(source_id, source);
    let source_before = common::tree_bytes(source);
    let shape_before = entry_shape(observation_root);
    let stages_before = staging_paths(observation_root);

    let error = compiler
        .compile(&approved, output)
        .expect_err("source/output overlap must fail before staging");
    assert!(
        matches!(error, VaultcError::UnsafePath { .. }),
        "source/output overlap must be unsafe: {error:?}"
    );
    assert_eq!(common::tree_bytes(source), source_before);
    assert_eq!(entry_shape(observation_root), shape_before);
    assert_eq!(staging_paths(observation_root), stages_before);
}

fn assert_unsafe_integrated_pack_destination(
    observation_root: &Path,
    source: &Path,
    compiler: &VaultCompiler,
    approved: &ApprovedPlan,
    output: &Path,
    pack: &Path,
) {
    let source_before = common::tree_bytes(source);
    let shape_before = entry_shape(observation_root);
    let stages_before = staging_paths(observation_root);

    let error = compiler
        .compile_with_options(
            approved,
            output,
            &CompileOptions {
                create_pack: Some(pack.to_path_buf()),
            },
        )
        .expect_err("pack destination overlapping an input source must fail before staging");
    assert!(
        matches!(error, VaultcError::UnsafePath { .. }),
        "source/pack overlap must be unsafe: {error:?}"
    );
    assert!(
        fs::symlink_metadata(output).is_err(),
        "directory output must remain absent"
    );
    assert!(
        fs::symlink_metadata(pack).is_err(),
        "pack output must remain absent"
    );
    assert_eq!(common::tree_bytes(source), source_before);
    assert_eq!(entry_shape(observation_root), shape_before);
    assert_eq!(staging_paths(observation_root), stages_before);
}

#[test]
fn sdk_directory_existing_file_and_directory_are_preserved() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary existing-output parent");

    let existing_file = temporary.path().join("existing-file");
    let file_sentinel = b"owned by another publisher\n";
    fs::write(&existing_file, file_sentinel).expect("write existing output file");
    let entries_before_file = entry_names(temporary.path());
    let file_error = compiler
        .compile(&approved, &existing_file)
        .expect_err("existing regular file must be rejected");
    assert_output_exists(&file_error, &existing_file);
    assert_eq!(
        fs::read(&existing_file).expect("read existing output file"),
        file_sentinel
    );
    assert_eq!(entry_names(temporary.path()), entries_before_file);
    assert!(staging_paths(temporary.path()).is_empty());

    let empty_directory = temporary.path().join("existing-empty-directory");
    fs::create_dir(&empty_directory).expect("create existing empty output directory");
    let entries_before_empty = entry_names(temporary.path());
    let empty_error = compiler
        .compile(&approved, &empty_directory)
        .expect_err("existing empty directory must be rejected");
    assert_output_exists(&empty_error, &empty_directory);
    assert!(
        entry_names(&empty_directory).is_empty(),
        "empty directory winner must not be populated"
    );
    assert_eq!(entry_names(temporary.path()), entries_before_empty);
    assert!(staging_paths(temporary.path()).is_empty());

    let nonempty_directory = temporary.path().join("existing-nonempty-directory");
    fs::create_dir(&nonempty_directory).expect("create existing nonempty output directory");
    fs::write(nonempty_directory.join("sentinel"), "preserve directory\n")
        .expect("write existing directory sentinel");
    let directory_before = common::tree_bytes(&nonempty_directory);
    let entries_before_nonempty = entry_names(temporary.path());
    let nonempty_error = compiler
        .compile(&approved, &nonempty_directory)
        .expect_err("existing nonempty directory must be rejected");
    assert_output_exists(&nonempty_error, &nonempty_directory);
    assert_eq!(common::tree_bytes(&nonempty_directory), directory_before);
    assert_eq!(entry_names(temporary.path()), entries_before_nonempty);
    assert!(staging_paths(temporary.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn sdk_directory_live_and_dangling_symlinks_are_rejected_without_following_referents() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary symlink-output parent");
    let source = temporary.path().join("source");
    common::copy_tree(&common::fixture("basic_vault"), &source);
    let source_before = common::tree_bytes(&source);
    let (compiler, approved) = approved_directory_source("symlink-output", &source);

    let live_referent = temporary.path().join("live-referent");
    fs::create_dir(&live_referent).expect("create live directory referent");
    fs::write(live_referent.join("sentinel"), "preserve live referent\n")
        .expect("write live referent sentinel");
    let live_before = common::tree_bytes(&live_referent);
    let live_link = temporary.path().join("live-output");
    symlink(&live_referent, &live_link).expect("create live output symlink");
    let live_target_before = fs::read_link(&live_link).expect("read live output link");
    let live_error = compiler
        .compile(&approved, &live_link)
        .expect_err("live symlink output must be rejected");
    assert_output_exists(&live_error, &live_link);
    assert!(
        fs::symlink_metadata(&live_link)
            .expect("stat preserved live link")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&live_link).expect("reread live link"),
        live_target_before
    );
    assert_eq!(common::tree_bytes(&live_referent), live_before);

    let dangerous_referent = source.join("must-not-be-created");
    let dangling_link = temporary.path().join("dangling-output");
    symlink(&dangerous_referent, &dangling_link).expect("create dangerous dangling output link");
    let dangling_target_before = fs::read_link(&dangling_link).expect("read dangling link");
    let dangling_error = compiler
        .compile(&approved, &dangling_link)
        .expect_err("dangling symlink output must be rejected");
    assert_output_exists(&dangling_error, &dangling_link);
    assert!(
        fs::symlink_metadata(&dangling_link)
            .expect("stat preserved dangling link")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&dangling_link).expect("reread dangling link"),
        dangling_target_before
    );
    assert!(!dangerous_referent.exists());
    assert_eq!(common::tree_bytes(&source), source_before);
    assert!(staging_paths(temporary.path()).is_empty());
}

#[test]
fn sdk_directory_concurrent_creators_have_exactly_one_winner() {
    const CREATOR_COUNT: usize = 12;

    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary concurrent-output parent");
    let destination = temporary.path().join("winner");
    let barrier = Arc::new(Barrier::new(CREATOR_COUNT));
    let approved = Arc::new(approved);

    let handles = (0..CREATOR_COUNT)
        .map(|_| {
            let compiler = compiler.clone();
            let approved = Arc::clone(&approved);
            let destination = destination.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                compiler.compile(&approved, &destination)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("directory creator must not panic"))
        .collect::<Vec<_>>();
    let winners = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect::<Vec<_>>();
    assert_eq!(winners.len(), 1, "exactly one directory commit may win");
    for error in results.iter().filter_map(|result| result.as_ref().err()) {
        assert_output_exists(error, &destination);
    }

    let report = compiler
        .verify(&destination)
        .expect("verify concurrent directory winner");
    assert!(report.valid);
    assert_eq!(report.artifact_id, winners[0].artifact_id.hex());
    assert!(staging_paths(temporary.path()).is_empty());
}

#[test]
fn sdk_directory_distinct_concurrent_builds_never_mix_or_replace_winner() {
    let temporary = tempfile::tempdir().expect("temporary distinct-race parent");
    let alpha_source = temporary.path().join("alpha-source");
    let beta_source = temporary.path().join("beta-source");
    write_distinct_source(&alpha_source, "Alpha");
    write_distinct_source(&beta_source, "Beta");
    let alpha_before = common::tree_bytes(&alpha_source);
    let beta_before = common::tree_bytes(&beta_source);
    let (alpha_compiler, alpha_approved) = approved_directory_source("alpha", &alpha_source);
    let (beta_compiler, beta_approved) = approved_directory_source("beta", &beta_source);

    let alpha_expected_path = temporary.path().join("expected-alpha");
    let beta_expected_path = temporary.path().join("expected-beta");
    let alpha_expected = alpha_compiler
        .compile(&alpha_approved, &alpha_expected_path)
        .expect("compile expected Alpha artifact");
    let beta_expected = beta_compiler
        .compile(&beta_approved, &beta_expected_path)
        .expect("compile expected Beta artifact");
    let expected_trees = BTreeMap::from([
        (
            alpha_expected.artifact_id.hex(),
            common::tree_bytes(&alpha_expected_path),
        ),
        (
            beta_expected.artifact_id.hex(),
            common::tree_bytes(&beta_expected_path),
        ),
    ]);
    assert_eq!(
        expected_trees.len(),
        2,
        "candidate artifacts must be distinct"
    );

    let destination = temporary.path().join("distinct-winner");
    let barrier = Arc::new(Barrier::new(2));
    let contenders = [
        (alpha_compiler.clone(), alpha_approved),
        (beta_compiler.clone(), beta_approved),
    ];
    let handles = contenders
        .into_iter()
        .map(|(compiler, approved)| {
            let barrier = Arc::clone(&barrier);
            let destination = destination.clone();
            thread::spawn(move || {
                barrier.wait();
                compiler.compile(&approved, &destination)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("distinct creator must not panic"))
        .collect::<Vec<_>>();
    let winners = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect::<Vec<_>>();
    assert_eq!(winners.len(), 1, "one distinct candidate must win");
    for error in results.iter().filter_map(|result| result.as_ref().err()) {
        assert_output_exists(error, &destination);
    }

    let winner_id = winners[0].artifact_id.hex();
    assert_eq!(
        common::tree_bytes(&destination),
        *expected_trees
            .get(&winner_id)
            .expect("winner identity must name one complete candidate")
    );
    let report = alpha_compiler
        .verify(&destination)
        .expect("verify distinct directory winner");
    assert!(report.valid);
    assert_eq!(report.artifact_id, winner_id);
    assert_eq!(common::tree_bytes(&alpha_source), alpha_before);
    assert_eq!(common::tree_bytes(&beta_source), beta_before);
    assert!(staging_paths(temporary.path()).is_empty());
}

#[test]
fn sdk_directory_source_output_equality_and_containment_are_rejected_before_staging() {
    let temporary = tempfile::tempdir().expect("temporary overlap-matrix parent");

    let equal_case = temporary.path().join("equal-case");
    let equal_source = equal_case.join("source");
    write_distinct_source(&equal_source, "Equal");
    assert_unsafe_source_output(&equal_case, &equal_source, &equal_source, "overlap-equal");

    let output_inside_case = temporary.path().join("output-inside-case");
    let containing_source = output_inside_case.join("source");
    write_distinct_source(&containing_source, "OutputInside");
    let output_inside_source = containing_source.join("compiled");
    assert_unsafe_source_output(
        &output_inside_case,
        &containing_source,
        &output_inside_source,
        "overlap-output-inside",
    );

    let source_inside_case = temporary.path().join("source-inside-case");
    let containing_output = source_inside_case.join("reserved-output");
    let nested_source = containing_output.join("source");
    write_distinct_source(&nested_source, "SourceInside");
    assert_unsafe_source_output(
        &source_inside_case,
        &nested_source,
        &containing_output,
        "overlap-source-inside",
    );
}

#[test]
fn sdk_directory_source_output_lexical_case_and_normalization_aliases_are_rejected() {
    let temporary = tempfile::tempdir().expect("temporary portable-overlap parent");

    let lexical_case = temporary.path().join("lexical-case");
    let lexical_source = lexical_case.join("source");
    write_distinct_source(&lexical_source, "Lexical");
    fs::create_dir(lexical_source.join("nested")).expect("create lexical alias segment");
    let lexical_output = lexical_source.join("nested/../compiled");
    assert_unsafe_source_output(
        &lexical_case,
        &lexical_source,
        &lexical_output,
        "overlap-lexical",
    );

    let casefold_case = temporary.path().join("casefold-case");
    fs::create_dir(&casefold_case).expect("create casefold observation root");
    let casefold_source = casefold_case.join("StraßeSource");
    write_distinct_source(&casefold_source, "Casefold");
    let casefold_output = casefold_case.join("STRASSESOURCE/compiled");
    assert_unsafe_source_output(
        &casefold_case,
        &casefold_source,
        &casefold_output,
        "overlap-casefold",
    );

    let normalization_case = temporary.path().join("normalization-case");
    fs::create_dir(&normalization_case).expect("create normalization observation root");
    let normalization_source = normalization_case.join("CaféSource");
    write_distinct_source(&normalization_source, "Normalization");
    let normalization_output = normalization_case.join("CAFE\u{301}SOURCE/compiled");
    assert_unsafe_source_output(
        &normalization_case,
        &normalization_source,
        &normalization_output,
        "overlap-normalization",
    );
}

#[cfg(unix)]
#[test]
fn sdk_directory_source_output_existing_ancestor_symlink_alias_is_rejected() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary source-alias parent");
    let source = temporary.path().join("source");
    write_distinct_source(&source, "SymlinkAlias");
    let source_alias = temporary.path().join("source-alias");
    symlink(&source, &source_alias).expect("create existing source ancestor alias");
    let output = source_alias.join("compiled");
    assert_unsafe_source_output(temporary.path(), &source, &output, "overlap-symlink-alias");
}

#[test]
fn sdk_directory_success_is_deterministic_and_source_immutable_without_staging_residue() {
    let source = common::fixture("basic_vault");
    let source_before = common::tree_bytes(&source);
    let (compiler, approved) = approved_directory_source("deterministic-directory", &source);
    let temporary = tempfile::tempdir().expect("temporary deterministic-output parent");
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");

    let first_artifact = compiler
        .compile(&approved, &first)
        .expect("publish first deterministic directory");
    let second_artifact = compiler
        .compile(&approved, &second)
        .expect("publish second deterministic directory");

    assert_eq!(first_artifact.artifact_id, second_artifact.artifact_id);
    assert_eq!(common::tree_bytes(&first), common::tree_bytes(&second));
    assert!(
        compiler
            .verify(&first)
            .expect("verify first directory")
            .valid
    );
    assert!(
        compiler
            .verify(&second)
            .expect("verify second directory")
            .valid
    );
    assert_eq!(common::tree_bytes(&source), source_before);
    assert!(staging_paths(temporary.path()).is_empty());
}

#[test]
fn compile_with_pack_has_exact_success_and_preflight_residual_states() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary compile-plus-pack parent");

    let successful_output = temporary.path().join("successful-output");
    let successful_pack = temporary.path().join("successful.vaultpack");
    compiler
        .compile_with_options(
            &approved,
            &successful_output,
            &CompileOptions {
                create_pack: Some(successful_pack.clone()),
            },
        )
        .expect("publish directory followed by pack");
    assert!(
        compiler
            .verify(&successful_output)
            .expect("verify successful output")
            .valid
    );
    assert!(
        compiler
            .verify(&successful_pack)
            .expect("verify successful pack")
            .valid
    );

    let existing_output = temporary.path().join("external-output-winner");
    fs::create_dir(&existing_output).expect("create external output winner");
    let external_sentinel = existing_output.join("sentinel");
    fs::write(&external_sentinel, "external output\n").expect("write external output sentinel");
    let skipped_pack = temporary.path().join("must-not-start.vaultpack");
    let output_error = compiler
        .compile_with_options(
            &approved,
            &existing_output,
            &CompileOptions {
                create_pack: Some(skipped_pack.clone()),
            },
        )
        .expect_err("directory failure must prevent optional pack publication");
    assert_output_exists(&output_error, &existing_output);
    assert_eq!(
        fs::read_to_string(&external_sentinel).expect("read external output sentinel"),
        "external output\n"
    );
    assert!(!skipped_pack.exists());

    let absent_output = temporary.path().join("preflight-output-must-remain-absent");
    let existing_pack = temporary.path().join("external-pack-winner.vaultpack");
    let pack_sentinel = b"external pack\n";
    fs::write(&existing_pack, pack_sentinel).expect("write external pack winner");
    let pack_error = compiler
        .compile_with_options(
            &approved,
            &absent_output,
            &CompileOptions {
                create_pack: Some(existing_pack.clone()),
            },
        )
        .expect_err("pack preflight must run before directory staging");
    assert_output_exists(&pack_error, &existing_pack);
    assert!(!absent_output.exists());
    assert_eq!(
        fs::read(&existing_pack).expect("read external pack winner"),
        pack_sentinel
    );
    assert!(staging_paths(temporary.path()).is_empty());
}

#[test]
fn compile_with_pack_rejects_pack_inside_or_aliased_into_immutable_source() {
    let temporary = tempfile::tempdir().expect("temporary source/pack overlap parent");
    let source = temporary.path().join("source");
    common::copy_tree(&common::fixture("basic_vault"), &source);
    let (compiler, approved) = approved_directory_source("integrated-pack-overlap", &source);

    let nested_pack = source.join("assets/must-not-create.vaultpack");
    let nested_output = temporary.path().join("nested-pack-output");
    assert_unsafe_integrated_pack_destination(
        temporary.path(),
        &source,
        &compiler,
        &approved,
        &nested_output,
        &nested_pack,
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let source_alias = temporary.path().join("source-alias");
        symlink(&source, &source_alias).expect("create source alias for pack destination");
        let alias_target_before = fs::read_link(&source_alias).expect("read source alias");
        let aliased_pack = source_alias.join("must-not-create.vaultpack");
        let aliased_output = temporary.path().join("aliased-pack-output");
        assert_unsafe_integrated_pack_destination(
            temporary.path(),
            &source,
            &compiler,
            &approved,
            &aliased_output,
            &aliased_pack,
        );
        assert_eq!(
            fs::read_link(&source_alias).expect("reread source alias"),
            alias_target_before
        );
    }
}
