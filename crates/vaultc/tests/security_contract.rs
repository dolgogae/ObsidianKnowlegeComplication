mod common;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Cursor, Write as _};
use std::path::Path;

use vaultc::approval::{ConflictDecision, ConflictDecisionLog};
use vaultc::compile::ArtifactManifest;
use vaultc::diagnostic::DiagnosticCode;
use vaultc::plan::ConflictResolution;
use vaultc::provenance::ProvenanceRecord;
use vaultc::{
    ApprovalLog, CompilerPolicy, SourceSpec, ValidatedProposals, VaultCompiler, VaultcError,
};
use zip::write::SimpleFileOptions;

fn refresh_checksum(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed artifact file");
    let hash = hex::encode(Sha256::digest(bytes));
    let checksums_path = root.join(".vaultc/checksums.txt");
    let checksums = fs::read_to_string(&checksums_path).expect("read artifact checksums");
    let mut replaced = false;
    let mut output = String::new();
    for line in checksums.lines() {
        let (_, path) = line.split_once("  ").expect("valid checksum fixture line");
        if path == logical_path {
            writeln!(&mut output, "{hash}  {logical_path}")
                .expect("write changed checksum fixture line");
            replaced = true;
        } else {
            writeln!(&mut output, "{line}").expect("write unchanged checksum fixture line");
        }
    }
    assert!(replaced, "changed artifact file is covered by checksums");
    fs::write(checksums_path, output).expect("refresh fixture checksum");
}

fn reseal_artifact_after_change(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let manifest_path = root.join(".vaultc/manifest.json");
    let mut manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read artifact manifest"))
            .expect("decode artifact manifest");
    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed audit file");
    let file = manifest
        .files
        .iter_mut()
        .find(|file| file.path == logical_path)
        .expect("changed audit file is covered by the manifest");
    file.byte_len = bytes.len() as u64;
    file.sha256 = hex::encode(Sha256::digest(bytes));
    manifest.artifact_id = vaultc::canonical::canonical_hash(
        "vaultc:artifact:v1\0",
        &(
            &manifest.plan_id,
            &manifest.files,
            &manifest.approved_proposal_hashes,
        ),
    )
    .expect("recalculate deliberately resealed artifact identity");
    fs::write(
        &manifest_path,
        vaultc::canonical::to_canonical_json_pretty(&manifest)
            .expect("encode deliberately resealed manifest"),
    )
    .expect("write deliberately resealed manifest");
    refresh_checksum(root, logical_path);
    refresh_checksum(root, ".vaultc/manifest.json");
}

fn encode_provenance(records: &[ProvenanceRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(
            vaultc::canonical::to_canonical_json(record).expect("encode provenance record"),
        );
        encoded.push(b'\n');
    }
    encoded
}

fn write_zip(path: &Path, member: &str, bytes: &[u8]) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(member, SimpleFileOptions::default())
        .expect("start ZIP member");
    archive.write_all(bytes).expect("write ZIP member");
    archive.finish().expect("finish ZIP fixture");
}

fn write_zip_entries(path: &Path, entries: &[(&str, &[u8])]) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = zip::ZipWriter::new(file);
    for (member, bytes) in entries {
        archive
            .start_file(*member, SimpleFileOptions::default())
            .expect("start ZIP member");
        archive.write_all(bytes).expect("write ZIP member");
    }
    archive.finish().expect("finish ZIP fixture");
}

fn rewrite_zip_member_name(path: &Path, from: &[u8], to: &[u8]) {
    assert_eq!(
        from.len(),
        to.len(),
        "ZIP name rewrite must preserve lengths"
    );
    let mut bytes = fs::read(path).expect("read ZIP for test-only member rewrite");
    let mut replacements = 0;
    let mut offset = 0;
    while let Some(relative) = bytes[offset..]
        .windows(from.len())
        .position(|window| window == from)
    {
        let start = offset + relative;
        bytes[start..start + from.len()].copy_from_slice(to);
        replacements += 1;
        offset = start + from.len();
    }
    assert_eq!(
        replacements, 2,
        "member name must occur once in local and once in central ZIP headers"
    );
    fs::write(path, bytes).expect("write duplicate-member ZIP fixture");
}

