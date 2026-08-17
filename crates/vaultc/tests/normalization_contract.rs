mod common;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Cursor, Write as _};
use std::path::Path;

use serde_json::Value;
use unicode_normalization::UnicodeNormalization as _;
use vaultc::compile::ArtifactManifest;
use vaultc::identity::{ContentHash, RecordId};
use vaultc::ir::{
    BlockKind, CanvasReferenceResolution, FileKind, LinkResolution, SourcePathEncoding,
};
use vaultc::plan::{ConflictKind, DraftPlan, OutputOperation, RewriteReplacement};
use vaultc::provenance::{ProvenanceRecord, ProvenanceRecordKind, SourceRecord};
use vaultc::{SourceSpec, VaultCompiler, VaultcError};
use zip::write::SimpleFileOptions;

const NFC_PATH: &str = "Caf\u{e9}.md";
const NFD_PATH: &str = "Cafe\u{301}.md";

fn write_single_member_zip(path: &Path, member: &str, bytes: &[u8]) {
    let file = File::create(path).expect("create Unicode path ZIP fixture");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(member, SimpleFileOptions::default())
        .expect("start Unicode path ZIP member");
    archive
        .write_all(bytes)
        .expect("write Unicode path ZIP member");
    archive.finish().expect("finish Unicode path ZIP fixture");
}

fn rewrite_zip_member_name(path: &Path, from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let mut bytes = fs::read(path).expect("read ZIP for raw-name mutation");
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
        "raw member name occurs in local and central ZIP headers"
    );
    fs::write(path, bytes).expect("write raw-name-mutated ZIP");
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn unicode_path_extra(raw_name: &[u8], unicode_name: &str) -> Box<[u8]> {
    let mut data = Vec::with_capacity(5 + unicode_name.len());
    data.push(1);
    data.extend_from_slice(&crc32(raw_name).to_le_bytes());
    data.extend_from_slice(unicode_name.as_bytes());
    data.into_boxed_slice()
}

fn write_zip_with_unicode_path_extra(path: &Path, raw_name: &str, unicode_name: &str) {
    let file = File::create(path).expect("create Unicode-extra ZIP fixture");
    let mut archive = zip::ZipWriter::new(file);
    let mut options = zip::write::FullFileOptions::default();
    options
        .add_extra_data(0x7075, unicode_path_extra(b"", unicode_name), false)
        .expect("add Info-ZIP Unicode Path extra field");
    archive
        .start_file(raw_name, options)
        .expect("start Unicode-extra ZIP member");
    archive
        .write_all(b"# Unicode extra fixture\n")
        .expect("write Unicode-extra ZIP member");
    archive.finish().expect("finish Unicode-extra ZIP fixture");
    rewrite_zip_unicode_path_crc(path, b"", raw_name.as_bytes());
}

fn rewrite_zip_unicode_path_crc(path: &Path, from_name: &[u8], to_name: &[u8]) {
    let mut bytes = fs::read(path).expect("read ZIP Unicode-extra fixture");
    let from_crc = crc32(from_name).to_le_bytes();
    let to_crc = crc32(to_name).to_le_bytes();
    let mut replacements = 0;
    let mut offset = 0;
    while offset + 9 <= bytes.len() {
        if bytes[offset..offset + 2] != [0x75, 0x70] {
            offset += 1;
            continue;
        }
        let payload_len = usize::from(u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]));
        let payload_start = offset + 4;
        let payload_end = payload_start + payload_len;
        if payload_end > bytes.len() {
            break;
        }
        if payload_len >= 5
            && bytes[payload_start] == 1
            && bytes[payload_start + 1..payload_start + 5] == from_crc
        {
            bytes[payload_start + 1..payload_start + 5].copy_from_slice(&to_crc);
            replacements += 1;
        }
        offset = payload_end;
    }
    assert!(
        replacements > 0,
        "fixture contains at least one Unicode Path extra field"
    );
    fs::write(path, bytes).expect("write Unicode Path CRC-mutated ZIP");
}

fn zip_library_visible_name(path: &Path) -> String {
    let file = File::open(path).expect("open Unicode-extra ZIP with generic library");
    let mut archive =
        zip::ZipArchive::new(file).expect("parse structurally valid Unicode-extra ZIP");
    archive
        .by_index(0)
        .expect("read Unicode-extra ZIP member")
        .name()
        .to_owned()
}

fn write_tar_octal(field: &mut [u8], value: u64) {
    let end = field.len() - 1;
    let digits = format!("{value:0end$o}");
    field[..end].copy_from_slice(digits.as_bytes());
    field[end] = 0;
}

fn write_raw_tar_zst(path: &Path, raw_name: &[u8]) {
    assert!(raw_name.len() <= 100);
    let content = b"# raw tar path\n";
    let mut header = [0_u8; 512];
    header[..raw_name.len()].copy_from_slice(raw_name);
    write_tar_octal(&mut header[100..108], 0o644);
    write_tar_octal(&mut header[108..116], 0);
    write_tar_octal(&mut header[116..124], 0);
    write_tar_octal(&mut header[124..136], content.len() as u64);
    write_tar_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    let checksum = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(checksum.as_bytes());

    let mut tar_bytes = header.to_vec();
    tar_bytes.extend_from_slice(content);
    tar_bytes.resize(tar_bytes.len().div_ceil(512) * 512, 0);
    tar_bytes.resize(tar_bytes.len() + 1024, 0);
    let compressed = zstd::stream::encode_all(std::io::Cursor::new(tar_bytes), 1)
        .expect("compress raw tar fixture");
    fs::write(path, compressed).expect("write raw tar.zst fixture");
}

fn write_tar_zst_with_only_directory_link_and_excluded_members(path: &Path) {
    let file = File::create(path).expect("create counted-nonfile tar.zst fixture");
    let encoder = zstd::Encoder::new(file, 1).expect("create counted-nonfile zstd encoder");
    let mut archive = tar::Builder::new(encoder);

    let mut directory = tar::Header::new_gnu();
    directory.set_size(0);
    directory.set_mode(0o755);
    directory.set_entry_type(tar::EntryType::Directory);
    directory.set_cksum();
    archive
        .append_data(&mut directory, "empty-directory/", Cursor::new([]))
        .expect("append counted tar directory");

    let mut link = tar::Header::new_gnu();
    link.set_size(0);
    link.set_mode(0o777);
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_link_name("target.md")
        .expect("set counted tar link target");
    link.set_cksum();
    archive
        .append_data(&mut link, "ignored-link.md", Cursor::new([]))
        .expect("append counted tar link");

    let mut excluded = tar::Header::new_gnu();
    excluded.set_size(0);
    excluded.set_mode(0o644);
    excluded.set_entry_type(tar::EntryType::Regular);
    excluded.set_cksum();
    archive
        .append_data(&mut excluded, ".obsidian/excluded.md", Cursor::new([]))
        .expect("append counted excluded tar member");

    archive.finish().expect("finish counted tar stream");
    let encoder = archive.into_inner().expect("recover counted zstd encoder");
    encoder.finish().expect("finish counted zstd stream");
}

fn write_tar_zst_with_pax_paths(path: &Path, pax_paths: &[&[u8]]) {
    let file = File::create(path).expect("create PAX tar.zst fixture");
    let encoder = zstd::Encoder::new(file, 1).expect("create PAX zstd encoder");
    let mut archive = tar::Builder::new(encoder);
    archive
        .append_pax_extensions(pax_paths.iter().map(|value| ("path", *value)))
        .expect("append PAX path extensions");
    let bytes = b"# PAX target\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    archive
        .append_data(&mut header, "safe.md", Cursor::new(bytes))
        .expect("append PAX target member");
    archive.finish().expect("finish PAX tar stream");
    let encoder = archive.into_inner().expect("recover PAX zstd encoder");
    encoder.finish().expect("finish PAX zstd stream");
}

fn assert_archive_path_rejected(compiler: &VaultCompiler, path: &Path, source_id: &str) {
    let source = SourceSpec::archive(source_id, path).expect("hostile archive source descriptor");
    let error = compiler
        .inspect([source])
        .expect_err("hostile raw archive path must fail closed");
    assert!(
        matches!(
            error,
            VaultcError::UnsafePath { .. } | VaultcError::MalformedInput { .. }
        ),
        "hostile archive path failed at wrong boundary: {error}"
    );
}

