---
title: Compiled Vault and VaultPack Format
status: normative-v1
owners:
  - core-rust-engineer
  - release-maintainer
last_updated: 2026-08-18
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0009
  - ADR-0010
  - ADR-0012
  - ADR-0013
  - ADR-0014
source_refs:
  - HIST-COMPILER-PLAN
---

# Compiled Vault and VaultPack Format

## Output layout

```text
CompiledVault/
├── knowledge/
│   ├── ... preserved/selected notes ...
│   └── _generated/ ... approved generated notes ...
├── attachments/
│   └── ab/abcdef...-sanitized-name.ext
├── canvases/
├── views/ ... opaque `.base` files ...
└── .vaultc/
    ├── manifest.json
    ├── plan.json
    ├── provenance.jsonl
    ├── conflicts.json
    ├── diagnostics.json
    ├── checksums.txt
    └── ai-transcript.jsonl
```

The output MUST NOT contain raw source Vault archives or a `_sources` copy. It MAY contain selected source-derived notes and assets, each tied to provenance.

`.vaultc/plan.json` contains the canonical, source-locator-redacted
`ApprovedPlan` envelope: its immutable `DraftPlan`, proposal
validations/approvals, conflict-decision overlay, and transcript. It is not a
bare `DraftPlan`.

Absolute or host-specific source locators are build inputs, not semantic
artifact data. The serialized audit plan MUST replace them with the stable
literal `[redacted-source-locator]`; manifests, provenance, diagnostics, and
transcripts MUST NOT leak an absolute input root. Snapshot/source IDs and
logical source paths remain so provenance is useful without revealing the host
filesystem.

## Path policy

All output paths use `/` in manifests, Unicode normalization defined by ALG-NRM-001, no leading slash, no drive/UNC prefix, no `.` or `..` segment, no control/NUL characters, and platform-portable component rules. Path comparison detects both exact and configured case-fold collisions before materialization.

Conflict suffixes are stable and derived from a short, collision-checked identity fragment, never traversal-prone user text. Sanitization produces a diagnostic. Provenance retains both the exact accepted UTF-8 source spelling and its NFC logical path; output paths use the NFC form.

## Generated note frontmatter

Generated Markdown begins with canonical YAML fields in this order:

```yaml
---
vaultc_generated: true
vaultc_pack_id: <pack-id-or-null>
vaultc_proposal_id: <proposal-id>
vaultc_confidence: <optional-calibrated-number>
vaultc_sources:
  - <evidence-reference-id>
---
```

`vaultc_confidence` MUST be omitted if it is not calibrated for the declared task/cohort. License and author attribution required by any source MUST remain reachable from the manifest and provenance, and SHOULD appear in generated content when policy requires visible attribution.

The `vaultc_sources` entries are ALG-PRV-001 `EvidenceId` values in the sealed
proposal evidence order. The current writer emits those canonical identities
and binds them to the approval materialization, provenance ledger, and exact
rendered bytes. `vaultc_pack_id` remains `null` because pack identity/signing
belongs to the future distribution profile.

## Manifest

The manifest includes format/schema/compiler versions, artifact ID, ordered source snapshot IDs, plan ID, configuration hash, approved proposal hashes, output file inventory, media types, sizes, hashes, license/attribution summaries, creation policy, and optional signature metadata. Wall-clock creation time is informational and excluded from reproducibility identity.

The `0.1.0` manifest contains `schema_version`, `compiler_version`,
`artifact_id`, `plan_id`, `policy_hash`, `projection_hash`, ordered source
snapshot IDs, approved proposal hashes, required
`provenance_schema_version`, required `provenance_graph_hash`, and
`files[{path, byte_len, sha256}]`. Its inventory excludes `manifest.json` and
`checksums.txt` and includes the stored provenance ledger. It does not yet
carry media types, license summaries, creation/distribution policy, or
signature metadata.

ADR-0010 freezes the non-circular inventory layers. The stored provenance
graph covers content plus plan/conflict/diagnostic/transcript audit outputs.
Provenance, manifest, and checksums themselves have virtual audit-envelope
records synthesized from their final bytes and exact inventories; those
records MUST NOT be serialized back into the provenance file.

## Checksums

Each `checksums.txt` line is `<64 lowercase raw-SHA-256 hex><two ASCII
spaces><validated normalized UTF-8 logical path>\n`, sorted by logical path.
Paths are literal rather than escaped because V1 path policy forbids control
characters, backslashes, and non-canonical forms. The file covers the manifest
and every other artifact file except itself and detached signatures as defined
by format version.

## Atomic materialization

The compiler verifies sources and destination parent, rejects source/output
overlap, creates a unique sibling staging directory with restrictive
permissions, writes and synchronizes files, synchronizes the completed staging
tree where supported, independently verifies it, and commits it to the
requested absent destination with the ADR-0014 platform no-replace primitive.
Existing output is rejected unless a separately specified, recoverable update
workflow is introduced by ADR.

