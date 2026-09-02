mod common;

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::Path;

use okc_core::provenance::{ProvenanceQuery, ProvenanceRecordKind, ProvenanceSubject};
use okc_core::{OkcError, SourceSpec};
use sha2::{Digest as _, Sha256};

const BASIC_OKCPACK_SHA256: &str =
    "e9fd228d99269d0ae0c143c57e803bacd2e5166983b6fce92f25ee096cbc4f9c";

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
    okc_core::pack::create_pack(artifact_root, pack, compiler.policy().output.zstd_level)
        .expect("create absolute fixture OKCPack");
}

#[test]
fn okcpack_is_byte_deterministic_verifiable_and_explainable() {
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

    let first = temporary.path().join("first.okcpack");
    let second = temporary.path().join("second.okcpack");
    okc_core::pack::create_pack(&artifact_root, &first, compiler.policy().output.zstd_level)
        .expect("create first OKCPack");
    okc_core::pack::create_pack(&artifact_root, &second, compiler.policy().output.zstd_level)
        .expect("create second OKCPack");

    let first_bytes = fs::read(&first).expect("read first pack");
    let second_bytes = fs::read(&second).expect("read second pack");
    assert_eq!(first_bytes, second_bytes);
    assert_eq!(
        hex::encode(Sha256::digest(&first_bytes)),
        BASIC_OKCPACK_SHA256,
        "the basic OKCPack is a cross-platform byte golden"
    );
    assert!(compiler.verify(&first).expect("verify first pack").valid);
    assert!(compiler.verify(&second).expect("verify second pack").valid);
    let explanation = compiler
        .explain_provenance(&first, "knowledge/Index.md")
        .expect("explain output directly from pack");
    assert_eq!(
        explanation.subject,
        ProvenanceSubject::ArtifactPath {
            path: "knowledge/Index.md".into()
        }
    );
    assert!(explanation.records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Output(output) if output.subject == explanation.subject
    )));
}

#[test]
fn source_argument_order_does_not_change_inspection_plan_output_or_okcpack() {
    let compiler = common::compiler();
    let forward_inspection = compiler
        .inspect([
            common::fixture_source("alpha", "collision_alpha"),
            common::fixture_source("beta", "collision_beta"),
        ])
        .expect("inspect forward source order");
    let reverse_inspection = compiler
        .inspect([
            common::fixture_source("beta", "collision_beta"),
            common::fixture_source("alpha", "collision_alpha"),
        ])
        .expect("inspect reverse source order");
    assert_eq!(
        serde_json::to_vec(&forward_inspection).expect("encode forward inspection"),
        serde_json::to_vec(&reverse_inspection).expect("encode reverse inspection")
    );

    let forward_plan = compiler
        .plan(&forward_inspection)
        .expect("plan forward source order");
    let reverse_plan = compiler
        .plan(&reverse_inspection)
        .expect("plan reverse source order");
    assert_eq!(
        serde_json::to_vec(&forward_plan).expect("encode forward plan"),
        serde_json::to_vec(&reverse_plan).expect("encode reverse plan")
    );
    let forward_approved = compiler
        .approve_without_augmentation(forward_plan)
        .expect("approve forward plan");
    let reverse_approved = compiler
        .approve_without_augmentation(reverse_plan)
        .expect("approve reverse plan");

    let temporary = tempfile::tempdir().expect("temporary order-invariance parent");
    let forward_output = temporary.path().join("forward");
    let reverse_output = temporary.path().join("reverse");
    compiler
        .compile(&forward_approved, &forward_output)
        .expect("compile forward plan");
    compiler
        .compile(&reverse_approved, &reverse_output)
        .expect("compile reverse plan");
    assert_eq!(
        common::tree_bytes(&forward_output),
        common::tree_bytes(&reverse_output)
    );

    let forward_pack = temporary.path().join("forward.okcpack");
    let reverse_pack = temporary.path().join("reverse.okcpack");
    okc_core::pack::create_pack(
        &forward_output,
        &forward_pack,
        compiler.policy().output.zstd_level,
    )
    .expect("pack forward output");
    okc_core::pack::create_pack(
        &reverse_output,
        &reverse_pack,
        compiler.policy().output.zstd_level,
    )
    .expect("pack reverse output");
    assert_eq!(
        fs::read(forward_pack).expect("read forward pack"),
        fs::read(reverse_pack).expect("read reverse pack")
    );
}

#[test]
fn public_pack_creation_rejects_tampered_input_without_destination() {
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

    let pack = temporary.path().join("tampered.okcpack");
    let entries_before: BTreeSet<_> = fs::read_dir(temporary.path())
        .expect("read tampered-pack parent before publication")
        .map(|entry| entry.expect("tampered-pack parent entry").file_name())
        .collect();
    let error =
        okc_core::pack::create_pack(&artifact_root, &pack, compiler.policy().output.zstd_level)
            .expect_err("public publisher must independently reject tampered input");
    assert!(matches!(error, OkcError::VerificationFailed(_)));
    assert!(!pack.exists());
    let entries_after: BTreeSet<_> = fs::read_dir(temporary.path())
        .expect("read tampered-pack parent after publication")
        .map(|entry| entry.expect("tampered-pack parent entry").file_name())
        .collect();
    assert_eq!(entries_after, entries_before);
}