#[test]
fn archive_paths_require_raw_utf8_and_reject_backslashes_host_independently() {
    let temporary = tempfile::tempdir().expect("temporary raw archive root");
    let compiler = common::compiler();

    let invalid_zip = temporary.path().join("invalid-utf8.zip");
    write_single_member_zip(&invalid_zip, "bad-x.md", b"# invalid raw UTF-8\n");
    rewrite_zip_member_name(&invalid_zip, b"bad-x.md", b"bad-\xff.md");
    assert_archive_path_rejected(&compiler, &invalid_zip, "zip-invalid-utf8");

    for (name, member) in [
        ("zip-backslash", r"folder\Note.md"),
        (
            "zip-excluded-backslash",
            r".obsidian\plugins\must-not-hide.js",
        ),
    ] {
        let archive = temporary.path().join(format!("{name}.zip"));
        write_single_member_zip(&archive, member, b"archive path data\n");
        assert_archive_path_rejected(&compiler, &archive, name);
    }

    let invalid_tar = temporary.path().join("invalid-utf8.tar.zst");
    write_raw_tar_zst(&invalid_tar, b"bad-\xff.md");
    assert_archive_path_rejected(&compiler, &invalid_tar, "tar-invalid-utf8");
    let backslash_tar = temporary.path().join("backslash.tar.zst");
    write_raw_tar_zst(&backslash_tar, br".obsidian\plugins\must-not-hide.js");
    assert_archive_path_rejected(&compiler, &backslash_tar, "tar-backslash");
}

#[test]
fn zip_unicode_path_extra_cannot_override_or_rescue_raw_central_name() {
    let temporary = tempfile::tempdir().expect("temporary ZIP Unicode-extra root");
    let compiler = common::compiler();

    let invalid_raw = temporary.path().join("invalid-extra.zip");
    let invalid_name = b"bad-\xff.md";
    write_zip_with_unicode_path_extra(&invalid_raw, "bad-x.md", "Café.md");
    rewrite_zip_member_name(&invalid_raw, b"bad-x.md", invalid_name);
    rewrite_zip_unicode_path_crc(&invalid_raw, b"bad-x.md", invalid_name);
    assert_eq!(
        zip_library_visible_name(&invalid_raw),
        "Café.md",
        "the attack is structurally valid and demonstrates 0x7075 overriding invalid raw bytes"
    );
    assert_archive_path_rejected(&compiler, &invalid_raw, "zip-invalid-extra");

    let divergent = temporary.path().join("divergent-extra.zip");
    write_zip_with_unicode_path_extra(&divergent, "Raw.md", "Spoof.md");
    assert_eq!(
        zip_library_visible_name(&divergent),
        "Spoof.md",
        "the generic ZIP view demonstrates why the raw central field is parsed independently"
    );
    let inspection = compiler
        .inspect([SourceSpec::archive("zip-divergent-extra", &divergent)
            .expect("divergent Unicode-extra ZIP source")])
        .expect("valid raw central name remains authoritative");
    let file = &inspection.snapshots[0].files[0];
    assert_eq!(file.original_path, "Raw.md");
    assert_eq!(file.logical_path, "Raw.md");
    assert_eq!(file.path_encoding, SourcePathEncoding::Utf8);
    assert!(
        inspection
            .workspace
            .documents
            .values()
            .all(|document| document.source_file.original_path != "Spoof.md")
    );
}

#[test]
fn directory_source_seals_original_and_normalized_path_spellings_into_provenance() {
    let temporary = tempfile::tempdir().expect("temporary directory-spelling root");
    let source_root = temporary.path().join("source");
    fs::create_dir_all(&source_root).expect("create directory-spelling source");
    fs::write(source_root.join(NFD_PATH), b"# Directory spelling\n")
        .expect("write decomposed directory member");
    let source_before = common::tree_bytes(&source_root);
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("directory-spelling", &source_root)
            .expect("directory-spelling source")])
        .expect("inspect directory original spelling");
    let file = &inspection.snapshots[0].files[0];
    assert_eq!(file.original_path, NFD_PATH);
    assert_eq!(file.logical_path, NFC_PATH);
    assert_eq!(file.path_encoding, SourcePathEncoding::Utf8);
    let plan = compiler
        .plan(&inspection)
        .expect("plan directory original spelling");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve directory original spelling");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile directory original spelling");
    let records = read_stored_records(&artifact);
    let ProvenanceRecordKind::Source(SourceRecord::VaultFile(source)) =
        &first_vault_file_record(&records).kind
    else {
        unreachable!("selected Vault-file source record")
    };
    assert_eq!(source.original_source_path, NFD_PATH);
    assert_eq!(source.source_path, NFC_PATH);
    assert_eq!(source.path_encoding, SourcePathEncoding::Utf8);
    assert_eq!(common::tree_bytes(&source_root), source_before);
}

#[test]
fn zip_outer_size_is_bounded_before_archive_parse() {
    let temporary = tempfile::tempdir().expect("temporary oversized outer ZIP root");
    let archive = temporary.path().join("oversized.zip");
    fs::write(&archive, vec![0_u8; 4096]).expect("write oversized malformed ZIP");
    let mut policy = vaultc::CompilerPolicy::default();
    policy.limits.max_file_bytes = 1024;
    policy.limits.max_structured_text_bytes = 512;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build outer-size-limited compiler");
    let error = compiler
        .inspect([SourceSpec::archive("oversized-outer", &archive).expect("oversized ZIP source")])
        .expect_err("outer ZIP metadata limit must run before archive parsing");
    assert!(
        matches!(error, VaultcError::ResourceLimit(_)),
        "outer-size preflight must win over malformed ZIP parsing: {error}"
    );
}

#[test]
fn zip_declared_count_includes_directories_and_excluded_members() {
    let temporary = tempfile::tempdir().expect("temporary ZIP declared-count root");
    let archive_path = temporary.path().join("declared-count.zip");
    let file = File::create(&archive_path).expect("create declared-count ZIP");
    let mut archive = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    archive
        .add_directory("excluded-directory/", options)
        .expect("add ZIP directory entry");
    archive
        .start_file(".obsidian/plugins/excluded.js", options)
        .expect("add excluded ZIP member");
    archive
        .write_all(b"excluded\n")
        .expect("write excluded ZIP member");
    archive
        .start_file("safe.md", options)
        .expect("add safe ZIP member");
    archive
        .write_all(b"# Safe\n")
        .expect("write safe ZIP member");
    archive.finish().expect("finish declared-count ZIP");

    let mut policy = vaultc::CompilerPolicy::default();
    policy.limits.max_files = 1;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build declared-count-limited compiler");
    let error = compiler
        .inspect([SourceSpec::archive("declared-count", &archive_path)
            .expect("declared-count ZIP source")])
        .expect_err("declared central count must include excluded and directory entries");
    assert!(matches!(error, VaultcError::ResourceLimit(_)));
}

#[test]
fn tar_member_limit_counts_directories_links_and_excluded_entries() {
    let temporary = tempfile::tempdir().expect("temporary counted tar root");
    let archive_path = temporary.path().join("counted-nonfiles.tar.zst");
    write_tar_zst_with_only_directory_link_and_excluded_members(&archive_path);
    let mut policy = vaultc::CompilerPolicy::default();
    policy.limits.max_files = 2;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build tar member-count-limited compiler");
    let error = compiler
        .inspect([SourceSpec::archive("tar-counted-nonfiles", &archive_path)
            .expect("counted-nonfiles tar source")])
        .expect_err("directory, link, and excluded tar members all consume the member limit");
    assert!(matches!(error, VaultcError::ResourceLimit(_)));
}

#[test]
fn tar_full_decompressed_stream_ratio_counts_non_file_padding() {
    let temporary = tempfile::tempdir().expect("temporary padded tar root");
    let archive_path = temporary.path().join("oversized-padding.tar.zst");
    let expanded = vec![0_u8; 512 * 1024];
    let compressed = zstd::stream::encode_all(Cursor::new(&expanded), 19)
        .expect("compress highly compressible tar padding");
    assert!(
        compressed.len() * 2 < expanded.len(),
        "fixture exceeds the configured full-stream expansion ratio"
    );
    fs::write(&archive_path, &compressed).expect("write padded tar.zst fixture");
    let mut policy = vaultc::CompilerPolicy::default();
    policy.limits.max_archive_expansion_ratio = 2;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build tar expansion-limited compiler");
    let error = compiler
        .inspect([
            SourceSpec::archive("tar-oversized-padding", &archive_path).expect("padded tar source")
        ])
        .expect_err("trailing non-file tar bytes must consume the full-stream ratio budget");
    assert!(
        matches!(error, VaultcError::ResourceLimit(_)),
        "padded tar failed at the wrong boundary: {error}"
    );
}

