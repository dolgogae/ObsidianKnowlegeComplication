mod common;

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;

use vaultc::diagnostic::DiagnosticCode;
use vaultc::plan::{ConflictKind, OutputOperation};
use vaultc::provenance::ProvenanceRecord;
use vaultc::{SourceSpec, VaultcError};

#[test]
fn immutable_source_bytes() {
    let source_root = common::fixture("basic_vault");
    let before = common::tree_bytes(&source_root);
    let compiler = common::compiler();

    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect immutable source");
    let plan = compiler.plan(&inspection).expect("plan immutable source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve deterministic plan");
    let temporary = tempfile::tempdir().expect("temporary output parent");
    compiler
        .compile(&approved, temporary.path().join("compiled"))
        .expect("compile immutable source");

    assert_eq!(common::tree_bytes(&source_root), before);
}

#[test]
fn inspect_plan_and_compile_are_deterministic() {
    let compiler = common::compiler();
    let first_inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("first inspection");
    let second_inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("second inspection");
    assert_eq!(
        serde_json::to_vec(&first_inspection).expect("serialize inspection"),
        serde_json::to_vec(&second_inspection).expect("serialize inspection")
    );

    let first_plan = compiler.plan(&first_inspection).expect("first plan");
    let second_plan = compiler.plan(&second_inspection).expect("second plan");
    assert_eq!(first_plan.plan_id, second_plan.plan_id);
    assert_eq!(
        serde_json::to_vec(&first_plan).expect("serialize plan"),
        serde_json::to_vec(&second_plan).expect("serialize plan")
    );

    let approved = compiler
        .approve_without_augmentation(first_plan)
        .expect("approve deterministic plan");
    let temporary = tempfile::tempdir().expect("temporary output parent");
    let first_output = temporary.path().join("first");
    let second_output = temporary.path().join("second");
    compiler
        .compile(&approved, &first_output)
        .expect("first compile");
    compiler
        .compile(&approved, &second_output)
        .expect("second compile");

    assert_eq!(
        common::tree_bytes(&first_output),
        common::tree_bytes(&second_output)
    );
}

#[test]
fn compiled_layout_excludes_raw_sources_and_preserves_v1_artifacts() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect layout fixture");
    assert!(inspection.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::OpaqueBaseUnvalidated
            && diagnostic.logical_path.as_deref() == Some("View.base")
    }));
    let plan = compiler.plan(&inspection).expect("plan layout fixture");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve layout fixture");
    let temporary = tempfile::tempdir().expect("temporary layout output");
    let output = temporary.path().join("compiled");
    compiler
        .compile(&approved, &output)
        .expect("compile layout fixture");

    assert!(!output.join("_sources").exists());
    assert!(!output.join(".obsidian").exists());
    assert!(output.join("canvases/Board.canvas").is_file());
    assert!(output.join("views/View.base").is_file());
    let compiled_canvas: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("canvases/Board.canvas")).expect("read compiled Canvas"),
    )
    .expect("parse rewritten compiled Canvas");
    let source_canvas: serde_json::Value = serde_json::from_slice(
        &fs::read(common::fixture("basic_vault/Board.canvas")).expect("read source Canvas"),
    )
    .expect("parse source Canvas");
    assert_eq!(compiled_canvas["nodes"][0]["file"], "../knowledge/Topic.md");
    assert_eq!(
        compiled_canvas["nodes"][0]["fixture_unknown"],
        source_canvas["nodes"][0]["fixture_unknown"]
    );
    assert_eq!(
        compiled_canvas["fixture_root_unknown"],
        source_canvas["fixture_root_unknown"]
    );
    assert_eq!(
        fs::read(output.join("views/View.base")).expect("read compiled Base"),
        fs::read(common::fixture("basic_vault/View.base")).expect("read source Base")
    );
}

#[test]
fn exact_duplicates_and_attachments_unify_with_complete_provenance() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([
            common::fixture_source("alpha", "dedup_alpha"),
            common::fixture_source("beta", "dedup_beta"),
        ])
        .expect("inspect duplicate sources");
    let plan = compiler.plan(&inspection).expect("plan duplicate sources");

    assert_eq!(plan.exact_groups.len(), 1);
    assert_eq!(plan.exact_groups[0].members.len(), 2);
    let markdown_operations = plan
        .operations
        .iter()
        .filter(|operation| {
            matches!(
                operation,
                OutputOperation::Copy {
                    kind: vaultc::ir::FileKind::Markdown,
                    ..
                } | OutputOperation::RewriteMarkdown { .. }
            )
        })
        .count();
    assert_eq!(markdown_operations, 1);
    assert_eq!(plan.asset_output_paths.len(), 1);

    let note_output = plan.output_paths[&plan.exact_groups[0].canonical].clone();
    let asset_output = plan
        .asset_output_paths
        .values()
        .next()
        .expect("deduplicated asset output")
        .clone();
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve duplicate plan");
    let temporary = tempfile::tempdir().expect("temporary output parent");
    let output = temporary.path().join("compiled");
    compiler
        .compile(&approved, &output)
        .expect("compile duplicates");

    let note_explanation = compiler
        .explain_provenance(&output, &note_output)
        .expect("explain deduplicated note");
    let ProvenanceRecord::Output {
        source_snapshot_ids,
        source_document_ids,
        ..
    } = &note_explanation.records[0];
    assert_eq!(source_snapshot_ids.len(), 2);
    assert_eq!(source_document_ids.len(), 2);

    let asset_explanation = compiler
        .explain_provenance(&output, &asset_output)
        .expect("explain deduplicated attachment");
    let ProvenanceRecord::Output {
        source_snapshot_ids,
        ..
    } = &asset_explanation.records[0];
    assert_eq!(source_snapshot_ids.len(), 2);
}

