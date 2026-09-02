mod common;

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Cursor, Write as _};
use std::path::Path;

use okc_core::approval::{ConflictAction, CuratorId, DecisionOverlay, DecisionOverlayLog};
use okc_core::compile::ArtifactManifest;
use okc_core::diagnostic::DiagnosticCode;
use okc_core::plan::{OutputOperation, RewriteReplacement};
use okc_core::provenance::{
    EdgeRelation, OperationRecord, ProvenanceRecord, ProvenanceRecordKind, ProvenanceSubject,
    SourceRecord,
};
use okc_core::{
    ApprovalLog, CompilerPolicy, OkcCompiler, OkcError, SourceSpec, ValidatedProposals,
};
use zip::write::SimpleFileOptions;

fn refresh_checksum(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed artifact file");
    let hash = hex::encode(Sha256::digest(bytes));
    let checksums_path = root.join(".okc/checksums.txt");
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

    let manifest_path = root.join(".okc/manifest.json");
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
    file.raw_sha256 = hex::encode(Sha256::digest(&bytes));
    file.content_hash = okc_core::identity::ContentHash::from_bytes(&bytes);
    if logical_path == ".okc/provenance.jsonl" {
        manifest.provenance_graph_hash = okc_core::provenance::stored_graph_hash(&bytes);
    }
    manifest.artifact_id = okc_core::canonical::canonical_hash(
        "okc:artifact:v2\0",
        &(
            &manifest.plan_id,
            &manifest.materialization_id,
            &manifest.files,
            &manifest.approved_proposal_hashes,
        ),
    )
    .expect("recalculate deliberately resealed artifact identity");
    fs::write(
        &manifest_path,
        okc_core::canonical::to_canonical_json_pretty(&manifest)
            .expect("encode deliberately resealed manifest"),
    )
    .expect("write deliberately resealed manifest");
    refresh_checksum(root, logical_path);
    refresh_checksum(root, ".okc/manifest.json");
}

fn encode_provenance(records: &[ProvenanceRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(
            okc_core::canonical::to_canonical_json(record).expect("encode provenance record"),
        );
        encoded.push(b'\n');
    }
    encoded
}

fn apply_markdown_replacements_for_test(
    mut source: Vec<u8>,
    replacements: &[RewriteReplacement],
) -> Vec<u8> {
    for replacement in replacements.iter().rev() {
        let start = usize::try_from(replacement.span.byte_start).expect("replacement start");
        let end = usize::try_from(replacement.span.byte_end).expect("replacement end");
        source.splice(
            start..end,
            replacement.replacement.as_bytes().iter().copied(),
        );
    }
    source
}

fn rewrite_provenance_output_hash(root: &Path, output_path: &str, output_bytes: &[u8]) {
    let provenance_path = root.join(".okc/provenance.jsonl");
    let provenance = fs::read_to_string(&provenance_path).expect("read provenance ledger");
    let mut records: Vec<ProvenanceRecord> = provenance
        .lines()
        .map(|line| serde_json::from_str(line).expect("decode provenance record"))
        .collect();
    let output_index = records
        .iter()
        .position(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Output(output)
                    if output.subject == (ProvenanceSubject::ArtifactPath {
                        path: output_path.to_owned(),
                    })
            )
        })
        .expect("rewritten Markdown has an output provenance record");
    let old_id = records[output_index].record_id;
    let ProvenanceRecordKind::Output(mut output) = records[output_index].kind.clone() else {
        unreachable!();
    };
    output.content_hash = okc_core::identity::ContentHash::from_bytes(output_bytes);
    output.byte_len = output_bytes.len() as u64;
    let replacement = ProvenanceRecord::new(ProvenanceRecordKind::Output(output))
        .expect("re-identify forged typed output");
    let new_id = replacement.record_id;
    records[output_index] = replacement;
    for record in &mut records {
        let ProvenanceRecordKind::Edge(edge) = &record.kind else {
            continue;
        };
        if edge.from != old_id && edge.to != old_id {
            continue;
        }
        let mut edge = edge.clone();
        if edge.from == old_id {
            edge.from = new_id;
        }
        if edge.to == old_id {
            edge.to = new_id;
        }
        *record = ProvenanceRecord::new(ProvenanceRecordKind::Edge(edge))
            .expect("re-identify forged typed edge");
    }
    records.sort_by(|left, right| {
        left.type_order().cmp(&right.type_order()).then_with(|| {
            left.record_id
                .hash()
                .as_bytes()
                .cmp(right.record_id.hash().as_bytes())
        })
    });
    fs::write(provenance_path, encode_provenance(&records))
        .expect("rewrite provenance to match the forged Markdown hash");
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
        assert!(matches!(error, OkcError::MalformedInput { .. }));
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
        assert!(matches!(error, OkcError::UnsafePath { .. }));
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
    assert!(matches!(error, OkcError::UnsafePath { .. }));
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
    let compiler = OkcCompiler::builder()
        .policy(policy)
        .build()
        .expect("build expansion-limited compiler");
    let source =
        SourceSpec::archive("compressed", &archive_path).expect("compressed ZIP descriptor");
    let error = compiler
        .inspect([source])
        .expect_err("excessive expansion must fail closed");
    assert!(matches!(error, OkcError::ResourceLimit(_)));
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
    assert!(matches!(error, OkcError::UnsafePath { .. }));
}