#[test]
fn duplicate_or_invalid_utf8_pax_paths_fail_closed() {
    let temporary = tempfile::tempdir().expect("temporary hostile PAX root");
    for (name, pax_paths) in [
        (
            "duplicate",
            vec![b"safe.md".as_slice(), b"safe.md".as_slice()],
        ),
        ("invalid-utf8", vec![b"bad-\xff.md".as_slice()]),
    ] {
        let archive_path = temporary.path().join(format!("{name}.tar.zst"));
        write_tar_zst_with_pax_paths(&archive_path, &pax_paths);
        let error = common::compiler()
            .inspect([SourceSpec::archive(format!("pax-{name}"), &archive_path)
                .expect("hostile PAX source")])
            .expect_err("duplicate and invalid UTF-8 PAX paths must fail closed");
        assert!(
            matches!(error, VaultcError::MalformedInput { .. }),
            "PAX vector {name} failed at the wrong boundary: {error}"
        );
    }
}

fn read_stored_records(artifact: &Path) -> Vec<ProvenanceRecord> {
    let bytes =
        fs::read(artifact.join(".vaultc/provenance.jsonl")).expect("read stored provenance graph");
    assert!(bytes.ends_with(b"\n"));
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let record: ProvenanceRecord =
                serde_json::from_slice(line).expect("decode typed provenance record");
            assert_eq!(
                vaultc::canonical::to_canonical_json(&record)
                    .expect("encode canonical provenance record"),
                line
            );
            record
                .validate_identity()
                .expect("valid provenance RecordId");
            record
        })
        .collect()
}

fn first_vault_file_record(records: &[ProvenanceRecord]) -> &ProvenanceRecord {
    records
        .iter()
        .find(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Source(SourceRecord::VaultFile(_))
            )
        })
        .expect("one Vault-file source record")
}

fn encode_records(records: &[ProvenanceRecord]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend(
            vaultc::canonical::to_canonical_json(record)
                .expect("encode deliberately changed provenance record"),
        );
        bytes.push(b'\n');
    }
    bytes
}

fn sort_records(records: &mut [ProvenanceRecord]) {
    records.sort_by(|left, right| {
        left.type_order().cmp(&right.type_order()).then_with(|| {
            left.record_id
                .hash()
                .as_bytes()
                .cmp(right.record_id.hash().as_bytes())
        })
    });
}

fn refresh_checksum(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed artifact file");
    let hash = hex::encode(Sha256::digest(&bytes));
    let checksum_path = root.join(".vaultc/checksums.txt");
    let checksums = fs::read_to_string(&checksum_path).expect("read artifact checksums");
    let mut replaced = false;
    let mut output = String::new();
    for line in checksums.lines() {
        let (_, path) = line.split_once("  ").expect("valid checksum line");
        if path == logical_path {
            writeln!(&mut output, "{hash}  {logical_path}")
                .expect("write refreshed artifact checksum");
            replaced = true;
        } else {
            writeln!(&mut output, "{line}").expect("write unchanged artifact checksum");
        }
    }
    assert!(replaced, "`{logical_path}` is covered by checksums");
    fs::write(checksum_path, output).expect("write refreshed artifact checksums");
}

fn reseal_records(root: &Path, records: &mut [ProvenanceRecord]) {
    use sha2::{Digest as _, Sha256};

    sort_records(records);
    let bytes = encode_records(records);
    fs::write(root.join(".vaultc/provenance.jsonl"), &bytes)
        .expect("write deliberately changed provenance graph");
    let manifest_path = root.join(".vaultc/manifest.json");
    let mut manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read artifact manifest"))
            .expect("decode artifact manifest");
    let file = manifest
        .files
        .iter_mut()
        .find(|file| file.path == ".vaultc/provenance.jsonl")
        .expect("manifest inventories provenance graph");
    file.byte_len = bytes.len() as u64;
    file.sha256 = hex::encode(Sha256::digest(&bytes));
    manifest.provenance_graph_hash = vaultc::provenance::stored_graph_hash(&bytes);
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
            .expect("encode deliberately resealed artifact manifest"),
    )
    .expect("write deliberately resealed artifact manifest");
    refresh_checksum(root, ".vaultc/provenance.jsonl");
    refresh_checksum(root, ".vaultc/manifest.json");
}

fn replace_record_and_edge_references(
    records: &mut [ProvenanceRecord],
    index: usize,
    kind: ProvenanceRecordKind,
) -> (RecordId, RecordId) {
    let old_id = records[index].record_id;
    let replacement = ProvenanceRecord::new(kind).expect("re-identify changed provenance source");
    let new_id = replacement.record_id;
    records[index] = replacement;
    for record in records {
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
            .expect("re-identify changed provenance edge");
    }
    (old_id, new_id)
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one end-to-end acceptance binds ordering, collision typing, allocation, provenance, and source immutability"
)]
fn cross_source_nfc_nfd_collision_is_typed_and_preserves_original_path() {
    let temporary = tempfile::tempdir().expect("temporary Unicode collision root");
    let composed_archive = temporary.path().join("nfc.zip");
    let decomposed_archive = temporary.path().join("nfd.zip");
    write_single_member_zip(&composed_archive, NFC_PATH, b"# Composed source\n\nalpha\n");
    write_single_member_zip(
        &decomposed_archive,
        NFD_PATH,
        b"# Decomposed source\n\nbeta\n",
    );
    let archive_bytes_before = [
        fs::read(&composed_archive).expect("read NFC archive fixture"),
        fs::read(&decomposed_archive).expect("read NFD archive fixture"),
    ];
    let compiler = common::compiler();
    let sources = || {
        [
            SourceSpec::archive("composed", &composed_archive).expect("NFC archive source"),
            SourceSpec::archive("decomposed", &decomposed_archive).expect("NFD archive source"),
        ]
    };

    let inspection = compiler
        .inspect(sources())
        .expect("inspect cross-source Unicode collision");
    let reversed = compiler
        .inspect(sources().into_iter().rev())
        .expect("inspect reversed cross-source Unicode collision");
    assert_eq!(inspection.inspection_hash, reversed.inspection_hash);
    assert!(inspection.snapshots.iter().all(|snapshot| {
        snapshot.files.len() == 1 && snapshot.files[0].logical_path == NFC_PATH
    }));

    let plan = compiler
        .plan(&inspection)
        .expect("plan cross-source Unicode collision");
    let reversed_plan = compiler
        .plan(&reversed)
        .expect("plan reversed cross-source Unicode collision");
    assert_eq!(plan.plan_id, reversed_plan.plan_id);
    assert_eq!(plan.output_paths, reversed_plan.output_paths);

    let path_conflicts: Vec<_> = plan
        .conflicts
        .iter()
        .filter(|conflict| {
            matches!(
                conflict.kind,
                ConflictKind::PathExact
                    | ConflictKind::PathCasefold
                    | ConflictKind::UnicodeNormalization
            )
        })
        .collect();
    assert_eq!(path_conflicts.len(), 1);
    assert_eq!(
        path_conflicts[0].kind,
        ConflictKind::UnicodeNormalization,
        "distinct source spellings that become equal only after NFC must not be mislabeled PATH_EXACT"
    );
    assert!(!path_conflicts[0].required);

    for (source_id, original_path) in [("composed", NFC_PATH), ("decomposed", NFD_PATH)] {
        let snapshot = plan
            .snapshots
            .iter()
            .find(|snapshot| snapshot.source_id.as_str() == source_id)
            .expect("sealed snapshot for Unicode source");
        assert_eq!(snapshot.files[0].original_path, original_path);
        assert_eq!(snapshot.files[0].logical_path, NFC_PATH);
        assert_eq!(snapshot.files[0].path_encoding, SourcePathEncoding::Utf8);
        let document = plan
            .workspace
            .documents
            .values()
            .find(|document| document.source_file.source_id.as_str() == source_id)
            .expect("canonical document for Unicode source");
        assert_eq!(document.source_file.original_path, original_path);
        assert_eq!(document.source_file.logical_path, NFC_PATH);
        assert_eq!(document.source_file.path_encoding, SourcePathEncoding::Utf8);
    }

    let destinations: BTreeSet<_> = plan.output_paths.values().cloned().collect();
    assert_eq!(destinations.len(), 2);
    assert!(destinations.iter().all(|path| path.nfc().eq(path.chars())));
    assert!(destinations.iter().any(|path| path.contains('~')));

    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve Unicode collision plan");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile Unicode collision fixture");
    compiler
        .verify(&artifact)
        .expect("verify Unicode collision artifact");

    let records = read_stored_records(&artifact);
    for (source_id, original_path) in [("composed", NFC_PATH), ("decomposed", NFD_PATH)] {
        let source = records
            .iter()
            .find_map(|record| match &record.kind {
                ProvenanceRecordKind::Source(SourceRecord::VaultFile(source))
                    if source.source_id.as_str() == source_id =>
                {
                    Some(source)
                }
                _ => None,
            })
            .expect("typed Vault-file provenance for Unicode source");
        assert_eq!(source.original_source_path, original_path);
        assert_eq!(source.source_path, NFC_PATH);
        assert_eq!(source.path_encoding, SourcePathEncoding::Utf8);
    }
    assert_eq!(
        [
            fs::read(&composed_archive).expect("reread NFC archive fixture"),
            fs::read(&decomposed_archive).expect("reread NFD archive fixture"),
        ],
        archive_bytes_before,
        "inspection and compilation must not mutate Unicode source archives"
    );
}

