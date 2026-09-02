mod common;

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;

use okc_core::diagnostic::DiagnosticCode;
use okc_core::plan::{ConflictKind, OutputOperation};
use okc_core::provenance::{ProvenanceRecordKind, SourceRecord};
use okc_core::{OkcError, SourceSpec};

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

#[cfg(unix)]
#[test]
fn source_bytes_permissions_and_modified_times_are_immutable() {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;
    use std::time::SystemTime;

    fn metadata_tree(root: &Path) -> BTreeMap<String, (u32, u64, SystemTime)> {
        fn visit(
            root: &Path,
            current: &Path,
            output: &mut BTreeMap<String, (u32, u64, SystemTime)>,
        ) {
            let mut entries = fs::read_dir(current)
                .expect("read source metadata tree")
                .collect::<Result<Vec<_>, _>>()
                .expect("enumerate source metadata tree");
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path).expect("read source metadata");
                let relative = path
                    .strip_prefix(root)
                    .expect("metadata entry below source root")
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                output.insert(
                    relative,
                    (
                        metadata.permissions().mode(),
                        metadata.len(),
                        metadata.modified().expect("source modified time"),
                    ),
                );
                if metadata.is_dir() {
                    visit(root, &path, output);
                }
            }
        }

        let mut output = BTreeMap::new();
        visit(root, root, &mut output);
        output
    }

    let temporary = tempfile::tempdir().expect("temporary immutable metadata root");
    let source_root = temporary.path().join("source");
    common::copy_tree(&common::fixture("basic_vault"), &source_root);
    let bytes_before = common::tree_bytes(&source_root);
    let metadata_before = metadata_tree(&source_root);
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("metadata", &source_root).expect("source")])
        .expect("inspect source metadata");
    let approved = compiler
        .approve_without_augmentation(compiler.plan(&inspection).expect("plan source metadata"))
        .expect("approve source metadata plan");
    compiler
        .compile(&approved, temporary.path().join("compiled"))
        .expect("compile source metadata fixture");

    assert_eq!(common::tree_bytes(&source_root), bytes_before);
    assert_eq!(metadata_tree(&source_root), metadata_before);
}

#[test]
fn duplicate_whole_vault_registration_is_rejected() {
    let compiler = common::compiler();
    let error = compiler
        .inspect([
            common::fixture_source("first-owner", "basic_vault"),
            common::fixture_source("second-owner", "basic_vault"),
        ])
        .expect_err("identical accepted Vault content may not be registered twice");
    assert!(
        matches!(error, OkcError::InvalidConfig(message) if message.contains("registered more than once"))
    );
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
fn compiled_layout_excludes_raw_sources_and_preserves_v2_artifacts() {
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
                    kind: okc_core::ir::FileKind::Markdown,
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
    let note_sources: Vec<_> = note_explanation
        .records
        .iter()
        .filter_map(|record| match &record.kind {
            ProvenanceRecordKind::Source(SourceRecord::VaultFile(source)) => Some(source),
            _ => None,
        })
        .collect();
    assert_eq!(note_sources.len(), 2);
    assert_eq!(
        note_sources
            .iter()
            .map(|source| source.snapshot_id)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    assert_eq!(
        note_sources
            .iter()
            .filter_map(|source| source.document_id)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );

    let asset_explanation = compiler
        .explain_provenance(&output, &asset_output)
        .expect("explain deduplicated attachment");
    let asset_sources: Vec<_> = asset_explanation
        .records
        .iter()
        .filter_map(|record| match &record.kind {
            ProvenanceRecordKind::Source(SourceRecord::VaultFile(source)) => Some(source),
            _ => None,
        })
        .collect();
    assert_eq!(asset_sources.len(), 2);
    assert_eq!(
        asset_sources
            .iter()
            .map(|source| source.snapshot_id)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
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

    assert!(matches!(error, OkcError::IdentityMismatch(_)));
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
    match &mut approved.plan.operations[0] {
        OutputOperation::Copy { destination, .. }
        | OutputOperation::RewriteMarkdown { destination, .. }
        | OutputOperation::RewriteCanvas { destination, .. } => {
            destination.clear();
            destination.push_str("knowledge/unapproved-plan-mutation.md");
        }
    }

    let temporary = tempfile::tempdir().expect("temporary plan-integrity output");
    let output = temporary.path().join("must-not-publish");
    let error = compiler
        .compile(&approved, &output)
        .expect_err("changed operation must invalidate the sealed plan");
    assert!(matches!(error, OkcError::PlanStale(_)));
    assert!(!output.exists());
}
