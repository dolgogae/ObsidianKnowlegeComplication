mod common;

use std::fs;
use std::path::Path;

use vaultc::{SourceSpec, VaultcError};

fn compile_and_pack(source: &Path, artifact_root: &Path, pack: &Path) {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("basic", source).expect("absolute fixture source")])
        .expect("inspect absolute fixture source");
    let plan = compiler.plan(&inspection).expect("plan absolute fixture");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve absolute fixture plan");
    compiler
        .compile(&approved, artifact_root)
        .expect("compile absolute fixture");
    vaultc::pack::create_pack(artifact_root, pack, compiler.policy().output.zstd_level)
        .expect("create absolute fixture VaultPack");
}

#[test]
fn vaultpack_is_byte_deterministic_verifiable_and_explainable() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect pack source");
    let plan = compiler.plan(&inspection).expect("plan pack source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve pack plan");
    let temporary = tempfile::tempdir().expect("temporary pack parent");
    let artifact_root = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact_root)
        .expect("compile pack source");

    let first = temporary.path().join("first.vaultpack");
    let second = temporary.path().join("second.vaultpack");
    vaultc::pack::create_pack(&artifact_root, &first, compiler.policy().output.zstd_level)
        .expect("create first VaultPack");
    vaultc::pack::create_pack(&artifact_root, &second, compiler.policy().output.zstd_level)
        .expect("create second VaultPack");

    assert_eq!(
        fs::read(&first).expect("read first pack"),
        fs::read(&second).expect("read second pack")
    );
    assert!(compiler.verify(&first).expect("verify first pack").valid);
    assert!(compiler.verify(&second).expect("verify second pack").valid);
    let explanation = compiler
        .explain_provenance(&first, "knowledge/Index.md")
        .expect("explain output directly from pack");
    assert_eq!(explanation.output_path, "knowledge/Index.md");
    assert_eq!(explanation.records.len(), 1);
}

#[test]
fn vaultpack_verifier_rejects_a_valid_archive_with_tampered_content() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect pack source");
    let plan = compiler.plan(&inspection).expect("plan pack source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve pack plan");
    let temporary = tempfile::tempdir().expect("temporary pack parent");
    let artifact_root = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact_root)
        .expect("compile pack source");
    fs::write(
        artifact_root.join("knowledge/Topic.md"),
        "tampered after compile\n",
    )
    .expect("tamper compiled content while retaining stale checksums");

    let pack = temporary.path().join("tampered.vaultpack");
    vaultc::pack::create_pack(&artifact_root, &pack, compiler.policy().output.zstd_level)
        .expect("package structurally valid tampered artifact");
    let error = compiler
        .verify(&pack)
        .expect_err("tampered VaultPack must fail closed");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));
}

#[test]
fn absolute_source_locations_do_not_affect_artifact_or_vaultpack_bytes() {
    let temporary = tempfile::tempdir().expect("temporary cross-location parent");
    let first_source = temporary.path().join("first-source");
    let second_source = temporary.path().join("nested/second-source");
    common::copy_tree(&common::fixture("basic_vault"), &first_source);
    common::copy_tree(&common::fixture("basic_vault"), &second_source);

    let first_artifact = temporary.path().join("first-artifact");
    let second_artifact = temporary.path().join("second-artifact");
    let first_pack = temporary.path().join("first-location.vaultpack");
    let second_pack = temporary.path().join("second-location.vaultpack");
    compile_and_pack(&first_source, &first_artifact, &first_pack);
    compile_and_pack(&second_source, &second_artifact, &second_pack);

    assert_eq!(
        common::tree_bytes(&first_artifact),
        common::tree_bytes(&second_artifact),
        "compiled artifact bytes must not disclose or depend on source location"
    );
    assert_eq!(
        fs::read(&first_pack).expect("read first cross-location pack"),
        fs::read(&second_pack).expect("read second cross-location pack"),
        "VaultPack bytes must not depend on source location"
    );

    let source_locators = [
        first_source
            .to_str()
            .expect("first temporary source path is UTF-8"),
        second_source
            .to_str()
            .expect("second temporary source path is UTF-8"),
    ];
    for artifact in [&first_artifact, &second_artifact] {
        let audit_plan = fs::read_to_string(artifact.join(".vaultc/plan.json"))
            .expect("read compiled audit plan");
        for locator in source_locators {
            assert!(
                !audit_plan.contains(locator),
                "compiled audit plan must redact source locator `{locator}`"
            );
        }
        assert!(audit_plan.contains("[redacted-source-locator]"));
    }
}