#[test]
fn collision_against_a_previously_allocated_suffix_is_classified_from_that_exact_spelling() {
    let temporary = tempfile::tempdir().expect("temporary allocated-suffix collision root");
    let composed_archive = temporary.path().join("allocator-nfc.zip");
    let decomposed_archive = temporary.path().join("allocator-nfd.zip");
    write_single_member_zip(&composed_archive, NFC_PATH, b"# Allocator NFC\n");
    write_single_member_zip(&decomposed_archive, NFD_PATH, b"# Allocator NFD\n");
    let compiler = common::compiler();
    let initial_sources = || {
        [
            SourceSpec::archive("allocator-a", &composed_archive).expect("allocator NFC source"),
            SourceSpec::archive("allocator-b", &decomposed_archive).expect("allocator NFD source"),
        ]
    };
    let initial = compiler
        .inspect(initial_sources())
        .and_then(|inspection| compiler.plan(&inspection))
        .expect("plan initial normalization collision");
    let allocated_suffix = initial
        .output_paths
        .values()
        .find(|path| path.contains('~'))
        .expect("initial collision receives an identity suffix")
        .strip_prefix("knowledge/")
        .expect("document output has knowledge prefix")
        .to_owned();

    let exact_archive = temporary.path().join("allocator-exact-suffix.zip");
    write_single_member_zip(
        &exact_archive,
        &allocated_suffix,
        b"# Exact allocated suffix request\n",
    );
    let inspection = compiler
        .inspect(
            initial_sources().into_iter().chain([SourceSpec::archive(
                "allocator-z",
                &exact_archive,
            )
            .expect("exact allocated-suffix source")]),
        )
        .expect("inspect exact allocated-suffix collision");
    let third = inspection
        .workspace
        .documents
        .values()
        .find(|document| document.source_file.source_id.as_str() == "allocator-z")
        .expect("third exact allocated-suffix document");
    assert_eq!(third.source_file.original_path, allocated_suffix);
    let third_id = third.document_id;
    let plan = compiler
        .plan(&inspection)
        .expect("plan exact allocated-suffix collision");
    assert_eq!(
        plan.conflicts
            .iter()
            .filter(|conflict| conflict.kind == ConflictKind::UnicodeNormalization)
            .count(),
        1,
        "the initial NFC/NFD request remains a normalization collision"
    );
    let later = plan
        .conflicts
        .iter()
        .find(|conflict| conflict.documents.contains(&third_id))
        .expect("third request has a typed collision against the allocated candidate");
    assert_eq!(
        later.kind,
        ConflictKind::PathExact,
        "classification compares the third original request with the exact allocated spelling"
    );
    assert_ne!(
        plan.output_paths[&third_id],
        format!("knowledge/{allocated_suffix}"),
        "the third request receives its own deterministic collision suffix"
    );
}

#[test]
fn canonical_path_ids_ignore_spelling_but_plan_and_provenance_bind_it() {
    let temporary = tempfile::tempdir().expect("temporary Unicode identity root");
    let composed_archive = temporary.path().join("identity-nfc.zip");
    let decomposed_archive = temporary.path().join("identity-nfd.zip");
    let bytes = b"# Same semantic file\n\nidentical bytes\n";
    write_single_member_zip(&composed_archive, NFC_PATH, bytes);
    write_single_member_zip(&decomposed_archive, NFD_PATH, bytes);
    let compiler = common::compiler();
    let composed_inspection = compiler
        .inspect([SourceSpec::archive("identity", &composed_archive).expect("NFC identity source")])
        .expect("inspect NFC identity source");
    let decomposed_inspection = compiler
        .inspect([
            SourceSpec::archive("identity", &decomposed_archive).expect("NFD identity source")
        ])
        .expect("inspect NFD identity source");
    let composed_file = &composed_inspection.snapshots[0].files[0];
    let decomposed_file = &decomposed_inspection.snapshots[0].files[0];
    assert_eq!(composed_file.original_path, NFC_PATH);
    assert_eq!(decomposed_file.original_path, NFD_PATH);
    assert_eq!(composed_file.logical_path, decomposed_file.logical_path);
    assert_eq!(composed_file.file_id, decomposed_file.file_id);
    assert_eq!(
        composed_inspection.snapshots[0].snapshot_id,
        decomposed_inspection.snapshots[0].snapshot_id,
        "SnapshotId deliberately uses the normalized path, not its observed spelling"
    );
    assert_eq!(
        composed_inspection.workspace.documents.keys().next(),
        decomposed_inspection.workspace.documents.keys().next(),
        "derived semantic identities remain stable across canonically equivalent spelling"
    );

    let composed_plan = compiler
        .plan(&composed_inspection)
        .expect("plan NFC identity source");
    let decomposed_plan = compiler
        .plan(&decomposed_inspection)
        .expect("plan NFD identity source");
    assert_ne!(
        composed_plan.plan_id, decomposed_plan.plan_id,
        "PlanId must bind the separately sealed original path spelling"
    );
    assert_eq!(composed_plan.output_paths, decomposed_plan.output_paths);

    let composed_approved = compiler
        .approve_without_augmentation(composed_plan)
        .expect("approve NFC identity plan");
    let decomposed_approved = compiler
        .approve_without_augmentation(decomposed_plan)
        .expect("approve NFD identity plan");
    let composed_artifact = temporary.path().join("nfc-compiled");
    let decomposed_artifact = temporary.path().join("nfd-compiled");
    compiler
        .compile(&composed_approved, &composed_artifact)
        .expect("compile NFC identity artifact");
    compiler
        .compile(&decomposed_approved, &decomposed_artifact)
        .expect("compile NFD identity artifact");
    let composed_records = read_stored_records(&composed_artifact);
    let decomposed_records = read_stored_records(&decomposed_artifact);
    assert_ne!(
        first_vault_file_record(&composed_records).record_id,
        first_vault_file_record(&decomposed_records).record_id,
        "typed source RecordId must commit to the original spelling"
    );
}

#[test]
fn spelling_only_source_rename_after_plan_is_stale_and_publishes_nothing() {
    let temporary = tempfile::tempdir().expect("temporary spelling-rename root");
    let archive = temporary.path().join("mutable.zip");
    let bytes = b"# Spelling-only rename\n";
    write_single_member_zip(&archive, NFD_PATH, bytes);
    let compiler = common::compiler();
    let inspection = compiler
        .inspect(
            [SourceSpec::archive("spelling-rename", &archive).expect("mutable archive source")],
        )
        .expect("inspect pre-rename source spelling");
    let file = &inspection.snapshots[0].files[0];
    let sealed_file_id = file.file_id;
    let sealed_snapshot_id = file.snapshot_id;
    let plan = compiler
        .plan(&inspection)
        .expect("plan pre-rename source spelling");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve pre-rename source spelling");

    write_single_member_zip(&archive, NFC_PATH, bytes);
    let after = compiler
        .inspect(
            [SourceSpec::archive("spelling-rename", &archive).expect("renamed archive source")],
        )
        .expect("inspect post-rename source spelling");
    assert_eq!(after.snapshots[0].files[0].file_id, sealed_file_id);
    assert_eq!(after.snapshots[0].files[0].snapshot_id, sealed_snapshot_id);
    assert_eq!(after.snapshots[0].files[0].original_path, NFC_PATH);

    let output = temporary.path().join("must-not-publish");
    let error = compiler
        .compile(&approved, &output)
        .expect_err("spelling-only source rename must invalidate materialization");
    assert!(
        matches!(
            error,
            VaultcError::IdentityMismatch(_) | VaultcError::PlanStale(_)
        ),
        "spelling-only rename failed at wrong boundary: {error}"
    );
    assert!(!output.exists());
}

