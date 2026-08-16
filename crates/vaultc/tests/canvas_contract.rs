mod common;

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde_json::Value;
use vaultc::approval::{ConflictDecision, ConflictDecisionLog};
use vaultc::compile::ArtifactManifest;
use vaultc::diagnostic::DiagnosticCode;
use vaultc::ir::{Canvas, CanvasFileReference, CanvasReferenceResolution, CanvasReferenceTarget};
use vaultc::plan::{ConflictKind, ConflictResolution, ConflictSubject, DraftPlan};
use vaultc::{ApprovalLog, SourceSpec, ValidatedProposals, VaultcError};

fn operation_destination(plan: &DraftPlan, source_suffix: &str) -> String {
    plan.operations
        .iter()
        .find(|operation| operation.destination().ends_with(source_suffix))
        .unwrap_or_else(|| panic!("output operation ending in {source_suffix}"))
        .destination()
        .to_owned()
}

fn relative_output_target(from: &str, to: &str) -> String {
    let from_parent: Vec<_> = Path::new(from)
        .parent()
        .and_then(Path::to_str)
        .unwrap_or_default()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to_parts: Vec<_> = to.split('/').filter(|part| !part.is_empty()).collect();
    let common = from_parent
        .iter()
        .zip(&to_parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = vec![".."; from_parent.len().saturating_sub(common)];
    result.extend_from_slice(&to_parts[common..]);
    result.join("/")
}

fn refresh_checksum(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed Canvas file");
    let hash = hex::encode(Sha256::digest(bytes));
    let checksums_path = root.join(".vaultc/checksums.txt");
    let checksums = fs::read_to_string(&checksums_path).expect("read artifact checksums");
    let mut replaced = false;
    let mut output = String::new();
    for line in checksums.lines() {
        let (_, path) = line.split_once("  ").expect("valid checksum fixture line");
        if path == logical_path {
            writeln!(&mut output, "{hash}  {logical_path}").expect("write changed Canvas checksum");
            replaced = true;
        } else {
            writeln!(&mut output, "{line}").expect("write unchanged checksum fixture line");
        }
    }
    assert!(replaced, "changed Canvas file is covered by checksums");
    fs::write(checksums_path, output).expect("refresh artifact checksums");
}

fn reseal_artifact_after_canvas_change(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let manifest_path = root.join(".vaultc/manifest.json");
    let mut manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read artifact manifest"))
            .expect("decode artifact manifest");
    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed Canvas");
    let file = manifest
        .files
        .iter_mut()
        .find(|file| file.path == logical_path)
        .expect("changed Canvas is covered by the manifest");
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

fn reseal_artifact_after_file_removal(root: &Path, logical_path: &str) {
    let manifest_path = root.join(".vaultc/manifest.json");
    let mut manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read artifact manifest"))
            .expect("decode artifact manifest");
    let original_len = manifest.files.len();
    manifest.files.retain(|file| file.path != logical_path);
    assert_eq!(
        manifest.files.len() + 1,
        original_len,
        "removed Canvas target is covered by the manifest"
    );
    manifest.artifact_id = vaultc::canonical::canonical_hash(
        "vaultc:artifact:v1\0",
        &(
            &manifest.plan_id,
            &manifest.files,
            &manifest.approved_proposal_hashes,
        ),
    )
    .expect("recalculate artifact identity without the removed Canvas target");
    fs::write(
        &manifest_path,
        vaultc::canonical::to_canonical_json_pretty(&manifest)
            .expect("encode artifact manifest without the Canvas target"),
    )
    .expect("write artifact manifest without the Canvas target");

    let checksums_path = root.join(".vaultc/checksums.txt");
    let checksums = fs::read_to_string(&checksums_path).expect("read artifact checksums");
    let mut removed = false;
    let mut output = String::new();
    for line in checksums.lines() {
        let (_, path) = line.split_once("  ").expect("valid checksum fixture line");
        if path == logical_path {
            removed = true;
        } else {
            writeln!(&mut output, "{line}").expect("retain checksum fixture line");
        }
    }
    assert!(removed, "removed Canvas target is covered by checksums");
    fs::write(checksums_path, output).expect("remove Canvas target checksum");
    refresh_checksum(root, ".vaultc/manifest.json");
}

fn canvas_node<'a>(value: &'a Value, node_id: &str) -> &'a Value {
    value["nodes"]
        .as_array()
        .expect("Canvas nodes array")
        .iter()
        .find(|node| node["id"].as_str() == Some(node_id))
        .unwrap_or_else(|| panic!("Canvas node {node_id}"))
}