#[test]
fn resource_limits_reject_oversized_files() {
    let temporary = tempfile::tempdir().expect("temporary limited source");
    fs::write(temporary.path().join("large.md"), "12345").expect("write oversized fixture");
    let mut policy = CompilerPolicy::default();
    policy.limits.max_file_bytes = 4;
    policy.limits.max_structured_text_bytes = 4;
    let compiler = OkcCompiler::builder()
        .policy(policy)
        .build()
        .expect("limited compiler");
    let source = SourceSpec::directory("limited", temporary.path()).expect("limited source");
    let error = compiler
        .inspect([source])
        .expect_err("oversized file must be rejected");
    assert!(matches!(error, OkcError::ResourceLimit(_)));
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
    let compiler = OkcCompiler::builder()
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
    assert!(matches!(error, OkcError::OutputExists(path) if path == destination));
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
    let valid = DecisionOverlay {
        plan_id: plan.plan_id.to_string(),
        conflict_id: conflict.conflict_id,
        conflict_content_hash: conflict.content_hash,
        action: ConflictAction::WaivePreserveOriginal,
        decided_by: CuratorId::new("qa-fixture").expect("valid curator ID"),
        policy_version: "test-v2".into(),
        rationale: Some("explicit V2 policy waiver".into()),
    };

    for (label, decisions) in [
        (
            "action with a target outside the sealed candidate set",
            vec![DecisionOverlay {
                action: ConflictAction::SelectMarkdownTarget {
                    target_document_id: okc_core::identity::DocumentId::from_parts(
                        "okc:test:unsealed:v2\0",
                        &[b"unsealed"],
                    ),
                },
                ..valid.clone()
            }],
        ),
        (
            "wrong plan",
            vec![DecisionOverlay {
                plan_id: "plan_forged".into(),
                ..valid.clone()
            }],
        ),
        (
            "stale conflict hash",
            vec![DecisionOverlay {
                conflict_content_hash: okc_core::identity::ContentHash::from_bytes(b"stale"),
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
            DecisionOverlayLog { decisions },
        ) else {
            panic!("{label} must be rejected");
        };
        assert!(matches!(error, OkcError::ApprovalStale(_)), "{label}");
    }

    compiler
        .approve_with_conflicts(
            plan,
            ValidatedProposals::default(),
            ApprovalLog::default(),
            DecisionOverlayLog {
                decisions: vec![valid],
            },
        )
        .expect("exactly bound V2 policy waiver is accepted");
}

#[test]
fn sealed_markdown_target_action_derives_and_verifies_one_materialization() {
    let temporary = tempfile::tempdir().expect("temporary conflict source");
    let source = temporary.path().join("ambiguous-vault");
    fs::create_dir_all(source.join("one")).expect("create first topic directory");
    fs::create_dir_all(source.join("two")).expect("create second topic directory");
    fs::write(source.join("Index.md"), "Read ![[Topic#Part|shown]].\n")
        .expect("write ambiguous embed");
    fs::write(source.join("one/Topic.md"), "# Part\nFirst.\n").expect("write first topic");
    fs::write(source.join("two/Topic.md"), "# Part\nSecond.\n").expect("write second topic");

    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("ambiguous", &source).expect("source")])
        .expect("inspect source");
    let plan = compiler.plan(&inspection).expect("plan source");
    let conflict = plan
        .unresolved_required_conflicts()
        .next()
        .expect("required conflict")
        .clone();
    let selected = conflict.documents[0];
    let base_operations = plan.operations.clone();
    let approved = compiler
        .approve_with_conflicts(
            plan.clone(),
            ValidatedProposals::default(),
            ApprovalLog::default(),
            DecisionOverlayLog {
                decisions: vec![DecisionOverlay {
                    plan_id: plan.plan_id.to_string(),
                    conflict_id: conflict.conflict_id,
                    conflict_content_hash: conflict.content_hash,
                    action: ConflictAction::SelectMarkdownTarget {
                        target_document_id: selected,
                    },
                    decided_by: CuratorId::new("qa-curator").expect("curator"),
                    policy_version: "test-v2".into(),
                    rationale: Some("select the first sealed candidate".into()),
                }],
            },
        )
        .expect("approve selected target");

    assert_eq!(
        approved.plan.operations, base_operations,
        "DraftPlan is immutable"
    );
    assert_ne!(
        approved.materialization.effective_operations, base_operations,
        "the action must derive a distinct effective operation set"
    );
    let replacement = approved
        .materialization
        .effective_operations
        .iter()
        .find_map(|operation| match operation {
            OutputOperation::RewriteMarkdown {
                source_path,
                replacements,
                ..
            } if source_path == "Index.md" => replacements.first(),
            _ => None,
        })
        .expect("selected ambiguity becomes one Markdown replacement");
    assert!(replacement.replacement.ends_with("#Part|shown"));

    let output = temporary.path().join("compiled");
    compiler
        .compile(&approved, &output)
        .expect("compile materialization");
    compiler.verify(&output).expect("verify materialization");
    let index_output = approved
        .materialization
        .effective_operations
        .iter()
        .find(|operation| operation.destination().ends_with("Index.md"))
        .expect("Index output")
        .destination();
    let rendered = fs::read_to_string(output.join(index_output)).expect("read materialized note");
    assert!(rendered.contains(&format!("![[{}]]", replacement.replacement)));
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
    assert!(matches!(error, OkcError::VerificationFailed(_)));

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
    assert!(matches!(error, OkcError::VerificationFailed(_)));
}