#[test]
fn verifier_rejects_fully_resealed_original_path_provenance_tamper() {
    let temporary = tempfile::tempdir().expect("temporary original-path tamper root");
    let archive = temporary.path().join("source.zip");
    write_single_member_zip(&archive, NFD_PATH, b"# Original spelling\n");
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([
            SourceSpec::archive("original-spelling", &archive).expect("original-spelling source")
        ])
        .expect("inspect original-spelling source");
    let plan = compiler
        .plan(&inspection)
        .expect("plan original-spelling source");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve original-spelling plan");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile original-spelling artifact");
    compiler
        .verify(&artifact)
        .expect("verify original-spelling baseline");

    let mut records = read_stored_records(&artifact);
    let index = records
        .iter()
        .position(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Source(SourceRecord::VaultFile(source))
                    if source.original_source_path == NFD_PATH
            )
        })
        .expect("NFD Vault-file provenance source");
    let mut kind = records[index].kind.clone();
    let ProvenanceRecordKind::Source(SourceRecord::VaultFile(source)) = &mut kind else {
        unreachable!("selected Vault-file source record")
    };
    NFC_PATH.clone_into(&mut source.original_source_path);
    assert_eq!(source.source_path, NFC_PATH);
    let (old_id, new_id) = replace_record_and_edge_references(&mut records, index, kind);
    assert_ne!(old_id, new_id);
    sort_records(&mut records);
    vaultc::provenance::validate_stored_graph(&records)
        .expect("forged graph remains structurally closed before plan reconstruction");
    reseal_records(&artifact, &mut records);

    let error = compiler
        .verify(&artifact)
        .expect_err("fully resealed original-path substitution must be rejected");
    assert!(matches!(error, VaultcError::VerificationFailed(_)));
}

fn folded(value: &str) -> String {
    caseless::default_case_fold_str(&value.nfc().collect::<String>())
}

#[test]
fn pinned_unicode_tables_and_full_fold_vectors_are_stable() {
    assert_eq!(caseless::UNICODE_VERSION, (16, 0, 0));
    assert_eq!(unicode_normalization::UNICODE_VERSION, (17, 0, 0));
    assert_eq!(folded("Straße"), "strasse");
    assert_eq!(folded("Straße"), folded("STRASSE"));
    assert_eq!(folded("σ"), folded("ς"));
    assert_eq!(folded("ς"), folded("Σ"));
    assert_eq!(folded("Cafe\u{301}"), folded("CAF\u{c9}"));
}

#[test]
fn full_unicode_case_fold_collides_sharp_s_and_sigma_without_rewriting_spelling() {
    let temporary = tempfile::tempdir().expect("temporary full-fold source root");
    let vectors = [
        ("sharp-lower", "Straße.md", b"# Sharp lower\n".as_slice()),
        ("sharp-upper", "STRASSE.md", b"# Sharp upper\n".as_slice()),
        ("sigma-upper", "ΟΣ.md", b"# Sigma upper\n".as_slice()),
        ("sigma-final", "ος.md", b"# Sigma final\n".as_slice()),
        ("lookup", "Index.md", "[[STRASSE]]\n[[οσ]]\n".as_bytes()),
    ];
    let mut sources = Vec::new();
    for (source_id, member, bytes) in vectors {
        let archive = temporary.path().join(format!("{source_id}.zip"));
        write_single_member_zip(&archive, member, bytes);
        sources.push(SourceSpec::archive(source_id, archive).expect("full-fold source"));
    }
    let compiler = common::compiler();
    let inspection = compiler
        .inspect(sources)
        .expect("inspect full-fold collision vectors");
    for (source_id, original_path) in [
        ("sharp-lower", "Straße.md"),
        ("sharp-upper", "STRASSE.md"),
        ("sigma-upper", "ΟΣ.md"),
        ("sigma-final", "ος.md"),
    ] {
        let file = inspection
            .snapshots
            .iter()
            .find(|snapshot| snapshot.source_id.as_str() == source_id)
            .and_then(|snapshot| snapshot.files.first())
            .expect("full-fold source file");
        assert_eq!(file.original_path, original_path);
        assert_eq!(file.logical_path, original_path.nfc().collect::<String>());
    }
    let plan = compiler
        .plan(&inspection)
        .expect("plan full-fold collision vectors");
    let path_conflicts: Vec<_> = plan
        .conflicts
        .iter()
        .filter(|conflict| conflict.kind == ConflictKind::PathCasefold)
        .collect();
    assert_eq!(
        path_conflicts.len(),
        2,
        "sharp-s and sigma each produce one typed portable path collision"
    );
    assert!(path_conflicts.iter().all(|conflict| {
        conflict.documents.len() == 2
            && !conflict.required
            && conflict
                .resolution
                .eq(&vaultc::plan::ConflictResolution::AutoResolvedByNormativeRule)
    }));
    assert_eq!(
        plan.output_paths.values().collect::<BTreeSet<_>>().len(),
        plan.output_paths.len()
    );
    assert_eq!(
        plan.output_paths
            .values()
            .filter(|path| path.contains('~'))
            .count(),
        2
    );

    let lookup = plan
        .workspace
        .documents
        .values()
        .find(|document| document.source_file.source_id.as_str() == "lookup")
        .expect("full-fold lookup document");
    assert_eq!(lookup.links.len(), 2);
    assert!(lookup.links.iter().all(|link| {
        matches!(
            &link.resolution,
            LinkResolution::Ambiguous { candidates } if candidates.len() == 2
        )
    }));
    assert_eq!(
        plan.conflicts
            .iter()
            .filter(|conflict| conflict.kind == ConflictKind::LinkAmbiguity && conflict.required)
            .count(),
        2
    );
}

fn write_markdown_vectors(root: &Path) -> Vec<u8> {
    fs::create_dir_all(root.join("deep/nested")).expect("create Markdown source directory");
    fs::create_dir_all(root.join("targets")).expect("create Markdown target directory");
    fs::create_dir_all(root.join("assets")).expect("create Markdown asset directory");
    fs::write(
        root.join("targets/Long Target (one) 100%.md"),
        "---\naliases:\n  - Grow\n---\n# Long target\n",
    )
    .expect("write long Markdown target");
    fs::write(
        root.join("deep/Short.md"),
        "# \u{c9e7}\u{c740} \u{c81c}\u{baa9}\n\nshort target\n",
    )
    .expect("write short Markdown target");
    fs::write(root.join("assets/diagram (1)%.bin"), b"asset\0bytes")
        .expect("write escaped-path asset");

    let source = concat!(
        "\u{feff}---\r\n",
        "title: Rewrite vectors\r\n",
        "---\r\n",
        "# Rewrite vectors\r\n",
        "Cafe\u{301} \u{1f642} before [[Grow|\u{d45c}\u{c2dc}\u{1f642}]] after.\r\n",
        "Shrink wiki [[../../deep/Short.md#\u{c9e7}\u{c740} \u{c81c}\u{baa9}|\u{cd95}\u{c57d}]].\r",
        "Shrink Markdown [short](<../../deep/./Short.md>).\r\n",
        "Markdown nested [label [nested]](Grow).\r\n",
        "Markdown escaped [asset](<../../assets/diagram (1)%.bin>).\r",
        "Escaped delimiter: \\[[Grow]].\r\n",
        "Inline code: `[[Grow]]`.\r\n",
        "```md\r\n",
        "[[Grow]]\r\n",
        "```\r\n",
        "Unicode tail: \u{d55c}\u{ae00} e\u{301} \u{1f469}\u{200d}\u{1f4bb}.\r"
    )
    .as_bytes()
    .to_vec();
    fs::write(root.join("deep/nested/Index.md"), &source).expect("write BOM/CRLF Markdown vector");
    source
}

