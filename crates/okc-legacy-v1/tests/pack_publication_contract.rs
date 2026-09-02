#![allow(
    clippy::similar_names,
    reason = "compiler is the SDK facade while compiled names the immutable output under test"
)]

mod common;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File};
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use vaultc::{ApprovedPlan, CompileOptions, SourceSpec, VaultCompiler, VaultcError};

fn approved_basic_plan() -> (VaultCompiler, ApprovedPlan) {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect pack-publication fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan pack-publication fixture");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve pack-publication fixture");
    (compiler, approved)
}

fn compile_basic(compiler: &VaultCompiler, approved: &ApprovedPlan, destination: &Path) {
    compiler
        .compile(approved, destination)
        .expect("compile pack-publication fixture");
}

fn entry_names(parent: &Path) -> BTreeSet<OsString> {
    fs::read_dir(parent)
        .unwrap_or_else(|error| panic!("read {}: {error}", parent.display()))
        .map(|entry| entry.expect("publication parent entry").file_name())
        .collect()
}

fn expected_entries_with(before: &BTreeSet<OsString>, path: &Path) -> BTreeSet<OsString> {
    let mut expected = before.clone();
    expected.insert(
        path.file_name()
            .expect("published destination has a file name")
            .to_os_string(),
    );
    expected
}

fn assert_output_exists(error: &VaultcError, destination: &Path) {
    assert!(
        matches!(error, VaultcError::OutputExists(path) if path == destination),
        "existing publication leaf must be reported without replacement: {error:?}"
    );
}

#[test]
fn sdk_pack_existing_destination_is_preserved() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary publication parent");
    let compiled = temporary.path().join("compiled");
    compile_basic(&compiler, &approved, &compiled);
    let compiled_before = common::tree_bytes(&compiled);

    let existing_file = temporary.path().join("existing-file.vaultpack");
    let file_sentinel = b"existing pack owner\n";
    fs::write(&existing_file, file_sentinel).expect("write existing pack sentinel");
    let entries_before_file = entry_names(temporary.path());
    let file_error = vaultc::pack::create_pack(
        &compiled,
        &existing_file,
        compiler.policy().output.zstd_level,
    )
    .expect_err("existing regular-file destination must be rejected");
    assert_output_exists(&file_error, &existing_file);
    assert_eq!(
        fs::read(&existing_file).expect("read existing pack sentinel"),
        file_sentinel
    );
    assert_eq!(entry_names(temporary.path()), entries_before_file);

    let existing_directory = temporary.path().join("existing-directory.vaultpack");
    fs::create_dir(&existing_directory).expect("create existing pack directory");
    fs::write(existing_directory.join("sentinel"), "preserve directory\n")
        .expect("write existing directory sentinel");
    let directory_before = common::tree_bytes(&existing_directory);
    let entries_before_directory = entry_names(temporary.path());
    let directory_error = vaultc::pack::create_pack(
        &compiled,
        &existing_directory,
        compiler.policy().output.zstd_level,
    )
    .expect_err("existing directory destination must be rejected");
    assert_output_exists(&directory_error, &existing_directory);
    assert_eq!(common::tree_bytes(&existing_directory), directory_before);
    assert_eq!(entry_names(temporary.path()), entries_before_directory);
    assert_eq!(common::tree_bytes(&compiled), compiled_before);
}