fn canvas_node_file<'a>(value: &'a Value, node_id: &str) -> &'a str {
    canvas_node(value, node_id)["file"]
        .as_str()
        .unwrap_or_else(|| panic!("file path for Canvas node {node_id}"))
}

fn canvas_reference<'a>(canvas: &'a Canvas, node_id: &str) -> &'a CanvasFileReference {
    canvas
        .file_references
        .iter()
        .find(|reference| reference.node_id == node_id)
        .unwrap_or_else(|| panic!("canonical Canvas reference {node_id}"))
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one acceptance flow binds planning, compilation, reparsing, and verifier semantics"
)]
fn canvas_references_resolve_rewrite_and_reparse() {
    let compiler = common::compiler();
    let first_inspection = compiler
        .inspect([common::fixture_source("canvas", "canvas_references")])
        .expect("inspect Canvas reference fixture");
    let second_inspection = compiler
        .inspect([common::fixture_source("canvas", "canvas_references")])
        .expect("repeat Canvas reference inspection");
    let first_plan = compiler
        .plan(&first_inspection)
        .expect("plan Canvas reference fixture");
    let second_plan = compiler
        .plan(&second_inspection)
        .expect("repeat Canvas reference plan");

    assert_eq!(
        serde_json::to_vec(&first_plan).expect("serialize first Canvas plan"),
        serde_json::to_vec(&second_plan).expect("serialize repeated Canvas plan"),
        "Canvas planning must not depend on discovery or map iteration order"
    );

    let canvas_output = operation_destination(&first_plan, "boards/nested/Map.canvas");
    assert_eq!(canvas_output, "canvases/boards/nested/Map.canvas");
    let source_canvas = first_plan
        .workspace
        .canvases
        .values()
        .find(|canvas| canvas.source_file.logical_path == "boards/nested/Map.canvas")
        .expect("source Canvas canonical record");
    let target_canvas_id = first_plan
        .workspace
        .canvases
        .values()
        .find(|canvas| canvas.source_file.logical_path == "boards/Other.canvas")
        .expect("referenced Canvas canonical record")
        .canvas_id;
    let target_base_id = first_plan
        .workspace
        .bases
        .iter()
        .find(|base| base.source_file.logical_path == "views/Related.base")
        .expect("referenced Base canonical record")
        .base_artifact_id;
    let document_id = first_plan
        .workspace
        .documents
        .values()
        .find(|document| document.source_file.logical_path == "notes/Topic.md")
        .expect("Canvas target document")
        .document_id;
    let document_output = first_plan
        .output_paths
        .get(&document_id)
        .expect("Canvas target document output")
        .clone();
    let asset_id = first_plan
        .workspace
        .assets
        .iter()
        .find(|(_, asset)| asset.basename == "diagram.txt")
        .expect("Canvas target attachment")
        .0;
    let asset_output = first_plan
        .asset_output_paths
        .get(asset_id)
        .expect("Canvas target attachment output");
    let target_canvas_output = first_plan
        .canvas_output_paths
        .get(&target_canvas_id)
        .expect("referenced Canvas sealed output path")
        .clone();
    let base_output = first_plan
        .base_output_paths
        .get(&target_base_id)
        .expect("referenced Base sealed output path")
        .clone();
    assert_eq!(
        first_plan.canvas_output_paths.get(&source_canvas.canvas_id),
        Some(&canvas_output)
    );
    assert_eq!(
        operation_destination(&first_plan, "boards/Other.canvas"),
        target_canvas_output
    );
    assert_eq!(
        operation_destination(&first_plan, "views/Related.base"),
        base_output
    );
    assert_eq!(
        &canvas_reference(source_canvas, "document-node").resolution,
        &CanvasReferenceResolution::Resolved {
            target: CanvasReferenceTarget::Document(document_id),
        }
    );
    assert_eq!(
        &canvas_reference(source_canvas, "attachment-node").resolution,
        &CanvasReferenceResolution::Resolved {
            target: CanvasReferenceTarget::Asset(*asset_id),
        }
    );
    assert_eq!(
        &canvas_reference(source_canvas, "canvas-node").resolution,
        &CanvasReferenceResolution::Resolved {
            target: CanvasReferenceTarget::Canvas(target_canvas_id),
        }
    );
    assert_eq!(
        &canvas_reference(source_canvas, "base-node").resolution,
        &CanvasReferenceResolution::Resolved {
            target: CanvasReferenceTarget::Base(target_base_id),
        }
    );
    assert_eq!(
        canvas_reference(source_canvas, "missing-node").resolution,
        CanvasReferenceResolution::Unresolved
    );
    let expected_document_reference = relative_output_target(&canvas_output, &document_output);
    let expected_asset_reference = relative_output_target(&canvas_output, asset_output);
    let expected_canvas_reference = relative_output_target(&canvas_output, &target_canvas_output);
    let expected_base_reference = relative_output_target(&canvas_output, &base_output);

    assert_eq!(
        expected_document_reference,
        "../../../knowledge/notes/Topic.md"
    );
    assert!(
        expected_asset_reference.starts_with("../../../attachments/"),
        "attachment reference must be relative to the nested Canvas output: {expected_asset_reference}"
    );
    assert_eq!(expected_canvas_reference, "../Other.canvas");
    assert!(
        expected_base_reference.ends_with("views/Related.base"),
        "Base reference must use the allocated Base output: {expected_base_reference}"
    );
    assert!(first_plan.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::LinkUnresolved
            && diagnostic.logical_path.as_deref() == Some("boards/nested/Map.canvas")
            && diagnostic.message.contains("../../missing/Ghost.md")
    }));

    let approved = compiler
        .approve_without_augmentation(first_plan)
        .expect("approve Canvas rewrite plan");
    let temporary = tempfile::tempdir().expect("temporary Canvas output parent");
    let first_output = temporary.path().join("first");
    let second_output = temporary.path().join("second");
    compiler
        .compile(&approved, &first_output)
        .expect("compile first Canvas artifact");
    compiler
        .compile(&approved, &second_output)
        .expect("compile repeated Canvas artifact");
    assert_eq!(
        common::tree_bytes(&first_output),
        common::tree_bytes(&second_output),
        "Canvas serialization and the complete artifact must be deterministic"
    );
    compiler
        .verify(&first_output)
        .expect("verify untampered rewritten Canvas artifact");

    let rewritten_bytes =
        fs::read(first_output.join(&canvas_output)).expect("read rewritten Canvas");
    let rewritten: Value =
        serde_json::from_slice(&rewritten_bytes).expect("reparse rewritten Canvas JSON");
    assert_eq!(
        canvas_node_file(&rewritten, "document-node"),
        expected_document_reference
    );
    assert_eq!(
        canvas_node_file(&rewritten, "attachment-node"),
        expected_asset_reference
    );
    assert_eq!(
        canvas_node_file(&rewritten, "canvas-node"),
        expected_canvas_reference
    );
    assert_eq!(
        canvas_node_file(&rewritten, "base-node"),
        expected_base_reference
    );
    assert_eq!(
        canvas_node_file(&rewritten, "missing-node"),
        "../../missing/Ghost.md",
        "unresolved references must preserve their raw path"
    );
    assert_eq!(
        canvas_node(&rewritten, "document-node")["fixture_node_unknown"]["retained"],
        "document"
    );
    assert_eq!(
        canvas_node(&rewritten, "attachment-node")["fixture_node_unknown"]["retained"],
        "attachment"
    );
    assert_eq!(
        canvas_node(&rewritten, "canvas-node")["fixture_node_unknown"]["retained"],
        "canvas"
    );
    assert_eq!(
        canvas_node(&rewritten, "base-node")["fixture_node_unknown"]["retained"],
        "base"
    );
    assert_eq!(
        rewritten["edges"][0]["fixture_edge_unknown"],
        serde_json::json!(["kept", 7])
    );
    assert_eq!(
        rewritten["fixture_root_unknown"],
        serde_json::json!({"retained": true, "order": [3, 1, 2]})
    );

    assert_eq!(
        fs::read(first_output.join("canvases/plain/Unchanged.canvas"))
            .expect("read unchanged compiled Canvas"),
        fs::read(common::fixture("canvas_references/plain/Unchanged.canvas"))
            .expect("read unchanged source Canvas"),
        "a Canvas with no rewritten references must remain byte-identical"
    );

    let reparsed = compiler
        .inspect([SourceSpec::directory("compiled", &first_output)
            .expect("compiled artifact is a valid source")])
        .expect("inspect the compiled artifact and reparse rewritten Canvas");
    let reparsed_canvas = reparsed
        .workspace
        .canvases
        .values()
        .find(|canvas| canvas.source_file.logical_path == canvas_output)
        .expect("rewritten Canvas reparsed into canonical IR");
    assert_eq!(reparsed_canvas.value, rewritten);
    assert_eq!(reparsed_canvas.file_references.len(), 5);
    compiler
        .plan(&reparsed)
        .expect("rewritten Canvas remains plannable after reparsing");

    for mutation in ["resolved file target", "unknown field"] {
        let tampered = temporary
            .path()
            .join(format!("tampered-{}", mutation.replace(' ', "-")));
        common::copy_tree(&first_output, &tampered);
        let canvas_path = tampered.join(&canvas_output);
        let mut value: Value = serde_json::from_slice(
            &fs::read(&canvas_path).expect("read Canvas selected for semantic tampering"),
        )
        .expect("decode Canvas selected for semantic tampering");
        match mutation {
            "resolved file target" => {
                canvas_node(&value, "document-node");
                let nodes = value["nodes"].as_array_mut().expect("mutable Canvas nodes");
                let node = nodes
                    .iter_mut()
                    .find(|node| node["id"].as_str() == Some("document-node"))
                    .expect("mutable document Canvas node");
                node["file"] = Value::String("../../../knowledge/notes/Forged.md".into());
            }
            "unknown field" => {
                let nodes = value["nodes"].as_array_mut().expect("mutable Canvas nodes");
                let node = nodes
                    .iter_mut()
                    .find(|node| node["id"].as_str() == Some("document-node"))
                    .expect("mutable document Canvas node");
                node["fixture_node_unknown"]["retained"] = Value::String("forged".into());
            }
            _ => unreachable!(),
        }
        fs::write(
            &canvas_path,
            vaultc::canonical::to_canonical_json_pretty(&value)
                .expect("encode canonical-pretty tampered Canvas"),
        )
        .expect("write semantically tampered Canvas");
        reseal_artifact_after_canvas_change(&tampered, &canvas_output);
        assert!(
            matches!(
                compiler.verify(&tampered),
                Err(VaultcError::VerificationFailed(_))
            ),
            "verifier must reject a fully checksum/manifest-resealed {mutation} mutation"
        );
    }

    let missing_target = temporary.path().join("tampered-missing-resolved-target");
    common::copy_tree(&first_output, &missing_target);
    fs::remove_file(missing_target.join(&document_output))
        .expect("remove the document referenced by the rewritten Canvas");
    reseal_artifact_after_file_removal(&missing_target, &document_output);
    assert!(
        matches!(
            compiler.verify(&missing_target),
            Err(VaultcError::VerificationFailed(_))
        ),
        "verifier must reject a fully manifest/checksum-resealed artifact missing a resolved Canvas target"
    );
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one lifecycle binds node-scoped conflicts, partial waiver rejection, and mixed rewrite preservation"
)]
fn ambiguous_canvas_reference_requires_waiver_and_preserves_raw_path() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source(
            "canvas-ambiguity",
            "canvas_ambiguity",
        )])
        .expect("inspect ambiguous Canvas fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan ambiguous Canvas fixture");
    let conflicts: Vec<_> = plan
        .conflicts
        .iter()
        .filter(|conflict| conflict.kind == ConflictKind::LinkAmbiguity && conflict.required)
        .cloned()
        .collect();
    assert_eq!(conflicts.len(), 2, "each ambiguous Canvas node is required");
    assert!(
        conflicts
            .iter()
            .all(|conflict| conflict.documents.len() == 2)
    );
    assert_ne!(conflicts[0].conflict_id, conflicts[1].conflict_id);
    assert_ne!(conflicts[0].content_hash, conflicts[1].content_hash);
    let mut conflict_node_ids: Vec<_> = conflicts
        .iter()
        .map(|conflict| match &conflict.subject {
            Some(ConflictSubject::CanvasReference {
                node_id, raw_path, ..
            }) => {
                assert_eq!(raw_path, "Topic.md");
                node_id.as_str()
            }
            None => panic!("Canvas ambiguity must carry a typed subject"),
        })
        .collect();
    conflict_node_ids.sort_unstable();
    assert_eq!(conflict_node_ids, ["ambiguous-node-a", "ambiguous-node-b"]);
    let source_canvas = plan
        .workspace
        .canvases
        .values()
        .find(|canvas| canvas.source_file.logical_path == "board/Map.canvas")
        .expect("ambiguous Canvas canonical record");
    for node_id in ["ambiguous-node-a", "ambiguous-node-b"] {
        let CanvasReferenceResolution::Ambiguous { candidates } =
            &canvas_reference(source_canvas, node_id).resolution
        else {
            panic!("{node_id} must remain typed ambiguous")
        };
        assert_eq!(candidates.len(), 2);
        assert!(
            candidates
                .iter()
                .all(|candidate| matches!(candidate, CanvasReferenceTarget::Document(_)))
        );
    }
    let resolved_document_id = plan
        .workspace
        .documents
        .values()
        .find(|document| document.source_file.logical_path == "unique/Resolved.md")
        .expect("unambiguous document in mixed Canvas")
        .document_id;
    assert_eq!(
        &canvas_reference(source_canvas, "resolved-node").resolution,
        &CanvasReferenceResolution::Resolved {
            target: CanvasReferenceTarget::Document(resolved_document_id),
        }
    );
    assert_eq!(
        plan.diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == DiagnosticCode::LinkAmbiguity
                    && diagnostic.logical_path.as_deref() == Some("board/Map.canvas")
                    && diagnostic.message.contains("Topic.md")
            })
            .count(),
        2,
        "each ambiguous Canvas node needs its own diagnostic"
    );
    assert!(matches!(
        compiler.approve_without_augmentation(plan.clone()),
        Err(VaultcError::ApprovalStale(_))
    ));

    let decisions: Vec<_> = conflicts
        .iter()
        .map(|conflict| ConflictDecision {
            plan_id: plan.plan_id.to_string(),
            conflict_id: conflict.conflict_id.clone(),
            conflict_content_hash: conflict.content_hash,
            resolution: ConflictResolution::WaivedByPolicy,
            resolver: "canvas-qa".into(),
            policy_version: "test-v1".into(),
            rationale: Some("preserve one raw ambiguous Canvas path".into()),
        })
        .collect();
    assert!(matches!(
        compiler.approve_with_conflicts(
            plan.clone(),
            ValidatedProposals::default(),
            ApprovalLog::default(),
            ConflictDecisionLog {
                decisions: vec![decisions[0].clone()],
            },
        ),
        Err(VaultcError::ApprovalStale(_))
    ));
    let approved = compiler
        .approve_with_conflicts(
            plan.clone(),
            ValidatedProposals::default(),
            ApprovalLog::default(),
            ConflictDecisionLog { decisions },
        )
        .expect("explicitly waive both Canvas node ambiguities");
    let temporary = tempfile::tempdir().expect("temporary ambiguous Canvas output");
    let output = temporary.path().join("compiled");
    compiler
        .compile(&approved, &output)
        .expect("compile explicitly waived ambiguous Canvas");
    let canvas_output = operation_destination(&plan, "board/Map.canvas");
    let compiled_bytes = fs::read(output.join(&canvas_output)).expect("read ambiguous Canvas");
    let canvas_value: Value =
        serde_json::from_slice(&compiled_bytes).expect("parse ambiguous Canvas");
    assert_eq!(
        canvas_node_file(&canvas_value, "ambiguous-node-a"),
        "Topic.md"
    );
    assert_eq!(
        canvas_node_file(&canvas_value, "ambiguous-node-b"),
        "Topic.md"
    );
    assert_eq!(
        canvas_node(&canvas_value, "ambiguous-node-a")["fixture_unknown"],
        "raw-a-must-survive"
    );
    assert_eq!(
        canvas_node(&canvas_value, "ambiguous-node-b")["fixture_unknown"],
        "raw-b-must-survive"
    );
    let resolved_output = plan
        .output_paths
        .get(&resolved_document_id)
        .expect("mixed resolved document output");
    assert_eq!(
        canvas_node_file(&canvas_value, "resolved-node"),
        relative_output_target(&canvas_output, resolved_output)
    );
    assert_eq!(
        canvas_node(&canvas_value, "resolved-node")["fixture_unknown"],
        "resolved-node-must-survive"
    );
    assert_eq!(
        canvas_value["fixture_root_unknown"],
        serde_json::json!({"retained": true}),
        "rewriting another node must preserve unknown fields around waived nodes"
    );
    assert_ne!(
        compiled_bytes,
        fs::read(common::fixture("canvas_ambiguity/board/Map.canvas"))
            .expect("read source mixed-ambiguity Canvas"),
        "the resolved node forces deterministic Canvas serialization"
    );
}