fn lone_cr_count(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .enumerate()
        .filter(|(index, byte)| **byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
        .count()
}

#[test]
fn lone_cr_drives_markdown_line_fence_block_and_link_semantics_with_raw_spans() {
    let temporary = tempfile::tempdir().expect("temporary lone-CR semantic root");
    let source_root = temporary.path().join("source");
    fs::create_dir_all(&source_root).expect("create lone-CR semantic source");
    let bytes = b"# Lone CR\r\r```md\r[[Hidden]]\r```\r\r[[Visible]]\r";
    fs::write(source_root.join("LoneCr.md"), bytes).expect("write lone-CR semantic vector");

    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("lone-cr-semantics", &source_root)
            .expect("lone-CR semantic source")])
        .expect("inspect lone-CR semantic vector");
    let document = inspection
        .workspace
        .documents
        .values()
        .next()
        .expect("parsed lone-CR document");
    assert_eq!(
        document.comparison_text,
        "# Lone CR\n\n```md\n[[Hidden]]\n```\n\n[[Visible]]\n"
    );
    assert_eq!(document.sections.len(), 1);
    assert_eq!(document.sections[0].heading, "Lone CR");
    let raw_span = |span: &vaultc::diagnostic::SourceSpan| {
        &bytes[usize::try_from(span.byte_start).expect("raw span start")
            ..usize::try_from(span.byte_end).expect("raw span end")]
    };
    assert_eq!(raw_span(&document.sections[0].span), b"# Lone CR\r");
    assert_eq!(
        document
            .blocks
            .iter()
            .map(|block| block.kind)
            .collect::<Vec<_>>(),
        vec![BlockKind::Heading, BlockKind::Code, BlockKind::Paragraph]
    );
    assert_eq!(raw_span(&document.blocks[0].span), b"# Lone CR\r");
    assert_eq!(
        raw_span(&document.blocks[1].span),
        b"```md\r[[Hidden]]\r```\r"
    );
    assert_eq!(raw_span(&document.blocks[2].span), b"[[Visible]]\r");
    assert_eq!(document.links.len(), 1, "fenced link remains non-semantic");
    assert_eq!(document.links[0].raw_target, "Visible");
    assert_eq!(raw_span(&document.links[0].span), b"Visible");
    assert_eq!(document.links[0].resolution, LinkResolution::Pending);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one BOM acceptance binds semantic exclusion, raw spans, exact deduplication, and byte-preserving copies"
)]
fn markdown_bom_is_semantic_only_at_offset_zero_and_spans_use_raw_offsets() {
    let temporary = tempfile::tempdir().expect("temporary BOM corpus root");
    let source_root = temporary.path().join("source");
    fs::create_dir_all(&source_root).expect("create BOM corpus source");
    let body = "# Same\r\n\r\n[[Missing]]\r";
    let leading = format!("\u{feff}{body}").into_bytes();
    let without_bom = body.as_bytes().to_vec();
    let frontmatter = "---\r\nfixture: adjacent-bom\r\n---\r\n";
    let adjacent = format!("{frontmatter}\u{feff}{body}").into_bytes();
    fs::write(source_root.join("Leading.md"), &leading).expect("write leading BOM vector");
    fs::write(source_root.join("Plain.md"), &without_bom).expect("write plain BOM control");
    fs::write(source_root.join("Adjacent.md"), &adjacent)
        .expect("write frontmatter-adjacent BOM vector");

    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("bom-vectors", &source_root).expect("BOM vector source")])
        .expect("inspect BOM vectors");
    let document = |path: &str| {
        inspection
            .workspace
            .documents
            .values()
            .find(|document| document.source_file.logical_path == path)
            .unwrap_or_else(|| panic!("BOM vector document {path}"))
    };
    let leading_document = document("Leading.md");
    let without_bom_document = document("Plain.md");
    let adjacent_document = document("Adjacent.md");
    assert_eq!(
        leading_document.comparison_text,
        without_bom_document.comparison_text
    );
    assert_eq!(leading_document.body_hash, without_bom_document.body_hash);
    assert!(!leading_document.comparison_text.starts_with('\u{feff}'));
    assert!(
        adjacent_document
            .comparison_text
            .starts_with("\u{feff}# Same\n"),
        "a BOM after frontmatter is content, not another file marker"
    );
    assert_ne!(adjacent_document.body_hash, without_bom_document.body_hash);

    for (document, bytes) in [
        (leading_document, leading.as_slice()),
        (without_bom_document, without_bom.as_slice()),
        (adjacent_document, adjacent.as_slice()),
    ] {
        assert_eq!(document.links.len(), 1);
        let link = &document.links[0];
        let start = usize::try_from(link.span.byte_start).expect("BOM link span start");
        let end = usize::try_from(link.span.byte_end).expect("BOM link span end");
        assert_eq!(&bytes[start..end], b"Missing");
        assert_eq!(
            start,
            bytes
                .windows(b"Missing".len())
                .position(|window| window == b"Missing")
                .expect("raw Missing target offset")
        );
        assert_eq!(
            document.source_file.original_path,
            document.source_file.logical_path
        );
        assert_eq!(document.source_file.path_encoding, SourcePathEncoding::Utf8);
    }
    assert_eq!(
        leading_document.links[0].span.byte_start,
        without_bom_document.links[0].span.byte_start + 3,
        "all source spans remain coordinates in the BOM-bearing raw byte stream"
    );

    let plan = compiler.plan(&inspection).expect("plan BOM vectors");
    assert!(plan.exact_groups.iter().any(|group| {
        group.members.contains(&leading_document.document_id)
            && group.members.contains(&without_bom_document.document_id)
    }));
    let copies: Vec<_> = plan
        .operations
        .iter()
        .map(|operation| match operation {
            OutputOperation::Copy {
                source_path,
                destination,
                expected_hash,
                kind: FileKind::Markdown,
                ..
            } => {
                let source_bytes = match source_path.as_str() {
                    "Leading.md" => leading.as_slice(),
                    "Plain.md" => without_bom.as_slice(),
                    "Adjacent.md" => adjacent.as_slice(),
                    other => panic!("unexpected BOM-vector source operation: {other}"),
                };
                assert_eq!(*expected_hash, ContentHash::from_bytes(source_bytes));
                (destination.clone(), source_bytes.to_vec())
            }
            other => panic!("BOM vectors require byte-preserving copies: {other:?}"),
        })
        .collect();
    assert_eq!(
        copies.len(),
        2,
        "leading/plain are one exact semantic group while adjacent BOM remains distinct"
    );
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve BOM vector copy plan");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile BOM vector artifact");
    for (path, bytes) in copies {
        assert_eq!(
            fs::read(artifact.join(path)).expect("read compiled BOM vector"),
            bytes
        );
    }
    compiler
        .verify(&artifact)
        .expect("verify BOM vector artifact");
}

fn apply_replacements(mut source: Vec<u8>, replacements: &[RewriteReplacement]) -> Vec<u8> {
    for replacement in replacements.iter().rev() {
        let start = usize::try_from(replacement.span.byte_start).expect("replacement start");
        let end = usize::try_from(replacement.span.byte_end).expect("replacement end");
        source.splice(start..end, replacement.replacement.bytes());
    }
    source
}

fn markdown_rewrite(plan: &DraftPlan) -> (String, String, ContentHash, Vec<RewriteReplacement>) {
    plan.operations
        .iter()
        .find_map(|operation| match operation {
            OutputOperation::RewriteMarkdown {
                source_path,
                destination,
                expected_output_hash,
                replacements,
                ..
            } if source_path == "deep/nested/Index.md" => Some((
                source_path.clone(),
                destination.clone(),
                *expected_output_hash,
                replacements.clone(),
            )),
            _ => None,
        })
        .expect("BOM/CRLF Markdown source requires a rewrite")
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one byte-preservation corpus asserts all sealed Markdown replacement spans and untouched syntax together"
)]
fn markdown_crlf_bom_unicode_escaping_and_multiple_grow_shrink_rewrites_preserve_bytes() {
    let temporary = tempfile::tempdir().expect("temporary Markdown vector root");
    let source_root = temporary.path().join("source");
    let source = write_markdown_vectors(&source_root);
    let source_tree_before = common::tree_bytes(&source_root);
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([SourceSpec::directory("markdown-vectors", &source_root)
            .expect("Markdown vector source")])
        .expect("inspect Markdown normalization vectors");
    let document = inspection
        .workspace
        .documents
        .values()
        .find(|document| document.source_file.logical_path == "deep/nested/Index.md")
        .expect("parsed Markdown vector document");
    assert!(document.comparison_text.starts_with("# Rewrite vectors\n"));
    assert!(!document.comparison_text.contains('\r'));
    assert!(!document.comparison_text.contains('\u{feff}'));
    assert!(document.comparison_text.contains("Caf\u{e9} \u{1f642}"));
    assert!(
        document
            .comparison_text
            .contains("\u{d55c}\u{ae00} \u{e9} \u{1f469}\u{200d}\u{1f4bb}")
    );
    assert_eq!(
        document.links.len(),
        5,
        "escaped, inline-code, and fenced wikilinks are not rewrite spans"
    );

    let plan = compiler
        .plan(&inspection)
        .expect("plan Markdown normalization vectors");
    let (_, destination, expected_output_hash, replacements) = markdown_rewrite(&plan);
    assert_eq!(replacements.len(), 5);
    assert!(replacements.windows(2).all(|pair| {
        pair[0].span.byte_end <= pair[1].span.byte_start
            && pair[0].span.byte_start < pair[0].span.byte_end
    }));
    let original_lengths: Vec<_> = replacements
        .iter()
        .map(|replacement| replacement.span.byte_end - replacement.span.byte_start)
        .collect();
    let growing = replacements
        .iter()
        .zip(&original_lengths)
        .filter(|(replacement, original)| (replacement.replacement.len() as u64) > **original)
        .count();
    let shrinking = replacements
        .iter()
        .zip(&original_lengths)
        .filter(|(replacement, original)| (replacement.replacement.len() as u64) < **original)
        .count();
    assert!(growing >= 2, "fixture must exercise multiple growing spans");
    assert!(
        shrinking >= 2,
        "fixture must exercise multiple shrinking spans"
    );
    assert!(replacements.iter().any(|replacement| {
        replacement
            .replacement
            .contains("Long%20Target%20%28one%29%20100%25.md")
    }));
    assert!(replacements.iter().any(|replacement| {
        replacement.replacement == "../Short.md#\u{c9e7}\u{c740} \u{c81c}\u{baa9}|\u{cd95}\u{c57d}"
    }));
    assert!(
        replacements
            .iter()
            .any(|replacement| { replacement.replacement.contains("diagram%20%281%29%25.bin") })
    );

    let expected = apply_replacements(source.clone(), &replacements);
    assert_eq!(
        expected_output_hash,
        ContentHash::from_bytes(&expected),
        "expected output hash must bind the exact independently span-applied bytes"
    );
    assert!(expected.starts_with("\u{feff}".as_bytes()));
    assert_eq!(
        source.windows(2).filter(|bytes| *bytes == b"\r\n").count(),
        expected
            .windows(2)
            .filter(|bytes| *bytes == b"\r\n")
            .count(),
        "rewriting target spans must not normalize untouched line endings"
    );
    assert!(lone_cr_count(&source) >= 2);
    assert_eq!(lone_cr_count(&source), lone_cr_count(&expected));
    let expected_text = std::str::from_utf8(&expected).expect("rewritten vector is UTF-8");
    assert!(expected_text.contains("Escaped delimiter: \\[[Grow]].\r\n"));
    assert!(expected_text.contains("Inline code: `[[Grow]]`.\r\n"));
    assert!(expected_text.contains("```md\r\n[[Grow]]\r\n```\r\n"));
    assert!(expected_text.contains("Cafe\u{301} \u{1f642} before"));
    assert!(
        expected_text
            .contains("Unicode tail: \u{d55c}\u{ae00} e\u{301} \u{1f469}\u{200d}\u{1f4bb}.\r")
    );

    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve Markdown vector plan");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile Markdown vector artifact");
    assert_eq!(
        fs::read(artifact.join(&destination)).expect("read rewritten Markdown vector"),
        expected
    );
    compiler
        .verify(&artifact)
        .expect("verify Markdown vector artifact");
    assert_eq!(common::tree_bytes(&source_root), source_tree_before);
}