#[cfg(unix)]
#[test]
fn sdk_pack_dangling_symlink_is_rejected_without_following_target() {
    use std::os::unix::fs::symlink;

    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary symlink publication parent");
    let compiled = temporary.path().join("compiled");
    compile_basic(&compiler, &approved, &compiled);
    let compiled_before = common::tree_bytes(&compiled);

    let dangling_target = temporary.path().join("must-not-be-created.vaultpack");
    let dangling_link = temporary.path().join("dangling.vaultpack");
    symlink(&dangling_target, &dangling_link).expect("create dangling pack symlink");
    let dangling_metadata = fs::symlink_metadata(&dangling_link).expect("stat dangling symlink");
    let entries_before_dangling = entry_names(temporary.path());
    let dangling_error = vaultc::pack::create_pack(
        &compiled,
        &dangling_link,
        compiler.policy().output.zstd_level,
    )
    .expect_err("dangling symlink leaf must fail closed");
    assert_output_exists(&dangling_error, &dangling_link);
    assert!(
        fs::symlink_metadata(&dangling_link)
            .expect("dangling link remains")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::symlink_metadata(&dangling_link)
            .expect("stat preserved dangling symlink")
            .file_type(),
        dangling_metadata.file_type()
    );
    assert!(
        !dangling_target.exists(),
        "publisher must not follow the dangling leaf and create its referent"
    );
    assert_eq!(entry_names(temporary.path()), entries_before_dangling);

    let live_target = temporary.path().join("live-owner.vaultpack");
    let live_sentinel = b"live symlink owner\n";
    fs::write(&live_target, live_sentinel).expect("write live symlink referent");
    let live_link = temporary.path().join("live.vaultpack");
    symlink(&live_target, &live_link).expect("create live pack symlink");
    let entries_before_live = entry_names(temporary.path());
    let live_error =
        vaultc::pack::create_pack(&compiled, &live_link, compiler.policy().output.zstd_level)
            .expect_err("live symlink leaf must fail closed");
    assert_output_exists(&live_error, &live_link);
    assert_eq!(
        fs::read(&live_target).expect("read preserved live symlink referent"),
        live_sentinel
    );
    assert!(
        fs::symlink_metadata(&live_link)
            .expect("live link remains")
            .file_type()
            .is_symlink()
    );
    assert_eq!(entry_names(temporary.path()), entries_before_live);
    assert_eq!(common::tree_bytes(&compiled), compiled_before);
}

#[test]
fn sdk_pack_destination_inside_compiled_vault_is_rejected_without_mutation() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary containment publication parent");
    let compiled = temporary.path().join("compiled");
    compile_basic(&compiler, &approved, &compiled);
    let compiled_before = common::tree_bytes(&compiled);

    let direct_inside = compiled.join("inside.vaultpack");
    let direct_error = vaultc::pack::create_pack(
        &compiled,
        &direct_inside,
        compiler.policy().output.zstd_level,
    )
    .expect_err("pack inside compiled Vault must be rejected");
    assert!(
        matches!(direct_error, VaultcError::UnsafePath { .. }),
        "direct containment must be classified as unsafe: {direct_error:?}"
    );
    assert!(!direct_inside.exists());

    fs::create_dir(compiled.join("nested")).expect("create lexical-alias directory");
    let lexical_inside = compiled.join("nested/../lexical.vaultpack");
    let lexical_error = vaultc::pack::create_pack(
        &compiled,
        &lexical_inside,
        compiler.policy().output.zstd_level,
    )
    .expect_err("lexically aliased pack inside compiled Vault must be rejected");
    assert!(
        matches!(lexical_error, VaultcError::UnsafePath { .. }),
        "lexical containment must be classified as unsafe: {lexical_error:?}"
    );
    assert!(!compiled.join("lexical.vaultpack").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let compiled_alias = temporary.path().join("compiled-alias");
        symlink(&compiled, &compiled_alias).expect("create compiled-Vault ancestor alias");
        let aliased_inside = compiled_alias.join("aliased.vaultpack");
        let alias_error = vaultc::pack::create_pack(
            &compiled,
            &aliased_inside,
            compiler.policy().output.zstd_level,
        )
        .expect_err("symlink-aliased pack inside compiled Vault must be rejected");
        assert!(
            matches!(alias_error, VaultcError::UnsafePath { .. }),
            "resolved-ancestor containment must be classified as unsafe: {alias_error:?}"
        );
        assert!(!compiled.join("aliased.vaultpack").exists());
    }
    assert_eq!(common::tree_bytes(&compiled), compiled_before);
}

