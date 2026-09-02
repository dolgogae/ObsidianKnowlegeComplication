#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use okc_core::{DocumentSelection, DraftPlan, RemoteProviderConsent, ValidatedProposals};
use okc_core::{OkcCompiler, SourceSpec};
use okc_protocol::{
    AugmentationResponse, DataBoundary, KnowledgeProposal, PROTOCOL_VERSION, ProviderCapabilities,
    ProviderOperation,
};

pub fn compiler() -> OkcCompiler {
    OkcCompiler::builder()
        .build()
        .expect("build fixture compiler")
}

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

pub fn fixture_source(source_id: &str, name: &str) -> SourceSpec {
    SourceSpec::directory(source_id, fixture(name)).expect("valid fixture source")
}

pub fn tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, current: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries = fs::read_dir(current)
            .unwrap_or_else(|error| panic!("read {}: {error}", current.display()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| panic!("enumerate {}: {error}", current.display()));
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .unwrap_or_else(|error| panic!("type {}: {error}", path.display()));
            if file_type.is_dir() {
                visit(root, &path, output);
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("entry below tree root")
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                output.insert(
                    relative,
                    fs::read(&path)
                        .unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
                );
            }
        }
    }

    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

pub fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination)
        .unwrap_or_else(|error| panic!("create {}: {error}", destination.display()));
    for entry in
        fs::read_dir(source).unwrap_or_else(|error| panic!("read {}: {error}", source.display()))
    {
        let entry = entry.expect("fixture directory entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().expect("fixture entry type");
        if file_type.is_dir() {
            copy_tree(&source_path, &destination_path);
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
                panic!(
                    "copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            });
        }
    }
}

pub fn record_proposals(
    compiler: &OkcCompiler,
    plan: &DraftPlan,
    proposals: Vec<KnowledgeProposal>,
) -> ValidatedProposals {
    let provider = proposals
        .first()
        .expect("record at least one proposal")
        .provider
        .clone();
    assert!(
        proposals
            .iter()
            .all(|proposal| proposal.provider == provider),
        "one recorded exchange has one provider identity"
    );
    let request = compiler
        .build_augmentation_request(plan, &DocumentSelection::All)
        .expect("build recorded proposal request");
    let capabilities = ProviderCapabilities {
        provider,
        protocol_versions: vec![PROTOCOL_VERSION],
        operations: vec![ProviderOperation::KnowledgeAugmentation],
        max_input_bytes: 64 * 1024 * 1024,
        max_output_bytes: 64 * 1024 * 1024,
        structured_output: true,
        streaming: false,
        deterministic_controls: true,
        data_boundary: DataBoundary::Local,
    };
    let authorization = compiler
        .authorize_augmentation_exchange(
            plan,
            &request,
            &capabilities,
            RemoteProviderConsent::Denied,
        )
        .expect("authorize canonical proposal exchange");
    compiler
        .record_augmentation_exchange(plan, authorization, &AugmentationResponse { proposals })
        .expect("record canonical proposal exchange")
        .into_validated()
}
