use std::fs;
use std::path::{Path, PathBuf};

use okc_core::integration::{ApprovedIntegrationPlan, compile, explain, verify};
use sha2::{Digest as _, Sha256};

const SDK_ARTIFACT_SHA256: &str =
    "452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5";

fn regular_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .expect("read artifact directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("read artifact entries");
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().expect("artifact entry type");
        if file_type.is_dir() {
            regular_files(root, &path, files);
        } else if file_type.is_file() {
            files.push(
                path.strip_prefix(root)
                    .expect("artifact-relative path")
                    .into(),
            );
        } else {
            panic!("fixture output contains a non-regular entry");
        }
    }
}

fn artifact_digest(root: &Path) -> String {
    let mut paths = Vec::new();
    regular_files(root, root, &mut paths);
    paths.sort();
    let mut inventory = Vec::new();
    for path in paths {
        let bytes = fs::read(root.join(&path)).expect("artifact file");
        inventory.extend(
            format!(
                "{:x}  ./{}\n",
                Sha256::digest(bytes),
                path.to_string_lossy().replace('\\', "/")
            )
            .as_bytes(),
        );
    }
    format!("{:x}", Sha256::digest(inventory))
}

#[test]
fn sdk_fixture_inventory_matches_language_golden() {
    let plan: ApprovedIntegrationPlan =
        serde_json::from_slice(include_bytes!("fixtures/sdk-integration-plan.json"))
            .expect("current SDK integration plan");
    let temporary = tempfile::tempdir().expect("temporary output root");
    let output = temporary.path().join("compiled");
    compile(&plan, &output).expect("compile current SDK fixture");
    verify(&output).expect("verify current SDK fixture");
    explain(&output, "knowledge/sdk/fixture.md").expect("explain current SDK fixture");
    assert_eq!(artifact_digest(&output), SDK_ARTIFACT_SHA256);
}
