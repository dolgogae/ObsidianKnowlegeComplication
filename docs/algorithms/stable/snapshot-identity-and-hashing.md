---
title: ALG-SNP-001 — Snapshot Identity and Hashing
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0012
  - ADR-0015
  - ADR-0016
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-SNP-001: Snapshot Identity and Hashing

## Objective and non-goals

Produce stable, domain-separated identities for bytes, files, records, and complete Vault snapshots independent of enumeration order and host metadata. This does not prove authorship, confidentiality, or semantic equivalence.

## Inputs and outputs

Input is a `SourceId`, optional non-identifying owner display name, versioned
identity policy, and a safely enumerated set of `(logical_path, kind, bytes)`.
Output is a sorted manifest, `SourceFileId` values, one `SnapshotId`, one
source-independent `VaultContentId`, and typed identities for parsed IR items.

## Formula

Let `H(x) = SHA-256(x)` and `||` be byte concatenation. Each component is length-prefixed unsigned LEB128 to prevent ambiguity.

```text
ContentHash  = H("okc:content:v2\0" || bytes)
SourceFileId = H("okc:file:v2\0" || lp(path) || lp(kind) || ContentHash)
SnapshotId   = H("okc:snapshot:v2\0" || lp(SourceId) || lp(policy_id)
                 || concat(sorted(lp(path) || SourceFileId)))
DocumentId     = H("okc:document:v2\0" || lp(raw(SnapshotId))
                    || lp(raw(SourceFileId)))
CanvasId       = H("okc:canvas:v2\0" || lp(raw(SnapshotId))
                    || lp(raw(SourceFileId)))
BaseArtifactId = H("okc:base:v2\0" || lp(raw(SnapshotId))
                    || lp(raw(SourceFileId)))
SectionId      = H("okc:section:v2\0" || lp(raw(DocumentId))
                    || lp(index_u64_be) || lp(heading_utf8))
BlockId        = H("okc:block:v2\0" || lp(raw(DocumentId))
                    || lp(index_u64_be) || lp(BlockContentHash))
```

`VaultContentId` is the V2 canonical-JSON hash under
`"okc:vault-content:v2\0"` of the strictly path-sorted sequence of
`(logical_path, SourceFileId)`. It deliberately excludes `SourceId`, owner
display name, timestamps, permissions, and enumeration order. It therefore
detects registering the same Vault bytes twice under different source IDs,
while `SnapshotId` remains source-specific.

Hex rendering uses lowercase 64-character SHA-256. Typed IDs carry a textual prefix outside the hash, for example `snap_...`; the prefix is not part of the formula unless a schema explicitly says so.
`raw(TypedId)` means its 32 hash bytes without the textual prefix. The three
file-level IR identities deliberately use distinct domains even when they bind
the same snapshot/file pair; a logical path or display name is never used as a
substitute identity.

## Symbols

| Symbol | Meaning | Type/range/unit | Default |
|---|---|---|---|
| `H` | SHA-256 function | bytes → 32 bytes | SHA-256 |
| `bytes` | exact source-file bytes | 0..configured file limit bytes | none |
| `lp(x)` | canonical length-prefixed byte string | unsigned length + UTF-8/bytes | ULEB128 length |
| `path` | safe NFC normalized logical path | relative UTF-8 path | ALG-NRM-001 |
| `original_path` | exact accepted pre-NFC component spelling | relative UTF-8 `/` path | ADR-0012 |
| `kind` | classified input kind/media family | versioned ASCII enum | detected |
| `SourceId` | stable user/domain source identity | non-empty UTF-8, policy-limited | required |
| `SourceFileId` | immutable normalized-path/kind/content identity | 256-bit ID | required |
| `SnapshotId` | immutable source snapshot identity | 256-bit ID | required |
| `VaultContentId` | source-independent identity of the accepted Vault manifest | 256-bit hash | required |
| `policy_id` | identity-affecting inclusion/normalization policy | versioned ASCII ID | `okc-source-v2` |
| `sorted` | ascending comparison | unsigned UTF-8 bytes of path | required |
| `concat` | unambiguous concatenation | bytes | required |

## Pseudocode

```text
manifest = []
for entry in safe_enumerate(source):
    original_path = validate_and_join_utf8_components(entry.raw_path)
    path = normalize_logical_path(original_path)
    content_hash = sha256(domain_content || entry.bytes_stream)
    file_id = sha256(domain_file || lp(path) || lp(entry.kind) || content_hash)
    manifest.append(path, entry.kind, entry.size, raw_sha256, content_hash, file_id)
sort manifest by unsigned UTF-8 path bytes
snapshot_id = sha256(domain_snapshot || lp(source_id) || lp(policy_id)
                     || encode_each(manifest.path, manifest.file_id))
vault_content_id = canonical_hash(domain_vault_content,
                                  encode_json_each(manifest.path, manifest.file_id))
seal manifest; return snapshot_id, vault_content_id, manifest
```

`original_path` is carried in the sealed manifest/IR and later Plan and
provenance records, but it is deliberately not an input to `SourceFileId` or
`SnapshotId`. Compilation still compares it during the source reread; a
post-plan spelling change is stale even when these lower semantic IDs match.

Hashing MUST stream bytes, check byte count, and re-stat/reopen according to platform race policy. A changed file during inspection invalidates the snapshot attempt.

Before planning, snapshots are canonical-sorted by `SourceId` and MUST have
strictly unique `SourceId` and `VaultContentId` values. Reordering input Vaults
therefore cannot affect inspection or plan identity. Reusing one `SourceId`
for another snapshot or registering identical accepted Vault content twice is
an error.

## Complexity

For `B` total bytes and `F` files: time `O(B + F log F)`, retained manifest memory `O(F)` (or external/SQLite sort), streaming buffer `O(1)` relative to `B`.

## Edge and security cases

Reject duplicate normalized paths, duplicate `SourceId`, duplicate
`VaultContentId`, unsafe symlinks or reparse points, special devices, changing
files, paths outside the root, unsupported raw filename encodings under
policy, and resource-limit violations. Owner display names and MCP product
names MUST NOT enter identity or policy. Hashes are integrity identities, not
signatures; authenticity requires a signing layer.

## Worked example

Source `alpha` contains `A.md` with UTF-8 bytes `Hello\n` and `z.png` with bytes `00 ff`. The algorithm hashes exact bytes, creates each file ID with its kind and logical path, sorts `A.md` before `z.png`, then commits both IDs and source/policy identities into the snapshot. Renaming `A.md` changes the file and snapshot IDs even if its bytes do not; changing filesystem mtime does not.

## Golden vectors

These primitives MUST match standard SHA-256:

| Input | Expected raw SHA-256 |
|---|---|
| empty bytes | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| ASCII `abc` | `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` |

Repository fixtures MUST freeze complete domain-separated IDs after the encoder is implemented. A schema encoder change requires a new domain version and migration ADR; it must never silently update V2 vectors.

## Correctness and rollback

Correctness metrics are byte-for-byte manifest equality, file- and
source-permutation invariance, MCP-origin neutrality, platform semantic-ID
equality, duplicate-Vault rejection, and mutation detection. On any mismatch
or race, fail the snapshot; do not fall back to timestamps, file sizes, or
partial manifests.
