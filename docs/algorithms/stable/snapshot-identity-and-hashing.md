---
title: ALG-SNP-001 — Snapshot Identity and Hashing
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0004
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-SNP-001: Snapshot Identity and Hashing

## Objective and non-goals

Produce stable, domain-separated identities for bytes, files, records, and complete Vault snapshots independent of enumeration order and host metadata. This does not prove authorship, confidentiality, or semantic equivalence.

## Inputs and outputs

Input is a `SourceId`, versioned identity policy, and a safely enumerated set of `(logical_path, kind, bytes)`. Output is a sorted manifest, `SourceFileId` values, and one `SnapshotId`.

## Formula

Let `H(x) = SHA-256(x)` and `||` be byte concatenation. Each component is length-prefixed unsigned LEB128 to prevent ambiguity.

```text
ContentHash  = H("vaultc:content:v1\0" || bytes)
SourceFileId = H("vaultc:file:v1\0" || lp(path) || lp(kind) || ContentHash)
SnapshotId   = H("vaultc:snapshot:v1\0" || lp(SourceId) || lp(policy_id)
                 || concat(sorted(lp(path) || SourceFileId)))
```

Hex rendering uses lowercase 64-character SHA-256. Typed IDs carry a textual prefix outside the hash, for example `snap_...`; the prefix is not part of the formula unless a schema explicitly says so.

## Symbols

| Symbol | Meaning | Type/range/unit | Default |
|---|---|---|---|
| `H` | SHA-256 function | bytes → 32 bytes | SHA-256 |
| `bytes` | exact source-file bytes | 0..configured file limit bytes | none |
| `lp(x)` | canonical length-prefixed byte string | unsigned length + UTF-8/bytes | ULEB128 length |
| `path` | safe normalized logical path | relative UTF-8 path | ALG-NRM-001 |
| `kind` | classified input kind/media family | versioned ASCII enum | detected |
| `SourceId` | stable user/domain source identity | non-empty UTF-8, policy-limited | required |
| `policy_id` | identity-affecting inclusion/normalization policy | versioned ASCII ID | `vaultc-source-v1` |
| `sorted` | ascending comparison | unsigned UTF-8 bytes of path | required |
| `concat` | unambiguous concatenation | bytes | required |

## Pseudocode

```text
manifest = []
for entry in safe_enumerate(source):
    path = normalize_logical_path(entry.path)
    content_hash = sha256(domain_content || entry.bytes_stream)
    file_id = sha256(domain_file || lp(path) || lp(entry.kind) || content_hash)
    manifest.append(path, entry.kind, entry.size, content_hash, file_id)
sort manifest by unsigned UTF-8 path bytes
snapshot_id = sha256(domain_snapshot || lp(source_id) || lp(policy_id)
                     || encode_each(manifest.path, manifest.file_id))
seal manifest; return snapshot_id, manifest
```

Hashing MUST stream bytes, check byte count, and re-stat/reopen according to platform race policy. A changed file during inspection invalidates the snapshot attempt.

## Complexity

For `B` total bytes and `F` files: time `O(B + F log F)`, retained manifest memory `O(F)` (or external/SQLite sort), streaming buffer `O(1)` relative to `B`.

## Edge and security cases

Reject duplicate normalized paths, unsafe symlinks, special devices, changing files, paths outside the root, unsupported raw filename encodings under policy, and resource-limit violations. Hashes are integrity identities, not signatures; authenticity requires a signing layer.

## Worked example

Source `alpha` contains `A.md` with UTF-8 bytes `Hello\n` and `z.png` with bytes `00 ff`. The algorithm hashes exact bytes, creates each file ID with its kind and logical path, sorts `A.md` before `z.png`, then commits both IDs and source/policy identities into the snapshot. Renaming `A.md` changes the file and snapshot IDs even if its bytes do not; changing filesystem mtime does not.

## Golden vectors

These primitives MUST match standard SHA-256:

| Input | Expected raw SHA-256 |
|---|---|
| empty bytes | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| ASCII `abc` | `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` |

Repository fixtures MUST freeze complete domain-separated IDs after the encoder is implemented. A schema encoder change requires a new domain version and migration ADR; it must never silently update V1 vectors.

## Correctness and rollback

Correctness metrics are byte-for-byte manifest equality, permutation invariance of enumeration, platform semantic-ID equality, and mutation detection. On any mismatch or race, fail the snapshot; do not fall back to timestamps, file sizes, or partial manifests.