#[test]
fn verifier_rejects_resealed_markdown_rewrite_output_that_disagrees_with_expected_output_hash() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source(
            "markdown-integrity",
            "markdown_rewrite_integrity",
        )])
        .expect("inspect Markdown rewrite integrity fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan Markdown rewrite integrity fixture");
    let (output_path, replacements, expected_output_hash) = plan
        .operations
        .iter()
        .find_map(|operation| match operation {
            OutputOperation::RewriteMarkdown {
                source_path,
                destination,
                replacements,
                expected_output_hash,
                ..
            } if source_path == "Index.md" => Some((
                destination.clone(),
                replacements.clone(),
                *expected_output_hash,
            )),
            _ => None,
        })
        .expect("Index.md requires a sealed Markdown rewrite");
    assert_eq!(replacements.len(), 1, "only the real wikilink is rewritten");
    let expected = apply_markdown_replacements_for_test(
        fs::read(common::fixture("markdown_rewrite_integrity/Index.md"))
            .expect("read source Markdown fixture"),
        &replacements,
    );
    assert_eq!(
        expected_output_hash,
        okc_core::identity::ContentHash::from_bytes(&expected),
        "the sealed Markdown output hash must bind the exact span-applied bytes"
    );

    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve Markdown rewrite plan");
    let temporary = tempfile::tempdir().expect("temporary Markdown artifact parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile Markdown rewrite fixture");
    compiler
        .verify(&artifact)
        .expect("verify untampered Markdown rewrite artifact");

    let actual = fs::read(artifact.join(&output_path)).expect("read rewritten Markdown output");
    assert_eq!(actual, expected, "only sealed source spans may change");
    let actual_text = std::str::from_utf8(&actual).expect("rewritten Markdown is UTF-8");
    assert!(actual_text.contains("[[Target.md|target display]]"));
    assert!(actual_text.contains("Inline code stays literal: `[[Target]]`."));
    assert!(actual_text.contains("```text\n[[Target]]\n```"));
    assert!(actual_text.contains("Sentinel before: alpha  spacing and punctuation !? [] {}."));
    assert!(actual_text.contains("Sentinel after: omega  spacing and punctuation <>/\\\\."));
    assert_eq!(
        fs::read(artifact.join("knowledge/Unchanged.md"))
            .expect("read unchanged compiled Markdown"),
        fs::read(common::fixture("markdown_rewrite_integrity/Unchanged.md"))
            .expect("read unchanged source Markdown"),
        "a Markdown copy operation must remain byte-identical"
    );

    let forged = actual_text.replace("ORIGINAL_MEANING", "FORGED_MEANING");
    assert_ne!(forged.as_bytes(), actual.as_slice());
    fs::write(artifact.join(&output_path), forged.as_bytes())
        .expect("semantically forge rewritten Markdown body");
    rewrite_provenance_output_hash(&artifact, &output_path, forged.as_bytes());
    reseal_artifact_after_change(&artifact, &output_path);
    reseal_artifact_after_change(&artifact, ".okc/provenance.jsonl");

    let error = compiler
        .verify(&artifact)
        .expect_err("a fully resealed semantic Markdown rewrite forgery must be rejected");
    assert!(
        matches!(error, OkcError::VerificationFailed(ref message) if message.contains("output hash does not match the sealed operation")),
        "unexpected verification boundary: {error}"
    );
}

