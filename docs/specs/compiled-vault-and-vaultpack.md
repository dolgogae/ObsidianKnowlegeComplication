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

## Manifest

The manifest includes format/schema/compiler versions, artifact ID, ordered source snapshot IDs, plan ID, configuration hash, approved proposal hashes, output file inventory, media types, sizes, hashes, license/attribution summaries, creation policy, and optional signature metadata. Wall-clock creation time is informational and excluded from reproducibility identity.

## Checksums

`checksums.txt` uses a specified UTF-8 line format, sorted normalized path, SHA-256 hex, two spaces, then escaped path. It covers every artifact file except the checksum file itself and detached signatures as defined by format version.

## Atomic materialization

The compiler verifies sources and destination parent, creates a unique sibling staging directory with restrictive permissions, writes files without following symlinks, fsyncs according to platform policy, verifies the staged tree, then renames it to the requested absent destination. Existing output is rejected unless a separately specified, recoverable update workflow is introduced by ADR.

## VaultPack

A `.vaultpack` is a deterministic `tar.zst` of the Compiled Vault plus distribution metadata. Archive members are lexically ordered; owner/group IDs, names, modes, timestamps, PAX fields, compression settings, and zstd version policy are normalized. Identical semantic build inputs MUST yield identical bytes under the supported toolchain contract.

Signatures are detached or embedded according to a future versioned signing profile. Signature verification MUST occur before installation, and signatures do not replace content-hash verification, permission display, license review, or provenance inspection.

## Verification

Verification checks archive safety, manifest/schema compatibility, duplicate members, path policy, hashes, plan linkage, approval linkage, provenance closure, generated frontmatter, source attribution, unresolved required conflicts, and internal link rewrites. Verification is independently callable and must not trust a prior successful compile status.