#[test]
fn verifier_rejects_noncanonical_okcpack_encoding() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect canonical-pack source");
    let plan = compiler
        .plan(&inspection)
        .expect("plan canonical-pack source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve canonical-pack plan");
    let temporary = tempfile::tempdir().expect("temporary canonical-pack parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile canonical-pack source");
    let canonical = temporary.path().join("canonical.okcpack");
    okc_core::pack::create_pack(&artifact, &canonical, compiler.policy().output.zstd_level)
        .expect("create canonical OKCPack");

    let mut tar_bytes = Vec::new();
    zstd::Decoder::new(File::open(&canonical).expect("open canonical OKCPack"))
        .expect("decode canonical OKCPack")
        .read_to_end(&mut tar_bytes)
        .expect("read canonical tar payload");
    let noncanonical = temporary.path().join("noncanonical.okcpack");
    let output = File::create(&noncanonical).expect("create noncanonical OKCPack");
    let alternate_level = if compiler.policy().output.zstd_level == 19 {
        1
    } else {
        19
    };
    let mut encoder =
        zstd::Encoder::new(output, alternate_level).expect("create alternate zstd encoder");
    encoder
        .write_all(&tar_bytes)
        .expect("recompress the exact canonical tar payload");
    encoder.finish().expect("finish alternate zstd stream");
    assert_ne!(
        fs::read(&canonical).expect("read canonical OKCPack"),
        fs::read(&noncanonical).expect("read noncanonical OKCPack"),
        "alternate zstd encoding must change only the outer bytes"
    );

    assert!(matches!(
        compiler.verify(&noncanonical),
        Err(OkcError::VerificationFailed(_))
    ));
    assert!(matches!(
        compiler.explain_provenance_page(&noncanonical, &ProvenanceQuery::package()),
        Err(OkcError::VerificationFailed(_))
    ));

    let wrong_level = temporary.path().join("wrong-level.okcpack");
    assert!(matches!(
        okc_core::pack::create_pack(&artifact, &wrong_level, alternate_level),
        Err(OkcError::InvalidConfig(_))
    ));
    assert!(
        !wrong_level.exists(),
        "profile mismatch must fail before publishing a destination"
    );
}

#[test]
fn highly_compressible_hostile_pack_is_rejected_without_residue() {
    let compiler = common::compiler();
    let temporary = tempfile::tempdir().expect("temporary hostile-pack parent");
    let hostile = temporary.path().join("hostile.okcpack");
    let output = File::create(&hostile).expect("create hostile OKCPack");
    let encoder = zstd::Encoder::new(output, 19).expect("create hostile zstd stream");
    let mut archive = tar::Builder::new(encoder);
    archive.mode(tar::HeaderMode::Deterministic);
    let payload = vec![0_u8; 4 * 1024 * 1024];
    let mut header = tar::Header::new_gnu();
    header.set_size(payload.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    archive
        .append_data(&mut header, "highly-compressible.bin", payload.as_slice())
        .expect("append highly-compressible hostile member");
    let encoder = archive.into_inner().expect("finish hostile tar stream");
    encoder.finish().expect("finish hostile zstd stream");
    let compressed_len = fs::metadata(&hostile).expect("stat hostile OKCPack").len();
    assert!(
        compressed_len.saturating_mul(100) < payload.len() as u64,
        "fixture must exceed the default archive expansion ratio"
    );

    let entries_before: BTreeSet<_> = fs::read_dir(temporary.path())
        .expect("read hostile-pack parent before verification")
        .map(|entry| entry.expect("hostile-pack parent entry").file_name())
        .collect();
    for result in [
        compiler.verify(&hostile).map(|_| ()),
        compiler
            .explain_provenance_page(&hostile, &ProvenanceQuery::package())
            .map(|_| ()),
    ] {
        assert!(
            matches!(
                &result,
                Err(OkcError::ResourceLimit(message))
                    if message.contains("expansion ratio")
            ),
            "hostile pack must fail at the expansion-ratio boundary: {result:?}"
        );
    }
    let entries_after: BTreeSet<_> = fs::read_dir(temporary.path())
        .expect("read hostile-pack parent after verification")
        .map(|entry| entry.expect("hostile-pack parent entry").file_name())
        .collect();
    assert_eq!(
        entries_after, entries_before,
        "failed verify/explain must leave no output or temporary sibling residue"
    );
}

#[test]
fn absolute_source_locations_do_not_affect_artifact_or_okcpack_bytes() {
    let temporary = tempfile::tempdir().expect("temporary cross-location parent");
    let first_source = temporary.path().join("first-source");
    let second_source = temporary.path().join("nested/second-source");
    common::copy_tree(&common::fixture("basic_vault"), &first_source);
    common::copy_tree(&common::fixture("basic_vault"), &second_source);

    let first_artifact = temporary.path().join("first-artifact");
    let second_artifact = temporary.path().join("second-artifact");
    let first_pack = temporary.path().join("first-location.okcpack");
    let second_pack = temporary.path().join("second-location.okcpack");
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
        "OKCPack bytes must not depend on source location"
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
        let audit_plan =
            fs::read_to_string(artifact.join(".okc/plan.json")).expect("read compiled audit plan");
        for locator in source_locators {
            assert!(
                !audit_plan.contains(locator),
                "compiled audit plan must redact source locator `{locator}`"
            );
        }
        assert!(audit_plan.contains("[redacted-source-locator]"));
    }
}