fn write_tar_zst_with_external_link(path: &Path) {
    let file = File::create(path).expect("create tar.zst fixture");
    let encoder = zstd::Encoder::new(file, 1).expect("create zstd encoder");
    let mut archive = tar::Builder::new(encoder);

    let safe = b"# Safe\n";
    let mut safe_header = tar::Header::new_gnu();
    safe_header.set_size(safe.len() as u64);
    safe_header.set_mode(0o644);
    safe_header.set_entry_type(tar::EntryType::Regular);
    safe_header.set_cksum();
    archive
        .append_data(&mut safe_header, "safe.md", Cursor::new(safe))
        .expect("append safe tar member");

    let mut link_header = tar::Header::new_gnu();
    link_header.set_size(0);
    link_header.set_mode(0o777);
    link_header.set_entry_type(tar::EntryType::Symlink);
    link_header
        .set_link_name("../outside-secret.md")
        .expect("set unsafe tar link target");
    link_header.set_cksum();
    archive
        .append_data(&mut link_header, "escape.md", Cursor::new(Vec::<u8>::new()))
        .expect("append tar symlink");
    let encoder = archive.into_inner().expect("finish tar stream");
    encoder.finish().expect("finish zstd stream");
}

#[test]
fn malformed_utf8_markdown_and_canvas_fail_closed() {
    for (name, bytes) in [
        ("bad.md", vec![0xff, 0xfe, 0xfd]),
        ("bad.canvas", b"{not-json".to_vec()),
    ] {
        let temporary = tempfile::tempdir().expect("temporary malformed source");
        fs::write(temporary.path().join(name), bytes).expect("write malformed fixture");
        let source = SourceSpec::directory("malformed", temporary.path())
            .expect("malformed source descriptor");
        let error = common::compiler()
            .inspect([source])
            .expect_err("malformed structured input must fail closed");
        assert!(matches!(error, VaultcError::MalformedInput { .. }));
    }
}

#[test]
fn zip_traversal_and_reserved_names_are_rejected() {
    for member in ["../escape.md", "/absolute.md", "CON.md"] {
        let temporary = tempfile::tempdir().expect("temporary unsafe ZIP source");
        let archive_path = temporary.path().join("unsafe.zip");
        write_zip(&archive_path, member, b"# must not escape\n");
        let source = SourceSpec::archive("unsafe", &archive_path).expect("ZIP source descriptor");
        let error = common::compiler()
            .inspect([source])
            .expect_err("unsafe ZIP member must fail closed");
        assert!(matches!(error, VaultcError::UnsafePath { .. }));
        assert!(!temporary.path().join("escape.md").exists());
        assert!(!temporary.path().join("absolute.md").exists());
    }
}

#[test]
fn duplicate_zip_members_are_rejected() {
    let temporary = tempfile::tempdir().expect("temporary duplicate ZIP source");
    let archive_path = temporary.path().join("duplicates.zip");
    write_zip_entries(
        &archive_path,
        &[("Topic1.md", b"first\n"), ("Topic2.md", b"second\n")],
    );
    rewrite_zip_member_name(&archive_path, b"Topic2.md", b"Topic1.md");
    let source = SourceSpec::archive("duplicates", &archive_path).expect("ZIP source descriptor");
    let error = common::compiler()
        .inspect([source])
        .expect_err("duplicate archive members must fail closed");
    assert!(matches!(error, VaultcError::UnsafePath { .. }));
}

#[test]
fn compressed_zip_expansion_ratio_is_bounded() {
    let temporary = tempfile::tempdir().expect("temporary compressed ZIP source");
    let archive_path = temporary.path().join("compressed.zip");
    let file = File::create(&archive_path).expect("create compressed ZIP fixture");
    let mut archive = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    archive
        .start_file("large.md", options)
        .expect("start compressed ZIP member");
    archive
        .write_all(&vec![b'a'; 128 * 1024])
        .expect("write compressible ZIP member");
    archive.finish().expect("finish compressed ZIP fixture");

    let mut policy = CompilerPolicy::default();
    policy.limits.max_archive_expansion_ratio = 2;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build expansion-limited compiler");
    let source =
        SourceSpec::archive("compressed", &archive_path).expect("compressed ZIP descriptor");
    let error = compiler
        .inspect([source])
        .expect_err("excessive expansion must fail closed");
    assert!(matches!(error, VaultcError::ResourceLimit(_)));
}

