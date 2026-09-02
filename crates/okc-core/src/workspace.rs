use std::path::Path;

#[cfg(feature = "sqlite")]
use rusqlite::{Connection, params};

use crate::error::Result;
use crate::plan::Inspection;

#[cfg(feature = "sqlite")]
const WORKSPACE_SCHEMA_VERSION: u32 = 2;

#[allow(clippy::too_many_lines)]
pub(crate) fn persist_inspection(path: &Path, inspection: &Inspection) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| crate::error::OkcError::io(parent, error))?;
    }
    let mut connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    migrate_workspace(&mut connection)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        DELETE FROM documents;
        DELETE FROM files;
        DELETE FROM snapshots;
        ",
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO workspace_meta(key, value) VALUES ('schema_version', '2')",
        [],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO workspace_meta(key, value) VALUES ('inspection_hash', ?1)",
        params![inspection.inspection_hash.hex()],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO workspace_meta(key, value) VALUES ('policy_hash', ?1)",
        params![inspection.policy_hash.hex()],
    )?;
    for snapshot in &inspection.snapshots {
        let file_count = i64::try_from(snapshot.files.len()).map_err(|_| {
            crate::error::OkcError::ResourceLimit(
                "snapshot file count cannot be represented by SQLite".into(),
            )
        })?;
        transaction.execute(
            "INSERT INTO snapshots(snapshot_id, source_id, source_kind, file_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                snapshot.snapshot_id.to_string(),
                snapshot.source_id.as_str(),
                snapshot.source.kind_name(),
                file_count,
            ],
        )?;
        for file in &snapshot.files {
            let byte_len = i64::try_from(file.byte_len).map_err(|_| {
                crate::error::OkcError::ResourceLimit(
                    "source file length cannot be represented by SQLite".into(),
                )
            })?;
            transaction.execute(
                "INSERT INTO files(file_id, snapshot_id, original_path, logical_path, path_encoding, kind, byte_len, raw_sha256, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    file.file_id.to_string(),
                    snapshot.snapshot_id.to_string(),
                    file.original_path,
                    file.logical_path,
                    "utf8",
                    format!("{:?}", file.kind).to_ascii_lowercase(),
                    byte_len,
                    file.raw_sha256,
                    file.content_hash.hex(),
                ],
            )?;
        }
    }
    for document in inspection.workspace.documents.values() {
        transaction.execute(
            "INSERT INTO documents(document_id, snapshot_id, file_id, body_hash, frontmatter_hash, ir_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                document.document_id.to_string(),
                document.source_file.snapshot_id.to_string(),
                document.source_file.file_id.to_string(),
                document.body_hash.hex(),
                document.frontmatter_hash.hex(),
                crate::canonical::to_canonical_json(document)?,
            ],
        )?;
    }
    transaction.execute(
        "INSERT OR REPLACE INTO inspection_runs(inspection_hash, status) VALUES (?1, 'complete')",
        params![inspection.inspection_hash.hex()],
    )?;
    transaction.commit()?;
    set_private_permissions(path)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn migrate_workspace(connection: &mut Connection) -> Result<()> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > WORKSPACE_SCHEMA_VERSION {
        return Err(crate::error::OkcError::PlanStale(format!(
            "workspace schema {version} is newer than supported schema {WORKSPACE_SCHEMA_VERSION}"
        )));
    }
    let transaction = connection.transaction()?;
    if version == 1 {
        transaction.execute(
            "ALTER TABLE files ADD COLUMN raw_sha256 TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS snapshots (
             snapshot_id TEXT PRIMARY KEY,
             source_id TEXT NOT NULL,
             source_kind TEXT NOT NULL,
             file_count INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS files (
             file_id TEXT NOT NULL,
             snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id),
             original_path TEXT NOT NULL,
             logical_path TEXT NOT NULL,
             path_encoding TEXT NOT NULL,
             kind TEXT NOT NULL,
             byte_len INTEGER NOT NULL,
             raw_sha256 TEXT NOT NULL,
             content_hash TEXT NOT NULL,
             PRIMARY KEY(snapshot_id, file_id),
             UNIQUE(snapshot_id, logical_path)
         );
         CREATE TABLE IF NOT EXISTS documents (
             document_id TEXT PRIMARY KEY,
             snapshot_id TEXT NOT NULL,
             file_id TEXT NOT NULL,
             body_hash TEXT NOT NULL,
             frontmatter_hash TEXT NOT NULL,
             ir_json BLOB NOT NULL,
             FOREIGN KEY(snapshot_id, file_id) REFERENCES files(snapshot_id, file_id)
         );
         CREATE TABLE IF NOT EXISTS inspection_runs (
             inspection_hash TEXT PRIMARY KEY,
             status TEXT NOT NULL CHECK(status IN ('in_progress', 'complete', 'invalidated'))
         );
         PRAGMA user_version = 2;",
    )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn set_private_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, permissions)
            .map_err(|error| crate::error::OkcError::io(path, error))?;
    }
    Ok(())
}