Linux uses `renameat2(RENAME_NOREPLACE)` and macOS uses
`renameatx_np(RENAME_EXCL)` through the safe `rustix` API. Windows uses the safe
`atomicwrites::move_atomic` wrapper around `MoveFileExW` without replace. The
compiler never falls back to a replacing rename. Existing files, empty or
non-empty directories, live or dangling symlinks, supported reparse points,
and publication-race winners remain untouched. Unsupported primitives or
filesystems fail closed.

Before staging, the requested output must be disjoint from every immutable
source locator after existing-ancestor resolution, lexical normalization, and
portable NFC/full-case-fold comparison. Descriptor-relative ancestor pinning
is still a separate hardening boundary.

Caught pre-commit failures explicitly remove the exact known staging directory
or, only under the debug retention option, synchronize an incomplete marker
before retaining it. A disposition failure is reported together with its stage
and original error. If parent synchronization fails after the directory
commit, the complete Compiled Vault remains visible and compilation returns
`PublishedButDurabilityUncertain`; it is not deleted as a false rollback and
optional Pack publication does not start.

ADR-0013 defines pack-file publication separately. SDK and CLI independently
verify the Compiled Vault, write a complete deterministic stream to a
synchronized, restrictive sibling temporary file, verify the staged pack, and
then atomically publish it without replacement. Existing regular files,
directories, live or dangling symlinks, and publication-race winners are never
overwritten. Pack destinations must end in `.vaultpack` and be disjoint from
the Compiled Vault in either containment direction. Caught pre-publication
failures leave no vaultc-created file at the requested pack destination.

Compiled Vault publication and pack publication are not one combined
filesystem transaction: a valid Compiled Vault remains if later pack creation
fails. `VaultCompiler::compile_with_options` performs detectable pack preflight
before compilation and then performs these two ordered commits. A runtime pack
failure is wrapped as `PackPublicationAfterCompile` so callers can identify
the retained valid directory. A future true all-or-nothing release requires a
single-root versioned bundle format rather than rollback across unrelated
paths.

If Unix parent-directory synchronization fails after the pack has been
published, the complete pack remains and the SDK returns the distinct
`PublishedButDurabilityUncertain` error. Windows V1 synchronizes the file and
provides process-visible atomic no-replace publication but does not claim a
safe-Rust parent-directory flush or physical power-loss durability.

## VaultPack

A `.vaultpack` is a deterministic `tar.zst` of the Compiled Vault plus
versioned distribution metadata. Archive members are lexically ordered;
owner/group IDs, names, regular-file modes, timestamps, and tar header mode are
normalized. Identical semantic build inputs MUST yield identical bytes under
the declared Rust/zstd toolchain contract. The current `0.1.0` writer packages
the Compiled Vault tree but has no separate distribution-metadata record, so
that part of the V1 format contract remains unimplemented.

The current format is unsigned. Signatures will be detached or embedded only
after a future versioned signing profile defines keys, algorithms, identity,
revocation, and reproducible coverage. Until then, verification establishes
internal checksum, sealed-audit, approval, provenance-graph, and independently
reconstructed output consistency, not publisher authenticity. Once that
profile exists, signature verification MUST
occur before installation, and signatures will not replace content-hash
verification, permission display, license review, or provenance inspection.

Verification of a `.vaultpack` requires both a valid inner Compiled Vault and
byte equality with a package recreated by the exact declared deterministic
tar/zstd writer profile. Equivalent extracted members encoded with different
compression parameters or archive headers are not canonical VaultPacks and
are rejected. Only a canonical outer archive may receive the virtual
`vaultc-tar-zstd-deterministic-v1` package provenance profile.

## Verification

The normative verifier checks archive safety, regular-file-only members, duplicate
members, path policy, canonical manifests, full tree/checksum equality,
recomputed artifact identity, sealed plan integrity, manifest-to-plan linkage,
proposal/conflict approvals, exact standalone audit-file equality, expected
provenance reconstruction, evidence closure, and generated frontmatter.
Verification is independently callable and must not trust a prior successful
compile status.

The current verifier requires its exact schema and compiler version and checks
the implemented internal audit relationships. Every source-derived Copy,
Markdown rewrite, and Canvas rewrite is compared with its sealed output hash.
For rewritten Canvas it reconstructs canonical bytes from the sealed source
value and typed node rewrites. For rewritten Markdown it rederives the ordered
recipe from sealed link resolution, reverses the recipe against output bytes to
reconstruct and hash the original source candidate, and reparses the rewritten
links. Generated notes are reconstructed byte-for-byte from the approved
proposal and its required materialization commitment, including canonical body
and output hashes, destination, operation ID, and ordered EvidenceId values.
V1 rejects arbitrary evidence spans and accepts only file/body evidence or
exact block evidence. It independently reconstructs the complete typed stored
graph, validates RecordIds, edge/cardinality/acyclic/reachability invariants,
retains declared frontmatter author/license values, synthesizes the
non-circular audit envelope from final bytes, and enforces bounded explanation
pages and cursors. It does not infer license compatibility or verify publisher
authenticity.