#[test]
fn verifier_rejects_resealed_copy_output_that_disagrees_with_plan() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source(
            "markdown-integrity",
            "markdown_rewrite_integrity",
        )])
        .expect("inspect Markdown copy integrity fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan Markdown copy integrity fixture");
    let (output_path, expected_hash) = plan
        .operations
        .iter()
        .find_map(|operation| match operation {
            OutputOperation::Copy {
                source_path,
                destination,
                expected_hash,
                kind: okc_core::ir::FileKind::Markdown,
                ..
            } if source_path == "Unchanged.md" => Some((destination.clone(), *expected_hash)),
            _ => None,
        })
        .expect("Unchanged.md uses a sealed Markdown copy operation");
    let source = fs::read(common::fixture("markdown_rewrite_integrity/Unchanged.md"))
        .expect("read unchanged Markdown source");
    assert_eq!(
        expected_hash,
        okc_core::identity::ContentHash::from_bytes(&source)
    );

    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve Markdown copy integrity plan");
    let temporary = tempfile::tempdir().expect("temporary Markdown copy artifact parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile Markdown copy integrity fixture");
    compiler
        .verify(&artifact)
        .expect("verify untampered Markdown copy artifact");
    assert_eq!(
        fs::read(artifact.join(&output_path)).expect("read copied Markdown output"),
        source
    );

    let forged = String::from_utf8(source)
        .expect("Markdown copy fixture is UTF-8")
        .replace("These bytes", "Forged bytes");
    fs::write(artifact.join(&output_path), forged.as_bytes())
        .expect("semantically forge copied Markdown body");
    rewrite_provenance_output_hash(&artifact, &output_path, forged.as_bytes());
    reseal_artifact_after_change(&artifact, &output_path);
    reseal_artifact_after_change(&artifact, ".okc/provenance.jsonl");

    let error = compiler
        .verify(&artifact)
        .expect_err("a fully resealed Copy output forgery must be rejected");
    assert!(
        matches!(error, OkcError::VerificationFailed(ref message) if message.contains("output hash does not match the sealed operation")),
        "unexpected verification boundary: {error}"
    );
}

