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
fn ambient_ignore_files_cannot_change_source_membership() {
    let temporary = tempfile::tempdir().unwrap();
    let first = temporary.path().join("first/vault");
    let second = temporary.path().join("second/vault");
    write(&first, "Note.md", "# Included by the sealed policy\n");
    write(&second, "Note.md", "# Included by the sealed policy\n");
    fs::write(second.parent().unwrap().join(".ignore"), "Note.md\n").unwrap();
    fs::write(first.join(".ignore"), "Note.md\n").unwrap();
    fs::write(second.join(".ignore"), "Note.md\n").unwrap();
    let baseline = CorpusBuilder::new()
        .build([SourceSpec::directory("source", &first).unwrap()])
        .unwrap();
    let ambient = CorpusBuilder::new()
        .build([SourceSpec::directory("source", &second).unwrap()])
        .unwrap();
    assert_eq!(baseline.corpus, ambient.corpus);
    assert_eq!(baseline.block_texts, ambient.block_texts);
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

#[cfg(unix)]
#[test]
fn source_root_and_archive_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    write(&source, "note.md", "# Source\n");
    let alias = temporary.path().join("alias");
    symlink(&source, &alias).unwrap();
    assert!(
        CorpusBuilder::new()
            .build([SourceSpec::directory("source", alias).unwrap()])
            .is_err()
    );
    for extension in ["zip", "tar.zst"] {
        let outside = temporary.path().join(format!("outside.{extension}"));
        fs::write(&outside, []).unwrap();
        let linked = temporary.path().join(format!("linked.{extension}"));
        symlink(&outside, &linked).unwrap();
        assert!(matches!(
            CorpusBuilder::new().build([SourceSpec::archive("source", linked).unwrap()]),
            Err(OkcError::UnsafePath { .. })
        ));
    }
}

#[test]
fn workspace_cannot_be_created_inside_a_source() {
    let temporary = tempfile::tempdir().expect("temporary fixture");
    let source = temporary.path().join("source");
    write(&source, "note.md", "# Source\n");
    let workspace = source.join("new-workspace/build.sqlite3");
    assert!(
        CorpusBuilder::new()
            .workspace(&workspace)
            .build([SourceSpec::directory("source", &source).expect("source")])
            .is_err()
    );
    assert!(!workspace.parent().expect("workspace parent").exists());
    assert_eq!(
        fs::read(source.join("note.md")).expect("source"),
        b"# Source\n"
    );
}

#[cfg(unix)]
#[test]
fn workspace_rejects_source_aliases_and_symlinked_databases() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary fixture");
    let source = temporary.path().join("source");
    let alias = temporary.path().join("source-alias");
    write(&source, "note.md", "# Source\n");
    symlink(&source, &alias).expect("source alias");
    assert!(
        CorpusBuilder::new()
            .workspace(alias.join("work/build.sqlite3"))
            .build([SourceSpec::directory("source", &source).expect("source")])
            .is_err()
    );
    assert!(!source.join("work").exists());

    let database = temporary.path().join("database.sqlite3");
    let database_alias = temporary.path().join("database-alias.sqlite3");
    fs::write(&database, []).expect("empty database");
    symlink(&database, &database_alias).expect("database alias");
    assert!(
        CorpusBuilder::new()
            .workspace(database_alias)
            .build([SourceSpec::directory("source", &source).expect("source")])
            .is_err()
    );
    assert_eq!(fs::read(database).expect("untouched database"), b"");
}

#[cfg(unix)]
#[test]
fn workspace_rejects_hardlinked_database_and_sidecars_before_writing() {
    use std::os::unix::fs::MetadataExt as _;

    for suffix in ["", "-wal", "-shm", "-journal"] {
        let temporary = tempfile::tempdir().expect("temporary fixture");
        let source = temporary.path().join("source");
        write(&source, "note.md", "# Source\n");
        let original = source.join("original.db");
        fs::write(&original, []).expect("source database");
        let before = fs::metadata(&original).expect("source metadata");
        let workspace = temporary.path().join("workspace.sqlite3");
        let alias = temporary.path().join(format!("workspace.sqlite3{suffix}"));
        fs::hard_link(&original, alias).expect("database hardlink");

        let result = CorpusBuilder::new()
            .workspace(&workspace)
            .build([SourceSpec::directory("source", &source).expect("source")]);
        assert!(
            matches!(result, Err(OkcError::UnsafePath { .. })),
            "hardlinked database suffix {suffix:?} was not rejected before SQLite: {result:?}"
        );
        assert_eq!(fs::read(&original).expect("unchanged source"), b"");
        let after = fs::metadata(original).expect("source metadata after");
        assert_eq!(after.mode(), before.mode());
        assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    }
}

#[test]
fn source_deserialization_cannot_bypass_identifier_or_field_validation() {
    for invalid in ["", ".", "..", "../escape", "has space"] {
        let input =
            serde_json::json!({"kind": "directory", "source_id": invalid, "path": "/source"});
        assert!(serde_json::from_value::<SourceSpec>(input).is_err());
    }
    let unknown = serde_json::json!({
        "kind": "directory", "source_id": "safe", "path": "/source", "unexpected": true
    });
    assert!(serde_json::from_value::<SourceSpec>(unknown).is_err());
}