#[test]
fn sdk_pack_equal_and_vault_inside_reserved_pack_path_are_rejected() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary reverse-containment parent");

    let equal = temporary.path().join("equal.vaultpack");
    compile_basic(&compiler, &approved, &equal);
    let equal_before = common::tree_bytes(&equal);
    let equal_error =
        vaultc::pack::create_pack(&equal, &equal, compiler.policy().output.zstd_level)
            .expect_err("equal compiled-Vault and pack paths must be rejected");
    assert!(matches!(
        equal_error,
        VaultcError::UnsafePath { .. } | VaultcError::OutputExists(_)
    ));
    assert_eq!(common::tree_bytes(&equal), equal_before);

    let reserved_pack = temporary.path().join("reserved.vaultpack");
    let nested_compiled = reserved_pack.join("compiled");
    compile_basic(&compiler, &approved, &nested_compiled);
    let nested_before = common::tree_bytes(&nested_compiled);
    let reverse_error = vaultc::pack::create_pack(
        &nested_compiled,
        &reserved_pack,
        compiler.policy().output.zstd_level,
    )
    .expect_err("compiled Vault inside path reserved for pack must be rejected");
    assert!(matches!(
        reverse_error,
        VaultcError::UnsafePath { .. } | VaultcError::OutputExists(_)
    ));
    assert_eq!(common::tree_bytes(&nested_compiled), nested_before);
}

#[test]
fn sdk_pack_portable_case_and_nfc_aliases_do_not_bypass_containment() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary portable-alias parent");

    let cases = [
        ("StraßeVault", "STRASSEVAULT"),
        ("CaféVault", "CAFE\u{301}VAULT"),
    ];
    for (compiled_spelling, alias_spelling) in cases {
        let compiled = temporary.path().join(compiled_spelling);
        compile_basic(&compiler, &approved, &compiled);
        let compiled_before = common::tree_bytes(&compiled);
        let aliased_inside = temporary
            .path()
            .join(alias_spelling)
            .join("portable-alias.vaultpack");
        let error = vaultc::pack::create_pack(
            &compiled,
            &aliased_inside,
            compiler.policy().output.zstd_level,
        )
        .expect_err("portable case/normalization alias must fail closed");
        assert!(
            matches!(error, VaultcError::UnsafePath { .. }),
            "portable containment alias must be unsafe: {error:?}"
        );
        assert!(!aliased_inside.exists());
        assert_eq!(common::tree_bytes(&compiled), compiled_before);
    }
}

#[test]
fn sdk_pack_failure_does_not_leave_partial_destination() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary failed-publication parent");
    let compiled = temporary.path().join("compiled");
    compile_basic(&compiler, &approved, &compiled);
    let topic = compiled.join("knowledge/Topic.md");
    let original_topic = fs::read(&topic).expect("read pristine compiled topic");
    fs::write(&topic, "tampered after compilation\n")
        .expect("tamper compiled Vault before public pack creation");
    let compiled_before = common::tree_bytes(&compiled);
    let pack = temporary.path().join("rejected.vaultpack");
    let entries_before = entry_names(temporary.path());

    let error = vaultc::pack::create_pack(&compiled, &pack, compiler.policy().output.zstd_level)
        .expect_err("publisher must independently reject a tampered compiled Vault");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));
    assert!(
        fs::symlink_metadata(&pack).is_err(),
        "pre-commit failure must leave the requested destination absent"
    );
    assert_eq!(
        entry_names(temporary.path()),
        entries_before,
        "caught failure must remove its sibling temporary pack"
    );
    assert_eq!(common::tree_bytes(&compiled), compiled_before);

    fs::write(&topic, original_topic).expect("restore exact sealed topic bytes");
    assert!(
        compiler
            .verify(&compiled)
            .expect("verify repaired compiled Vault")
            .valid
    );
    vaultc::pack::create_pack(&compiled, &pack, compiler.policy().output.zstd_level)
        .expect("retry at the same absent destination after repairing input");
    assert!(compiler.verify(&pack).expect("verify retry pack").valid);
}