fn write_canvas_vectors(root: &Path) -> (Vec<u8>, Vec<u8>) {
    fs::create_dir_all(root.join("boards/nested")).expect("create Canvas source directory");
    fs::create_dir_all(root.join("plain")).expect("create unchanged Canvas directory");
    fs::create_dir_all(root.join("notes")).expect("create Canvas target directory");
    fs::create_dir_all(root.join("assets")).expect("create Canvas asset directory");
    fs::write(root.join("notes/Caf\u{e9}.md"), "# Caf\u{e9}\n").expect("write NFC Canvas target");
    fs::write(root.join("assets/space file %.bin"), b"canvas asset")
        .expect("write Canvas normalization asset");

    let rewritten = concat!(
        "{\r\n",
        "  \"nodes\": [\r\n",
        "    {\"id\":\"nfd-document\",\"type\":\"file\",\"file\":\"../../notes/Cafe\\u0301.md\",\"x\":0,\"y\":0,\"width\":100,\"height\":100,\"unknown\":{\"semantic_nfd\":\"e\\u0301\",\"emoji\":\"🧠\"}},\r\n",
        "    {\"id\":\"asset-spaces\",\"type\":\"file\",\"file\":\"../../assets/space file %.bin\",\"x\":120,\"y\":0,\"width\":100,\"height\":100,\"unknown\":[\"line\\nquote\\\"slash\\\\\",7]},\r\n",
        "    {\"id\":\"unresolved-nfd\",\"type\":\"file\",\"file\":\"../../missing/Ghoste\\u0301.md\",\"x\":240,\"y\":0,\"width\":100,\"height\":100,\"unknown\":{\"raw\":true}}\r\n",
        "  ],\r\n",
        "  \"edges\": [],\r\n",
        "  \"root_unknown\": {\"decomposed\":\"Cafe\\u0301\",\"order\":[3,1,2]}\r\n",
        "}\r\n"
    )
    .as_bytes()
    .to_vec();
    fs::write(root.join("boards/nested/Vector.canvas"), &rewritten)
        .expect("write rewritten Canvas vector");

    let unchanged = concat!(
        "{\r\n",
        " \"nodes\":[{\"id\":\"text\",\"type\":\"text\",\"text\":\"Cafe\\u0301 👾\",\"x\":0,\"y\":0,\"width\":80,\"height\":40}],\r\n",
        " \"edges\":[],\r\n",
        " \"unknown\":{\"escaped\":\"\\u0065\\u0301\"}\r\n",
        "}\r\n"
    )
    .as_bytes()
    .to_vec();
    fs::write(root.join("plain/Unchanged.canvas"), &unchanged)
        .expect("write unchanged Canvas vector");
    (rewritten, unchanged)
}

fn operation_for_source<'a>(plan: &'a DraftPlan, source_path: &str) -> &'a OutputOperation {
    plan.operations
        .iter()
        .find(|operation| match operation {
            OutputOperation::Copy {
                source_path: source,
                ..
            }
            | OutputOperation::RewriteMarkdown {
                source_path: source,
                ..
            }
            | OutputOperation::RewriteCanvas {
                source_path: source,
                ..
            } => source == source_path,
        })
        .unwrap_or_else(|| panic!("operation for {source_path}"))
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one Canvas acceptance binds normalized lookup, raw-value preservation, unknown fields, deterministic rewrite, and unchanged copy"
)]
fn canvas_normalization_vectors_resolve_nfd_preserve_unknowns_and_copy_unchanged_bytes() {
    let temporary = tempfile::tempdir().expect("temporary Canvas vector root");
    let source_root = temporary.path().join("source");
    let (rewritten_source, unchanged_source) = write_canvas_vectors(&source_root);
    let source_tree_before = common::tree_bytes(&source_root);
    let compiler = common::compiler();
    let source =
        || SourceSpec::directory("canvas-vectors", &source_root).expect("Canvas vector source");
    let inspection = compiler
        .inspect([source()])
        .expect("inspect Canvas normalization vectors");
    let repeated = compiler
        .inspect([source()])
        .expect("repeat Canvas normalization inspection");
    let plan = compiler
        .plan(&inspection)
        .expect("plan Canvas normalization vectors");
    let repeated_plan = compiler
        .plan(&repeated)
        .expect("repeat Canvas normalization plan");
    assert_eq!(
        serde_json::to_vec(&plan).expect("serialize Canvas vector plan"),
        serde_json::to_vec(&repeated_plan).expect("serialize repeated Canvas vector plan")
    );
    let sealed_canvas = plan
        .workspace
        .canvases
        .values()
        .find(|canvas| canvas.source_file.logical_path == "boards/nested/Vector.canvas")
        .expect("sealed Canvas normalization vector");
    let resolved_reference = sealed_canvas
        .file_references
        .iter()
        .find(|reference| reference.node_id == "nfd-document")
        .expect("sealed NFD Canvas reference");
    assert_eq!(resolved_reference.raw_path, "../../notes/Cafe\u{301}.md");
    assert!(matches!(
        resolved_reference.resolution,
        CanvasReferenceResolution::Resolved { .. }
    ));
    let unresolved_reference = sealed_canvas
        .file_references
        .iter()
        .find(|reference| reference.node_id == "unresolved-nfd")
        .expect("sealed unresolved NFD Canvas reference");
    assert_eq!(
        unresolved_reference.raw_path,
        "../../missing/Ghoste\u{301}.md"
    );
    assert_eq!(
        unresolved_reference.resolution,
        CanvasReferenceResolution::Unresolved
    );

    let (destination, expected_output_hash, rewrites) =
        match operation_for_source(&plan, "boards/nested/Vector.canvas") {
            OutputOperation::RewriteCanvas {
                destination,
                expected_output_hash,
                rewrites,
                ..
            } => (destination.clone(), *expected_output_hash, rewrites.clone()),
            operation => panic!("expected rewritten Canvas operation, got {operation:?}"),
        };
    assert_eq!(rewrites.len(), 2);
    let document_rewrite = rewrites
        .iter()
        .find(|rewrite| rewrite.node_id == "nfd-document")
        .expect("NFD document Canvas rewrite");
    assert_eq!(document_rewrite.original_path, "../../notes/Cafe\u{301}.md");
    assert!(document_rewrite.replacement_path.contains("Caf\u{e9}.md"));
    assert!(!document_rewrite.replacement_path.contains("Cafe\u{301}.md"));
    let asset_rewrite = rewrites
        .iter()
        .find(|rewrite| rewrite.node_id == "asset-spaces")
        .expect("space-containing asset Canvas rewrite");
    assert!(asset_rewrite.replacement_path.contains("space file %.bin"));
    assert!(!asset_rewrite.replacement_path.contains("%20"));
    assert!(!asset_rewrite.replacement_path.contains("%25"));

    let unchanged_destination = match operation_for_source(&plan, "plain/Unchanged.canvas") {
        OutputOperation::Copy {
            destination,
            kind: FileKind::Canvas,
            expected_hash,
            ..
        } => {
            assert_eq!(*expected_hash, ContentHash::from_bytes(&unchanged_source));
            destination.clone()
        }
        operation => panic!("expected unchanged Canvas copy, got {operation:?}"),
    };

    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve Canvas vector plan");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile Canvas vector artifact");
    compiler
        .verify(&artifact)
        .expect("verify Canvas vector artifact");

    let rewritten_output = fs::read(artifact.join(&destination)).expect("read rewritten Canvas");
    assert_eq!(
        expected_output_hash,
        ContentHash::from_bytes(&rewritten_output)
    );
    assert!(!rewritten_output.windows(2).any(|bytes| bytes == b"\r\n"));
    let source_value: Value =
        serde_json::from_slice(&rewritten_source).expect("parse source Canvas vector");
    let output_value: Value =
        serde_json::from_slice(&rewritten_output).expect("parse rewritten Canvas vector");
    assert_eq!(output_value["root_unknown"], source_value["root_unknown"]);
    assert_eq!(
        output_value["nodes"][0]["unknown"],
        source_value["nodes"][0]["unknown"]
    );
    assert_eq!(
        output_value["nodes"][1]["unknown"],
        source_value["nodes"][1]["unknown"]
    );
    assert_eq!(
        output_value["nodes"][2]["unknown"],
        source_value["nodes"][2]["unknown"]
    );
    assert_eq!(output_value["root_unknown"]["decomposed"], "Cafe\u{301}");
    assert_eq!(
        output_value["nodes"][0]["file"],
        document_rewrite.replacement_path
    );
    assert_eq!(
        output_value["nodes"][1]["file"],
        asset_rewrite.replacement_path
    );
    assert_eq!(
        output_value["nodes"][2]["file"], "../../missing/Ghoste\u{301}.md",
        "normalization is lookup-only for an unresolved raw Canvas file value"
    );
    assert_eq!(
        rewritten_output,
        vaultc::canonical::to_canonical_json_pretty(&output_value)
            .expect("canonicalize rewritten Canvas value")
    );
    assert_eq!(
        fs::read(artifact.join(&unchanged_destination)).expect("read unchanged Canvas output"),
        unchanged_source,
        "Canvas without rewrites must retain exact CRLF and JSON escape bytes"
    );
    assert_eq!(common::tree_bytes(&source_root), source_tree_before);
}