#[test]
fn resolved_links_are_rewritten_without_touching_inline_code() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect link fixture");
    let plan = compiler.plan(&inspection).expect("plan link fixture");
    assert!(plan.operations.iter().any(|operation| matches!(
        operation,
        OutputOperation::RewriteMarkdown { source_path, .. } if source_path == "Index.md"
    )));
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve link plan");
    let temporary = tempfile::tempdir().expect("temporary output parent");
    let output = temporary.path().join("compiled");
    compiler
        .compile(&approved, &output)
        .expect("compile link fixture");

    let index =
        fs::read_to_string(output.join("knowledge/Index.md")).expect("read rewritten Index note");
    assert!(index.contains("[[Topic.md|the topic]]"));
    assert!(index.contains("![[../attachments/"));
    assert!(index.contains("`[[NotALink]]`"));
}

#[test]
fn portable_path_collisions_are_stable_and_typed() {
    let compiler = common::compiler();
    let forward = compiler
        .inspect([
            common::fixture_source("alpha", "collision_alpha"),
            common::fixture_source("beta", "collision_beta"),
        ])
        .expect("inspect collision sources");
    let reverse = compiler
        .inspect([
            common::fixture_source("beta", "collision_beta"),
            common::fixture_source("alpha", "collision_alpha"),
        ])
        .expect("inspect reversed collision sources");
    let forward = compiler.plan(&forward).expect("plan collisions");
    let reverse = compiler.plan(&reverse).expect("plan reversed collisions");

    assert_eq!(forward.plan_id, reverse.plan_id);
    assert_eq!(forward.output_paths, reverse.output_paths);
    let paths: BTreeSet<_> = forward.output_paths.values().collect();
    assert_eq!(paths.len(), 2);
    assert!(
        paths
            .iter()
            .any(|path| path.as_str() == "knowledge/Topic.md")
    );
    assert!(paths.iter().any(|path| path.contains('~')));
    assert!(
        forward
            .conflicts
            .iter()
            .any(|conflict| { conflict.kind == ConflictKind::PathExact && !conflict.required })
    );
}

#[test]
fn source_change_after_plan_fails_without_publishing() {
    let temporary = tempfile::tempdir().expect("temporary test root");
    let source_root = temporary.path().join("source");
    common::copy_tree(&common::fixture("basic_vault"), &source_root);
    let compiler = common::compiler();
    let source = SourceSpec::directory("mutable", &source_root).expect("temporary source");
    let inspection = compiler
        .inspect([source])
        .expect("inspect temporary source");
    let plan = compiler.plan(&inspection).expect("plan temporary source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve temporary plan");

    let mut topic = fs::OpenOptions::new()
        .append(true)
        .open(source_root.join("Topic.md"))
        .expect("open source for deliberate test mutation");
    writeln!(topic, "changed after planning").expect("mutate test source");
    let destination = temporary.path().join("must-not-publish");
    let error = compiler
        .compile(&approved, &destination)
        .expect_err("stale source must fail compilation");

    assert!(matches!(error, VaultcError::IdentityMismatch(_)));
    assert!(!destination.exists());
}

#[test]
fn sealed_plan_operations_cannot_change_before_materialization() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect plan-integrity fixture");
    let plan = compiler.plan(&inspection).expect("seal plan");
    let mut approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve deterministic plan");
    let destination = match &mut approved.plan.operations[0] {
        OutputOperation::Copy { destination, .. }
        | OutputOperation::RewriteMarkdown { destination, .. }
        | OutputOperation::RewriteCanvas { destination, .. } => destination,
    };
    *destination = "knowledge/unapproved-plan-mutation.md".into();

    let temporary = tempfile::tempdir().expect("temporary plan-integrity output");
    let output = temporary.path().join("must-not-publish");
    let error = compiler
        .compile(&approved, &output)
        .expect_err("changed operation must invalidate the sealed plan");
    assert!(matches!(error, VaultcError::PlanStale(_)));
    assert!(!output.exists());
}