#[test]
fn sdk_pack_concurrent_publish_has_exactly_one_winner() {
    const CREATOR_COUNT: usize = 12;

    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary concurrent-publication parent");
    let compiled = temporary.path().join("compiled");
    compile_basic(&compiler, &approved, &compiled);
    let compiled_before = common::tree_bytes(&compiled);
    let pack = temporary.path().join("winner.vaultpack");
    let entries_before = entry_names(temporary.path());
    let barrier = Arc::new(Barrier::new(CREATOR_COUNT));
    let zstd_level = compiler.policy().output.zstd_level;

    let handles = (0..CREATOR_COUNT)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let compiled = compiled.clone();
            let pack = pack.clone();
            thread::spawn(move || {
                barrier.wait();
                vaultc::pack::create_pack(&compiled, &pack, zstd_level)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("pack creator must not panic"))
        .collect::<Vec<_>>();
    let winner_count = results.iter().filter(|result| result.is_ok()).count();
    assert_eq!(winner_count, 1, "exactly one no-clobber publish may win");
    for error in results.iter().filter_map(|result| result.as_ref().err()) {
        assert_output_exists(error, &pack);
    }

    assert!(compiler.verify(&pack).expect("verify winning pack").valid);
    assert_eq!(common::tree_bytes(&compiled), compiled_before);
    assert_eq!(
        entry_names(temporary.path()),
        expected_entries_with(&entries_before, &pack),
        "all losing sibling staging files must be removed"
    );
}

// CompileOptions integration assertions are below the direct publisher tests so
// this contract still exercises the public publisher independently. The SDK
// facade is required to delegate to the same implementation.
#[test]
fn compile_options_reject_output_pack_aliases_before_publication() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary compile-options parent");

    let cases = [
        (
            temporary.path().join("equal.vaultpack"),
            temporary.path().join("equal.vaultpack"),
        ),
        (
            temporary.path().join("output-with-inner-pack"),
            temporary
                .path()
                .join("output-with-inner-pack/inner.vaultpack"),
        ),
        (
            temporary.path().join("outer.vaultpack/compiled"),
            temporary.path().join("outer.vaultpack"),
        ),
        (
            temporary.path().join("CaféOutput"),
            temporary.path().join("CAFE\u{301}OUTPUT/inner.vaultpack"),
        ),
        (
            temporary.path().join("StraßeOutput"),
            temporary.path().join("STRASSEOUTPUT/inner.vaultpack"),
        ),
    ];

    for (compiled, pack) in cases {
        let error = compiler
            .compile_with_options(
                &approved,
                &compiled,
                &CompileOptions {
                    create_pack: Some(pack.clone()),
                },
            )
            .expect_err("output/pack alias must fail during preflight");
        assert!(
            matches!(error, VaultcError::UnsafePath { .. }),
            "alias must be rejected as an unsafe path: {error:?}"
        );
        assert!(!compiled.exists(), "preflight must not publish the output");
        assert!(!pack.exists(), "preflight must not publish a pack");
    }
}

#[test]
fn compile_with_existing_pack_fails_preflight_and_leaves_output_absent() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary compile preflight parent");
    let compiled = temporary.path().join("must-remain-absent");
    let pack = temporary.path().join("existing.vaultpack");
    let sentinel = b"pre-existing publisher\n";
    fs::write(&pack, sentinel).expect("write pre-existing pack");
    let entries_before = entry_names(temporary.path());

    let error = compiler
        .compile_with_options(
            &approved,
            &compiled,
            &CompileOptions {
                create_pack: Some(pack.clone()),
            },
        )
        .expect_err("existing requested pack must fail before compiling");
    assert_output_exists(&error, &pack);
    assert!(!compiled.exists());
    assert_eq!(fs::read(&pack).expect("read existing pack"), sentinel);
    assert_eq!(entry_names(temporary.path()), entries_before);
}

