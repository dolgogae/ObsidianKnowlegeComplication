---
title: Compiled Vault and VaultPack Format
status: normative-v1
owners:
  - core-rust-engineer
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0009
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

Conflict suffixes are stable and derived from a short, collision-checked identity fragment, never traversal-prone user text. Sanitization produces a diagnostic and retains the original logical path in provenance.

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

The target `vaultc_sources` entries are ALG-PRV-001 `EvidenceId` values. The
current writer instead emits each evidence `document_id`, and
`vaultc_pack_id` is always `null`; both are implementation gaps rather than a
change to the target format.

## Manifest

The manifest includes format/schema/compiler versions, artifact ID, ordered source snapshot IDs, plan ID, configuration hash, approved proposal hashes, output file inventory, media types, sizes, hashes, license/attribution summaries, creation policy, and optional signature metadata. Wall-clock creation time is informational and excluded from reproducibility identity.

The `0.1.0` manifest currently contains `schema_version`, `compiler_version`,
`artifact_id`, `plan_id`, `policy_hash`, `projection_hash`, ordered source
snapshot IDs, approved proposal hashes, and
`files[{path, byte_len, sha256}]`. Its inventory excludes `manifest.json` and
`checksums.txt`; it does not yet carry media types, license/attribution,
creation/distribution policy, or signature metadata.

## Checksums

Each `checksums.txt` line is `<64 lowercase raw-SHA-256 hex><two ASCII
spaces><validated normalized UTF-8 logical path>\n`, sorted by logical path.
Paths are literal rather than escaped because V1 path policy forbids control
characters, backslashes, and non-canonical forms. The file covers the manifest
and every other artifact file except itself and detached signatures as defined
by format version.

## Atomic materialization

The compiler verifies sources and destination parent, creates a unique sibling staging directory with restrictive permissions, writes files without following symlinks, fsyncs according to platform policy, verifies the staged tree, then renames it to the requested absent destination. Existing output is rejected unless a separately specified, recoverable update workflow is introduced by ADR.

The `0.1.0` implementation performs an existence check immediately before the
rename, but a portable atomic no-replace directory primitive has not been
implemented. Closing that check/rename race on every supported platform is a
release blocker for the no-clobber portion of REQ-CMP-001.

When the CLI is also asked for a pack, Compiled Vault publication and pack
publication are not one combined filesystem transaction: a valid Compiled
Vault may remain if later pack creation fails. The CLI pack wrapper stages and
no-clobber-publishes the pack file. The public SDK `pack::create_pack` currently
writes directly to its absent destination and can leave a partial pack on I/O
failure. Callers needing an all-or-nothing release need a future hardened SDK
pack/release-bundle API.

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
internal checksum, sealed-audit, approval, and implemented provenance-linkage
consistency, not publisher authenticity or full independent reconstruction of
every output operation. Once that profile exists, signature verification MUST
occur before installation, and signatures will not replace content-hash
verification, permission display, license review, or provenance inspection.

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
links. It does not yet perform equivalent sealed reconstruction for generated
frontmatter/body/source IDs, rehash arbitrary evidence-span bytes, validate
license/attribution, or verify authenticity. Verification therefore establishes
strong source-derived operation integrity but not complete derivation
correctness or publisher authenticity for every output class.