fn unsafe_markdown_targets() -> [(&'static str, &'static str, bool); 7] {
    [
        ("percent-decoded-parent", "%2e%2e/%2e%2e/escape.md", true),
        ("raw-parent", "../../escape.md", true),
        ("absolute", "/x", false),
        ("drive-slash", "C:/x", false),
        ("drive-backslash", r"C:\x", false),
        ("unc-slash", "//server/share", false),
        ("unc-backslash", r"\\server\share", false),
    ]
}

fn unsafe_canvas_targets() -> [(&'static str, &'static str); 6] {
    [
        ("raw-parent", "../../escape.md"),
        ("absolute", "/x"),
        ("drive-slash", "C:/x"),
        ("drive-backslash", r"C:\x"),
        ("unc-slash", "//server/share"),
        ("unc-backslash", r"\\server\share"),
    ]
}

#[test]
fn unsafe_markdown_targets_fail_closed_before_preservation_or_waiver() {
    for (name, target, markdown_syntax) in unsafe_markdown_targets() {
        let temporary = tempfile::tempdir().expect("temporary unsafe Markdown root");
        let source_root = temporary.path().join("source");
        fs::create_dir_all(&source_root).expect("create unsafe Markdown source");
        let link = if markdown_syntax {
            format!("[unsafe]({target})")
        } else {
            format!("[[{target}]]")
        };
        fs::write(
            source_root.join("Index.md"),
            format!("# Unsafe {name}\n\n{link}\n"),
        )
        .expect("write unsafe Markdown vector");
        let compiler = common::compiler();
        let inspection = compiler
            .inspect([
                SourceSpec::directory(format!("markdown-{name}"), &source_root)
                    .expect("unsafe Markdown source"),
            ])
            .expect("unsafe Markdown syntax remains parseable data");
        let error = compiler
            .plan(&inspection)
            .expect_err("unsafe unresolved Markdown target must fail closed");
        assert!(
            matches!(error, VaultcError::UnsafePath { .. }),
            "Markdown target {target} failed with wrong boundary: {error}"
        );
        assert!(!temporary.path().join("escape.md").exists());
    }
}

#[test]
fn unsafe_canvas_targets_fail_closed_before_preservation_or_waiver() {
    for (name, target) in unsafe_canvas_targets() {
        let temporary = tempfile::tempdir().expect("temporary unsafe Canvas root");
        let source_root = temporary.path().join("source");
        fs::create_dir_all(&source_root).expect("create unsafe Canvas source");
        let canvas = serde_json::json!({
            "nodes": [{
                "id": "unsafe",
                "type": "file",
                "file": target,
                "x": 0,
                "y": 0,
                "width": 100,
                "height": 100
            }],
            "edges": [],
            "fixture": name
        });
        fs::write(
            source_root.join("Board.canvas"),
            serde_json::to_vec(&canvas).expect("encode unsafe Canvas vector"),
        )
        .expect("write unsafe Canvas vector");
        let compiler = common::compiler();
        let inspection = compiler
            .inspect([
                SourceSpec::directory(format!("canvas-{name}"), &source_root)
                    .expect("unsafe Canvas source"),
            ])
            .expect("unsafe Canvas syntax remains parseable data");
        let error = compiler
            .plan(&inspection)
            .expect_err("unsafe unresolved Canvas target must fail closed");
        assert!(
            matches!(error, VaultcError::UnsafePath { .. }),
            "Canvas target {target} failed with wrong boundary: {error}"
        );
        assert!(!temporary.path().join("escape.md").exists());
    }
}

#[test]
fn canvas_percent_encoded_parent_segments_remain_literal_unresolved_data() {
    let temporary = tempfile::tempdir().expect("temporary percent-literal Canvas root");
    let source_root = temporary.path().join("source");
    fs::create_dir_all(&source_root).expect("create percent-literal Canvas source");
    let raw_target = "%2e%2e/%2e%2e/escape.md";
    let bytes = serde_json::to_vec(&serde_json::json!({
        "nodes": [{
            "id": "literal-percent",
            "type": "file",
            "file": raw_target,
            "x": 0,
            "y": 0,
            "width": 100,
            "height": 100
        }],
        "edges": [],
        "unknown": {"percent_decoding": false}
    }))
    .expect("encode percent-literal Canvas");
    fs::write(source_root.join("Board.canvas"), &bytes).expect("write percent-literal Canvas");

    let compiler = common::compiler();
    let inspection = compiler
        .inspect([
            SourceSpec::directory("canvas-percent-literal", &source_root)
                .expect("percent-literal Canvas source"),
        ])
        .expect("inspect percent-literal Canvas");
    let plan = compiler
        .plan(&inspection)
        .expect("percent-encoded Canvas target is not traversal syntax");
    let canvas = plan
        .workspace
        .canvases
        .values()
        .next()
        .expect("sealed percent-literal Canvas");
    assert_eq!(canvas.file_references.len(), 1);
    assert_eq!(canvas.file_references[0].raw_path, raw_target);
    assert_eq!(
        canvas.file_references[0].resolution,
        CanvasReferenceResolution::Unresolved
    );
    let destination = match operation_for_source(&plan, "Board.canvas") {
        OutputOperation::Copy {
            destination,
            expected_hash,
            kind: FileKind::Canvas,
            ..
        } => {
            assert_eq!(*expected_hash, ContentHash::from_bytes(&bytes));
            destination.clone()
        }
        operation => panic!("literal unresolved Canvas must copy unchanged: {operation:?}"),
    };
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve percent-literal Canvas copy");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile percent-literal Canvas");
    assert_eq!(
        fs::read(artifact.join(destination)).expect("read percent-literal Canvas output"),
        bytes
    );
    assert!(
        compiler
            .verify(&artifact)
            .expect("verify Canvas output")
            .valid
    );
}