#[test]
fn tar_zst_external_links_are_excluded() {
    let temporary = tempfile::tempdir().expect("temporary tar.zst source");
    let archive_path = temporary.path().join("links.tar.zst");
    write_tar_zst_with_external_link(&archive_path);
    let source = SourceSpec::archive("tar-links", &archive_path).expect("tar.zst descriptor");
    let inspection = common::compiler()
        .inspect([source])
        .expect("tar link is ignored rather than followed");

    assert_eq!(inspection.snapshots[0].files.len(), 1);
    assert_eq!(inspection.snapshots[0].files[0].logical_path, "safe.md");
    assert!(inspection.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::ExcludedPath
            && diagnostic.logical_path.as_deref() == Some("escape.md")
    }));
}

#[cfg(unix)]
#[test]
fn external_symlink_is_not_followed_or_snapshotted() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary symlink source");
    let source_root = temporary.path().join("vault");
    fs::create_dir(&source_root).expect("create source root");
    fs::write(source_root.join("safe.md"), "# Safe\n").expect("write safe note");
    let outside = temporary.path().join("outside-secret.md");
    fs::write(&outside, "never snapshot this\n").expect("write outside target");
    symlink(&outside, source_root.join("escape.md")).expect("create external symlink");

    let source = SourceSpec::directory("symlink", &source_root).expect("symlink source descriptor");
    let inspection = common::compiler()
        .inspect([source])
        .expect("inspection excludes symlink");
    assert_eq!(inspection.snapshots[0].files.len(), 1);
    assert_eq!(inspection.snapshots[0].files[0].logical_path, "safe.md");
    assert!(inspection.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::ExcludedPath
            && diagnostic.logical_path.as_deref() == Some("escape.md")
    }));
}

#[cfg(all(unix, not(target_vendor = "apple")))]
#[test]
fn non_utf8_source_paths_fail_closed() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let temporary = tempfile::tempdir().expect("temporary non-UTF-8 source");
    let filename = OsString::from_vec(b"bad-\xff.md".to_vec());
    fs::write(temporary.path().join(filename), "# Invalid path\n")
        .expect("write non-UTF-8 path fixture");
    let source = SourceSpec::directory("non-utf8", temporary.path()).expect("source descriptor");
    let error = common::compiler()
        .inspect([source])
        .expect_err("non-UTF-8 source paths must fail closed");
    assert!(matches!(error, VaultcError::UnsafePath { .. }));
}

#[test]
fn resource_limits_reject_oversized_files() {
    let temporary = tempfile::tempdir().expect("temporary limited source");
    fs::write(temporary.path().join("large.md"), "12345").expect("write oversized fixture");
    let mut policy = CompilerPolicy::default();
    policy.limits.max_file_bytes = 4;
    policy.limits.max_structured_text_bytes = 4;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("limited compiler");
    let source = SourceSpec::directory("limited", temporary.path()).expect("limited source");
    let error = compiler
        .inspect([source])
        .expect_err("oversized file must be rejected");
    assert!(matches!(error, VaultcError::ResourceLimit(_)));
}

#[test]
fn configured_exclusions_are_applied_without_reading_excluded_content() {
    let temporary = tempfile::tempdir().expect("temporary excluded source");
    fs::write(temporary.path().join("keep.md"), "# Keep\n").expect("write retained note");
    fs::write(temporary.path().join("never-read.md"), vec![b'x'; 32]).expect("write excluded note");
    let mut policy = CompilerPolicy::default();
    policy.paths.exclude.push("never-read.md".into());
    policy.limits.max_file_bytes = 16;
    policy.limits.max_structured_text_bytes = 16;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build exclusion compiler");
    let source = SourceSpec::directory("excluded", temporary.path()).expect("source descriptor");
    let inspection = compiler
        .inspect([source])
        .expect("excluded oversized content is not read");

    assert_eq!(inspection.snapshots[0].files.len(), 1);
    assert_eq!(inspection.snapshots[0].files[0].logical_path, "keep.md");
}

#[test]
fn existing_destination_is_never_overwritten() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect source");
    let plan = compiler.plan(&inspection).expect("plan source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve plan");
    let temporary = tempfile::tempdir().expect("temporary destination parent");
    let destination = temporary.path().join("existing");
    fs::create_dir(&destination).expect("create existing destination");
    fs::write(destination.join("sentinel.txt"), "preserve me\n").expect("write sentinel");
    let before = common::tree_bytes(&destination);

    let error = compiler
        .compile(&approved, &destination)
        .expect_err("existing destination must be rejected");
    assert!(matches!(error, VaultcError::OutputExists(path) if path == destination));
    assert_eq!(common::tree_bytes(&destination), before);
}