#[test]
fn unsafe_canvas_reference_that_escapes_output_root_fails_closed() {
    let compiler = common::compiler();
    let temporary = tempfile::tempdir().expect("temporary unsafe Canvas parent");
    let output = temporary.path().join("must-not-exist");
    let inspection =
        match compiler.inspect([common::fixture_source("canvas-unsafe", "canvas_unsafe")]) {
            Err(VaultcError::UnsafePath { .. } | VaultcError::MalformedInput { .. }) => {
                assert!(!output.exists());
                return;
            }
            Err(error) => panic!("unsafe Canvas reference failed with the wrong error: {error}"),
            Ok(inspection) => inspection,
        };
    assert!(
        matches!(
            compiler.plan(&inspection),
            Err(VaultcError::UnsafePath { .. } | VaultcError::MalformedInput { .. })
        ),
        "a Canvas path that escapes the allocated output root must hard-fail planning"
    );
    assert!(
        !output.exists(),
        "unsafe Canvas input must publish no output"
    );
}

#[test]
fn duplicate_canvas_node_ids_fail_closed() {
    let compiler = common::compiler();
    let inspection = match compiler.inspect([common::fixture_source(
        "canvas-duplicate-node",
        "canvas_duplicate_node",
    )]) {
        Err(VaultcError::MalformedInput { .. }) => return,
        Err(error) => panic!("duplicate Canvas node IDs failed with the wrong error: {error}"),
        Ok(inspection) => inspection,
    };
    assert!(
        matches!(
            compiler.plan(&inspection),
            Err(VaultcError::MalformedInput { .. })
        ),
        "duplicate Canvas node IDs must never reach an approved output plan"
    );
}

#[test]
fn duplicate_json_object_keys_fail_closed_during_canvas_inspection() {
    let compiler = common::compiler();
    for (source_id, fixture_name) in [
        ("duplicate-root-key", "canvas_duplicate_root_key"),
        ("duplicate-nested-key", "canvas_duplicate_nested_key"),
    ] {
        assert!(
            matches!(
                compiler.inspect([common::fixture_source(source_id, fixture_name)]),
                Err(VaultcError::MalformedInput { .. })
            ),
            "duplicate JSON object keys in {fixture_name} must fail during inspection"
        );
    }
}
