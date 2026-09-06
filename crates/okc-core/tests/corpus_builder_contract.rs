use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use okc_core::{CorpusBuilder, OkcError, SourceSpec};
use zip::write::SimpleFileOptions;

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent");
    }
    fs::write(path, bytes).expect("write fixture member");
}

#[test]
fn current_corpus_is_order_invariant_immutable_and_workspace_backed() {
    let temporary = tempfile::tempdir().expect("temporary fixture");
    let alpha = temporary.path().join("alpha");
    let beta = temporary.path().join("beta");
    write(
        &alpha,
        "Index.md",
        "---\ntags: [one, two]\n---\n# Alpha\nA [[Topic]] link.\n",
    );
    write(&alpha, "Topic.md", "# Topic\nAlpha body.\n");
    write(
        &alpha,
        ".obsidian/plugins/example/cache.db",
        b"private-derived-cache",
    );
    write(&beta, "Other.md", "# Other\nBeta body.\n");

    let alpha_before = fs::read(alpha.join("Index.md")).expect("source before build");
    let workspace = temporary.path().join("work/build.sqlite3");
    let builder = CorpusBuilder::new().workspace(&workspace);
    let forward = builder
        .build([
            SourceSpec::directory("alpha", &alpha).expect("alpha source"),
            SourceSpec::directory("beta", &beta).expect("beta source"),
        ])
        .expect("build forward corpus");
    let reverse = builder
        .build([
            SourceSpec::directory("beta", &beta).expect("beta source"),
            SourceSpec::directory("alpha", &alpha).expect("alpha source"),
        ])
        .expect("build reverse corpus");

    assert_eq!(forward.corpus, reverse.corpus);
    assert_eq!(forward.block_texts, reverse.block_texts);
    assert_eq!(forward.source_count, 2);
    assert!(workspace.is_file());
    assert_eq!(
        fs::read(alpha.join("Index.md")).expect("source after build"),
        alpha_before
    );
    let corpus_json = serde_json::to_string(&forward.corpus).expect("serialize corpus");
    assert!(!corpus_json.contains("private-derived-cache"));
    assert!(!corpus_json.contains(".obsidian"));
}

#[test]
fn duplicate_vault_bytes_and_unsafe_archive_paths_fail_closed() {
    let temporary = tempfile::tempdir().expect("temporary fixture");
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");
    write(&first, "same.md", "# Same\n");
    write(&second, "same.md", "# Same\n");

    let duplicate = CorpusBuilder::new()
        .build([
            SourceSpec::directory("first", &first).expect("first source"),
            SourceSpec::directory("second", &second).expect("second source"),
        ])
        .expect_err("duplicate whole-Vault bytes must fail");
    assert!(matches!(duplicate, OkcError::InvalidConfig(_)));

    let archive_path = temporary.path().join("unsafe.zip");
    let mut archive = zip::ZipWriter::new(File::create(&archive_path).expect("create ZIP"));
    archive
        .start_file("../escape.md", SimpleFileOptions::default())
        .expect("start unsafe member");
    archive.write_all(b"# Escape\n").expect("write member");
    archive.finish().expect("finish ZIP");

    let unsafe_path = CorpusBuilder::new()
        .build([SourceSpec::archive("archive", &archive_path).expect("ZIP source")])
        .expect_err("traversal member must fail");
    assert!(matches!(unsafe_path, OkcError::UnsafePath { .. }));
}

#[cfg(unix)]
#[test]
fn directory_symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary fixture");
    let source = temporary.path().join("source");
    let outside = temporary.path().join("outside.md");
    write(&source, "safe.md", "# Safe\n");
    fs::write(&outside, "# Secret\nnever include this token\n").expect("outside file");
    symlink(&outside, source.join("linked.md")).expect("source symlink");

    let prepared = CorpusBuilder::new()
        .build([SourceSpec::directory("source", &source).expect("source")])
        .expect("build without following symlink");
    let corpus_json = serde_json::to_string(&prepared.corpus).expect("serialize corpus");
    assert!(!corpus_json.contains("never include this token"));
    assert!(!corpus_json.contains("linked.md"));
}