#[test]
fn conflict_waivers_reject_unactionable_stale_and_duplicate_decisions() {
    let temporary = tempfile::tempdir().expect("temporary conflict source");
    let source = temporary.path().join("ambiguous-vault");
    fs::create_dir_all(source.join("one")).expect("create first topic directory");
    fs::create_dir_all(source.join("two")).expect("create second topic directory");
    fs::write(source.join("Index.md"), "Read [[Topic]].\n").expect("write ambiguous link");
    fs::write(source.join("one/Topic.md"), "# First Topic\n").expect("write first topic");
    fs::write(source.join("two/Topic.md"), "# Second Topic\n").expect("write second topic");
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("ambiguous", &source).expect("conflict source")])
        .expect("inspect conflict source");
    let plan = compiler.plan(&inspection).expect("plan conflict source");
    let conflict = plan
        .unresolved_required_conflicts()
        .next()
        .expect("required conflict")
        .clone();
    let valid = ConflictDecision {
        plan_id: plan.plan_id.to_string(),
        conflict_id: conflict.conflict_id,
        conflict_content_hash: conflict.content_hash,
        resolution: ConflictResolution::WaivedByPolicy,
        resolver: "qa-fixture".into(),
        policy_version: "test-v1".into(),
        rationale: Some("explicit V1 policy waiver".into()),
    };

    for (label, decisions) in [
        (
            "unactionable user resolution",
            vec![ConflictDecision {
                resolution: ConflictResolution::UserResolved,
                ..valid.clone()
            }],
        ),
        (
            "wrong plan",
            vec![ConflictDecision {
                plan_id: "plan_forged".into(),
                ..valid.clone()
            }],
        ),
        (
            "stale conflict hash",
            vec![ConflictDecision {
                conflict_content_hash: vaultc::identity::ContentHash::from_bytes(b"stale"),
                ..valid.clone()
            }],
        ),
        (
            "duplicate conflict decisions",
            vec![valid.clone(), valid.clone()],
        ),
    ] {
        let Err(error) = compiler.approve_with_conflicts(
            plan.clone(),
            ValidatedProposals::default(),
            ApprovalLog::default(),
            ConflictDecisionLog { decisions },
        ) else {
            panic!("{label} must be rejected");
        };
        assert!(matches!(error, VaultcError::ApprovalStale(_)), "{label}");
    }

    compiler
        .approve_with_conflicts(
            plan,
            ValidatedProposals::default(),
            ApprovalLog::default(),
            ConflictDecisionLog {
                decisions: vec![valid],
            },
        )
        .expect("exactly bound V1 policy waiver is accepted");
}

#[test]
fn verifier_detects_checksum_tampering_and_unchecksummed_files() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect source");
    let plan = compiler.plan(&inspection).expect("plan source");
    let approved = compiler
        .approve_without_augmentation(plan.clone())
        .expect("approve plan");
    let temporary = tempfile::tempdir().expect("temporary artifact parent");

    let tampered = temporary.path().join("tampered");
    compiler
        .compile(&approved, &tampered)
        .expect("compile artifact");
    fs::write(tampered.join("knowledge/Topic.md"), "tampered\n").expect("tamper artifact");
    let error = compiler
        .verify(&tampered)
        .expect_err("checksum tampering must be detected");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));

    let extra = temporary.path().join("extra");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve second plan");
    compiler
        .compile(&approved, &extra)
        .expect("compile second artifact");
    fs::write(extra.join("unchecksummed.txt"), "unexpected\n").expect("add unchecksummed file");
    let error = compiler
        .verify(&extra)
        .expect_err("unchecksummed file must be detected");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));
}