#[test]
fn separate_and_compile_with_options_publish_byte_identical_results() {
    let (compiler, approved) = approved_basic_plan();
    let temporary = tempfile::tempdir().expect("temporary deterministic publication parent");
    let separate_output = temporary.path().join("separate-output");
    let integrated_output = temporary.path().join("integrated-output");
    let separate_pack = temporary.path().join("separate.vaultpack");
    let integrated_pack = temporary.path().join("integrated.VAULTPACK");
    let source_before = common::tree_bytes(&common::fixture("basic_vault"));

    compile_basic(&compiler, &approved, &separate_output);
    vaultc::pack::create_pack(
        &separate_output,
        &separate_pack,
        compiler.policy().output.zstd_level,
    )
    .expect("publish separate pack");
    compiler
        .compile_with_options(
            &approved,
            &integrated_output,
            &CompileOptions {
                create_pack: Some(integrated_pack.clone()),
            },
        )
        .expect("compile and publish integrated pack");

    assert_eq!(
        common::tree_bytes(&separate_output),
        common::tree_bytes(&integrated_output)
    );
    assert_eq!(
        fs::read(&separate_pack).expect("read separate pack"),
        fs::read(&integrated_pack).expect("read integrated pack")
    );
    assert!(
        compiler
            .verify(&separate_pack)
            .expect("verify separate pack")
            .valid
    );
    assert!(
        compiler
            .verify(&integrated_pack)
            .expect("verify integrated pack")
            .valid
    );
    assert_eq!(
        common::tree_bytes(&common::fixture("basic_vault")),
        source_before,
        "neither publication path may mutate the source Vault"
    );
}

#[test]
fn compile_with_pack_failure_leaves_valid_compiled_vault_and_no_partial_pack() {
    // The pack path is raced only after the compiled directory appears. Eight
    // MiB keeps the verifier busy long enough for that observation without
    // turning this integration contract into a stress benchmark.
    const LARGE_FIXTURE_BYTES: u64 = 8 * 1024 * 1024;

    let temporary = tempfile::tempdir().expect("temporary runtime-failure parent");
    let source = temporary.path().join("source");
    fs::create_dir(&source).expect("create runtime-failure source");
    fs::write(source.join("Index.md"), "# Runtime failure fixture\n")
        .expect("write runtime-failure note");
    let large_asset = source.join("large.bin");
    File::create(&large_asset)
        .expect("create large deterministic fixture asset")
        .set_len(LARGE_FIXTURE_BYTES)
        .expect("size large deterministic fixture asset");
    let source_before = common::tree_bytes(&source);

    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("runtime", &source).expect("runtime fixture source")])
        .expect("inspect runtime-failure fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan runtime-failure fixture");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve runtime-failure fixture");
    let compiled = temporary.path().join("compiled-after-pack-failure");
    let pack_parent = temporary.path().join("pack-parent");
    fs::create_dir(&pack_parent).expect("create pack parent for post-preflight race");
    let pack = pack_parent.join("raced.vaultpack");

    let observed_output = compiled.clone();
    let raced_parent = pack_parent.clone();
    let racer = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !observed_output.exists() {
            assert!(
                Instant::now() < deadline,
                "compiled output was not published before the race deadline"
            );
            thread::yield_now();
        }
        fs::remove_dir(&raced_parent).expect("remove empty pack parent after preflight");
        fs::write(&raced_parent, "external namespace race\n")
            .expect("replace pack parent with external regular file");
    });

    let error = compiler
        .compile_with_options(
            &approved,
            &compiled,
            &CompileOptions {
                create_pack: Some(pack.clone()),
            },
        )
        .expect_err("runtime pack-parent race must fail after output publication");
    racer
        .join()
        .expect("runtime namespace racer must not panic");

    match error {
        VaultcError::PackPublicationAfterCompile {
            compiled_vault,
            pack: failed_pack,
            ..
        } => {
            assert_eq!(compiled_vault, compiled);
            assert_eq!(failed_pack, pack);
        }
        other => panic!("runtime publication failure must preserve phase state: {other:?}"),
    }
    assert!(
        compiler
            .verify(&compiled)
            .expect("verify output retained after pack failure")
            .valid
    );
    assert!(
        fs::symlink_metadata(&pack).is_err(),
        "vaultc must leave no requested pack or partial pack"
    );
    assert_eq!(
        fs::read_to_string(&pack_parent).expect("read external race winner"),
        "external namespace race\n"
    );
    assert_eq!(
        common::tree_bytes(&source),
        source_before,
        "compile-plus-pack orchestration must not mutate its source"
    );
}
