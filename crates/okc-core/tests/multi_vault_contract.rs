mod common;

use std::fs;
use std::path::{Path, PathBuf};

use okc_core::ir::{CanvasReferenceResolution, LinkResolution};
use okc_core::{OkcCompiler, SourceSpec};

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create multi-Vault fixture parent");
    }
    fs::write(path, bytes).expect("write multi-Vault fixture member");
}

fn sources(roots: &[PathBuf], reverse: bool) -> Vec<SourceSpec> {
    let ids = ["alpha", "beta", "gamma", "delta", "epsilon"];
    let mut sources: Vec<_> = ids
        .into_iter()
        .zip(roots)
        .enumerate()
        .map(|(index, (source_id, root))| {
            SourceSpec::directory(source_id, root)
                .expect("multi-Vault source")
                .with_owner_display_name(format!("Owner {}", index + 1))
                .expect("owner display name")
        })
        .collect();
    if reverse {
        sources.reverse();
    }
    sources
}

fn compile_and_pack(
    compiler: &OkcCompiler,
    inspection: &okc_core::plan::Inspection,
    output: &Path,
    pack: &Path,
) {
    let approved = compiler
        .approve_without_augmentation(compiler.plan(inspection).expect("plan five Vaults"))
        .expect("approve five-Vault plan");
    compiler
        .compile(&approved, output)
        .expect("compile five Vaults");
    okc_core::pack::create_pack(output, pack, compiler.policy().output.zstd_level)
        .expect("pack five-Vault output");
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one lifecycle proves the five heterogeneous source styles remain one deterministic compilation contract"
)]
fn heterogeneous_mcp_style_vaults_are_origin_neutral_and_order_invariant() {
    let temporary = tempfile::tempdir().expect("temporary multi-Vault fixture");
    let roots: Vec<_> = (0..5)
        .map(|index| temporary.path().join(format!("vault-{index}")))
        .collect();

    write(
        &roots[0],
        "Index.md",
        "---\naliases: [Start]\ntags: [alpha]\n---\n# Alpha\n[[Local]] and [[Shared]].\n",
    );
    write(&roots[0], "Local.md", "# Local\nSource-local alpha.\n");
    write(&roots[0], "AOnly.md", "# A Only\nCross-source target.\n");
    write(&roots[0], "Shared.md", "# Shared\nExact shared note.\n");
    write(&roots[0], "assets/shared.bin", b"same-attachment\0");
    write(
        &roots[0],
        ".obsidian/plugins/fictional-mcp-alpha/cache.db",
        b"fictional-mcp-alpha-private-cache",
    );

    write(
        &roots[1],
        "Index.md",
        "---\ntags:\n  - beta\naliases:\n  - Other Start\n---\n# Beta\n[A only](AOnly.md).\n",
    );
    write(&roots[1], "Local.md", "# Local\nSource-local beta.\n");
    write(
        &roots[1],
        ".obsidian/plugins/fictional-mcp-beta/index.sqlite",
        b"fictional-mcp-beta-private-index",
    );

    write(
        &roots[2],
        "Board.canvas",
        br#"{"nodes":[{"id":"cross","type":"file","file":"AOnly.md","x":0,"y":0,"width":320,"height":180,"fixture_unknown":"retained"}],"edges":[],"fixture_root_unknown":{"retained":true}}"#,
    );
    write(
        &roots[2],
        "Rich.md",
        "# Rich\n> [!note] Callout\n\n$$x^2$$\n\n- list\n",
    );
    write(&roots[2], "assets/other.bin", b"other-attachment");

    write(
        &roots[3],
        "Paraphrase.md",
        "# Shared idea\nThis phrasing is similar in meaning but not byte-identical.\n",
    );

    write(&roots[4], "Shared.md", "# Shared\nExact shared note.\n");
    write(&roots[4], "assets/shared.bin", b"same-attachment\0");

    let compiler = common::compiler();
    let forward = compiler
        .inspect(sources(&roots, false))
        .expect("inspect five heterogeneous Vaults");
    let reverse = compiler
        .inspect(sources(&roots, true))
        .expect("inspect five Vaults in reverse");
    assert_eq!(
        serde_json::to_vec(&forward).expect("encode forward inspection"),
        serde_json::to_vec(&reverse).expect("encode reverse inspection")
    );
    let serialized = serde_json::to_string(&forward).expect("serialize inspection");
    assert!(!serialized.contains("fictional-mcp"));
    assert!(forward.snapshots.iter().all(|snapshot| {
        snapshot
            .files
            .iter()
            .all(|file| !file.logical_path.starts_with(".obsidian/"))
    }));

    let plan = compiler.plan(&forward).expect("plan five Vaults");
    let reverse_plan = compiler.plan(&reverse).expect("plan reversed five Vaults");
    assert_eq!(
        serde_json::to_vec(&plan).expect("encode forward plan"),
        serde_json::to_vec(&reverse_plan).expect("encode reverse plan")
    );
    assert!(
        plan.exact_groups
            .iter()
            .any(|group| group.members.len() == 2)
    );
    assert!(
        plan.workspace
            .assets
            .values()
            .any(|asset| asset.sources.len() == 2)
    );

    let alpha_index = plan
        .workspace
        .documents
        .values()
        .find(|document| {
            document.source_file.source_id.as_str() == "alpha"
                && document.source_file.logical_path == "Index.md"
        })
        .expect("alpha Index");
    let local_link = alpha_index
        .links
        .iter()
        .find(|link| link.raw_target == "Local")
        .expect("alpha local link");
    let LinkResolution::Resolved { document_id } = &local_link.resolution else {
        panic!("source-local exact link must resolve")
    };
    assert_eq!(
        plan.workspace.documents[document_id]
            .source_file
            .source_id
            .as_str(),
        "alpha"
    );

    let canvas = plan
        .workspace
        .canvases
        .values()
        .next()
        .expect("typed Canvas");
    let CanvasReferenceResolution::Resolved { target } = &canvas.file_references[0].resolution
    else {
        panic!("unique cross-source Canvas reference must resolve")
    };
    let okc_core::ir::CanvasReferenceTarget::Document(document_id) = target else {
        panic!("Canvas fixture target must be a document")
    };
    assert_eq!(
        plan.workspace.documents[document_id]
            .source_file
            .source_id
            .as_str(),
        "alpha"
    );

    let forward_output = temporary.path().join("compiled-forward");
    let reverse_output = temporary.path().join("compiled-reverse");
    let forward_pack = temporary.path().join("forward.okcpack");
    let reverse_pack = temporary.path().join("reverse.okcpack");
    compile_and_pack(&compiler, &forward, &forward_output, &forward_pack);
    compile_and_pack(&compiler, &reverse, &reverse_output, &reverse_pack);
    assert_eq!(
        common::tree_bytes(&forward_output),
        common::tree_bytes(&reverse_output)
    );
    assert_eq!(
        fs::read(forward_pack).expect("read forward five-Vault pack"),
        fs::read(reverse_pack).expect("read reverse five-Vault pack")
    );
}