#[test]
fn verifier_detects_sealed_plan_tampering_even_with_updated_checksum() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect source");
    let plan = compiler.plan(&inspection).expect("plan source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve plan");
    let temporary = tempfile::tempdir().expect("temporary artifact parent");
    let artifact = temporary.path().join("tampered-plan");
    compiler
        .compile(&approved, &artifact)
        .expect("compile artifact");

    let plan_path = artifact.join(".vaultc/plan.json");
    let mut plan: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan_path).expect("read sealed plan"))
            .expect("decode sealed plan");
    plan["plan"]["operations"][0]["destination"] =
        serde_json::Value::String("knowledge/forged-destination.md".into());
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("encode forged plan"),
    )
    .expect("write forged plan");
    refresh_checksum(&artifact, ".vaultc/plan.json");

    let error = compiler
        .verify(&artifact)
        .expect_err("checksum alone cannot authorize a changed sealed plan");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));
}

#[test]
fn verifier_detects_manifest_plan_linkage_tampering_with_updated_checksum() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect source");
    let plan = compiler.plan(&inspection).expect("plan source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve plan");
    let temporary = tempfile::tempdir().expect("temporary artifact parent");
    let artifact = temporary.path().join("tampered-manifest");
    compiler
        .compile(&approved, &artifact)
        .expect("compile artifact");

    let manifest_path = artifact.join(".vaultc/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
            .expect("decode manifest");
    manifest["plan_id"] = serde_json::Value::String("plan_forged".into());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("encode forged manifest"),
    )
    .expect("write forged manifest");
    refresh_checksum(&artifact, ".vaultc/manifest.json");

    let error = compiler
        .verify(&artifact)
        .expect_err("manifest and sealed plan linkage must be independently checked");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));
}

#[test]
fn verifier_rejects_a_provenance_subset_even_when_artifact_is_resealed() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([
            common::fixture_source("alpha", "dedup_alpha"),
            common::fixture_source("beta", "dedup_beta"),
        ])
        .expect("inspect duplicate sources");
    let plan = compiler.plan(&inspection).expect("plan duplicate sources");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve duplicate plan");
    let temporary = tempfile::tempdir().expect("temporary provenance artifact parent");
    let artifact = temporary.path().join("tampered-provenance");
    compiler
        .compile(&approved, &artifact)
        .expect("compile duplicate artifact");

    let provenance_path = artifact.join(".vaultc/provenance.jsonl");
    let provenance = fs::read_to_string(&provenance_path).expect("read provenance ledger");
    let mut records: Vec<ProvenanceRecord> = provenance
        .lines()
        .map(|line| serde_json::from_str(line).expect("decode provenance record"))
        .collect();
    let record = records
        .iter_mut()
        .find(|record| {
            matches!(
                record,
                ProvenanceRecord::Output {
                    source_document_ids,
                    sources,
                    ..
                } if !source_document_ids.is_empty() && sources.len() > 1
            )
        })
        .expect("deduplicated note has multiple exact sources");
    let ProvenanceRecord::Output {
        source_snapshot_ids,
        source_document_ids,
        sources,
        ..
    } = record;
    sources.pop().expect("omit one valid exact source");
    *source_snapshot_ids = sources
        .iter()
        .map(|source| source.snapshot_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    *source_document_ids = sources
        .iter()
        .filter_map(|source| source.source_document_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    fs::write(&provenance_path, encode_provenance(&records))
        .expect("write internally consistent but incomplete provenance");
    reseal_artifact_after_change(&artifact, ".vaultc/provenance.jsonl");

    let error = compiler
        .verify(&artifact)
        .expect_err("resealing cannot authorize an incomplete derivation source set");
    assert!(
        matches!(error, VaultcError::VerificationFailed(message) if message.contains("exactly match"))
    );
}

#[test]
fn verifier_rejects_transcript_audit_mismatch_even_when_artifact_is_resealed() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect transcript source");
    let plan = compiler.plan(&inspection).expect("plan transcript source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve plan without augmentation");
    let temporary = tempfile::tempdir().expect("temporary transcript artifact parent");
    let artifact = temporary.path().join("tampered-transcript");
    compiler
        .compile(&approved, &artifact)
        .expect("compile transcript artifact");

    fs::write(
        artifact.join(".vaultc/ai-transcript.jsonl"),
        b"{\"forged\":true}\n",
    )
    .expect("forge standalone transcript audit log");
    reseal_artifact_after_change(&artifact, ".vaultc/ai-transcript.jsonl");

    let error = compiler
        .verify(&artifact)
        .expect_err("standalone transcript must exactly match the approved plan overlay");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));
}