#[test]
fn verifier_rejects_resealed_markdown_expected_hash_and_replacement_plan_tampering() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source(
            "markdown-integrity",
            "markdown_rewrite_integrity",
        )])
        .expect("inspect Markdown plan tamper fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan Markdown plan tamper fixture");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve Markdown plan tamper fixture");
    let temporary = tempfile::tempdir().expect("temporary Markdown plan artifact parent");
    let baseline = temporary.path().join("baseline");
    compiler
        .compile(&approved, &baseline)
        .expect("compile Markdown plan tamper baseline");

    for mutation in [
        "expected output hash",
        "missing expected output hash",
        "replacement",
    ] {
        let artifact = temporary
            .path()
            .join(format!("tampered-{}", mutation.replace(' ', "-")));
        common::copy_tree(&baseline, &artifact);
        let plan_path = artifact.join(".okc/plan.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&plan_path).expect("read sealed Markdown plan"))
                .expect("decode sealed Markdown plan");
        let operation = value["plan"]["operations"]
            .as_array_mut()
            .expect("sealed plan operation array")
            .iter_mut()
            .find(|operation| {
                operation["type"].as_str() == Some("rewrite_markdown")
                    && operation["source_path"].as_str() == Some("Index.md")
            })
            .expect("sealed Index.md rewrite operation");
        match mutation {
            "expected output hash" => {
                assert!(
                    operation.get("expected_output_hash").is_some(),
                    "Markdown rewrite operations must seal an expected output hash"
                );
                operation["expected_output_hash"] = serde_json::Value::String(
                    okc_core::identity::ContentHash::from_bytes(b"forged Markdown output").hex(),
                );
            }
            "missing expected output hash" => {
                operation
                    .as_object_mut()
                    .expect("Markdown rewrite operation object")
                    .remove("expected_output_hash")
                    .expect("required Markdown expected output hash field");
            }
            "replacement" => {
                operation["replacements"][0]["replacement"] =
                    serde_json::Value::String("Forged.md".into());
            }
            _ => unreachable!(),
        }
        fs::write(
            &plan_path,
            okc_core::canonical::to_canonical_json_pretty(&value)
                .expect("encode canonical resealed Markdown plan"),
        )
        .expect("write tampered Markdown plan");
        reseal_artifact_after_change(&artifact, ".okc/plan.json");

        let error = compiler
            .verify(&artifact)
            .expect_err("fully resealed Markdown plan operation tampering must be rejected");
        assert!(
            matches!(error, OkcError::VerificationFailed(ref message) if !message.contains("checksum mismatch") && !message.contains("manifest inventory")),
            "{mutation} must reach and fail the sealed plan integrity boundary: {error}"
        );
    }
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

    let plan_path = artifact.join(".okc/plan.json");
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
    refresh_checksum(&artifact, ".okc/plan.json");

    let error = compiler
        .verify(&artifact)
        .expect_err("checksum alone cannot authorize a changed sealed plan");
    assert!(matches!(error, OkcError::VerificationFailed(_)));
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

    let manifest_path = artifact.join(".okc/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
            .expect("decode manifest");
    manifest["plan_id"] = serde_json::Value::String("plan_forged".into());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("encode forged manifest"),
    )
    .expect("write forged manifest");
    refresh_checksum(&artifact, ".okc/manifest.json");

    let error = compiler
        .verify(&artifact)
        .expect_err("manifest and sealed plan linkage must be independently checked");
    assert!(matches!(error, OkcError::VerificationFailed(_)));
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

    let provenance_path = artifact.join(".okc/provenance.jsonl");
    let provenance = fs::read_to_string(&provenance_path).expect("read provenance ledger");
    let mut records: Vec<ProvenanceRecord> = provenance
        .lines()
        .map(|line| serde_json::from_str(line).expect("decode provenance record"))
        .collect();
    let operation_id = records
        .iter()
        .find(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Operation(OperationRecord::Deduplicate {
                    member_count: 2,
                    operation: OutputOperation::Copy {
                        kind: okc_core::ir::FileKind::Markdown,
                        ..
                    } | OutputOperation::RewriteMarkdown { .. },
                })
            )
        })
        .expect("deduplicated note has a typed operation")
        .record_id;
    let omitted_source = records
        .iter()
        .find_map(|record| match &record.kind {
            ProvenanceRecordKind::Edge(edge)
                if edge.from == operation_id && edge.relation == EdgeRelation::Deduplicates =>
            {
                Some(edge.to)
            }
            _ => None,
        })
        .expect("deduplicated note has an exact source edge");
    assert!(records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Source(SourceRecord::VaultFile(source))
            if record.record_id == omitted_source && source.document_id.is_some()
    )));
    records.retain(|record| {
        record.record_id != omitted_source
            && !matches!(
                &record.kind,
                ProvenanceRecordKind::Edge(edge)
                    if edge.from == operation_id && edge.to == omitted_source
            )
    });
    fs::write(&provenance_path, encode_provenance(&records))
        .expect("write internally consistent but incomplete provenance");
    reseal_artifact_after_change(&artifact, ".okc/provenance.jsonl");

    let error = compiler
        .verify(&artifact)
        .expect_err("resealing cannot authorize an incomplete derivation source set");
    assert!(matches!(error, OkcError::VerificationFailed(_)));
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
        artifact.join(".okc/ai-transcript.jsonl"),
        b"{\"forged\":true}\n",
    )
    .expect("forge standalone transcript audit log");
    reseal_artifact_after_change(&artifact, ".okc/ai-transcript.jsonl");

    let error = compiler
        .verify(&artifact)
        .expect_err("standalone transcript must exactly match the approved plan overlay");
    assert!(matches!(error, OkcError::VerificationFailed(_)));
}
