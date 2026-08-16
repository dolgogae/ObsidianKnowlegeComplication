---
title: Canonical Knowledge Intermediate Representation
status: normative-v1
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0008
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Canonical Knowledge Intermediate Representation

## Purpose

The canonical IR separates source syntax from output policy. It is versioned, serializable, deterministic, and sufficient to re-plan without reparsing unchanged files. Original bytes and exact source spans remain available for lossless copying and targeted rewriting.

## Identity hierarchy

```text
SourceId
└── SnapshotId
    ├── SourceFileId
    │   ├── DocumentId
    │   │   ├── SectionId
    │   │   │   └── BlockId
    │   │   └── LinkId / EvidenceRef
    │   ├── CanvasId
    │   ├── BaseArtifactId
    │   └── AssetId
    └── Manifest identity
```

IDs MUST be domain-separated hashes as specified by [`../algorithms/stable/snapshot-identity-and-hashing.md`](../algorithms/stable/snapshot-identity-and-hashing.md). Display names never serve as identity.

## Core records

### Source and snapshot

- `SourceDescriptor`: stable source ID, user label, input kind, policy overrides.
- `VaultSnapshot`: schema version, snapshot ID, source ID, creation observation, ordered file manifest, exclusion report.
- `SourceFile`: normalized logical path, raw-path encoding metadata, media type, byte length, SHA-256, safety classification.

Timestamps from the input filesystem MAY be preserved as informational metadata but MUST NOT influence deterministic identity or output bytes.

### Knowledge content

- `Document`: file identity, source bytes/hash, decoded text policy, frontmatter, title/aliases/tags, ordered sections, syntax spans, outbound links.
- `Section`: heading level/path, source span, ordered blocks.
- `Block`: stable block ID, block kind, source span, raw slice hash, comparison form.
- `Link`: syntax kind, raw target, parsed path/heading/block components, display text, embed flag, resolution state.
- `Asset`: media type, byte hash, size, original logical paths.
- `Canvas`: typed nodes/edges plus preserved unknown JSON fields and source file references.
- `BaseArtifact`: opaque bytes and path only; no inferred semantics in V1.
- `EvidenceRef`: snapshot ID, document ID, optional block ID/span, and content hash.

### Future semantic records

`Entity`, `Claim`, `Relationship`, `Topic`, and `SourceEvidence` are `normative-future`. They may be emitted by experimental packages but MUST NOT be required for deterministic V1 file compilation. A `Claim` cannot exist without one or more `EvidenceRef` records.

## Parsing and preservation

The current implementation pins Comrak `0.48` for a normalized CommonMark
structural projection and supplements it with a custom Obsidian-aware byte-span
scanner for blocks, wikilinks, embeds, and ordinary Markdown links. Parser or
Unicode-library upgrades are semantic changes and require frozen golden-vector
review.

Frontmatter is represented twice:

1. original byte span for preservation;
2. normalized typed map for comparison and policy.

Mapping key order and formatting MUST NOT be destroyed when a file is copied unchanged. When generated frontmatter is emitted, canonical field order and scalar rules from the output specification apply.

## Link resolution

Resolution produces zero, one, or many candidates and records the reason. It MUST account for explicit relative paths, Vault-root-like paths, filename stems, headings, block IDs, aliases, case behavior, and source-local namespace. Ambiguity is a conflict; the compiler must not guess based on host filesystem ordering.

The `0.1.0` implementation applies that resolver to Markdown and attachment
links. Canvas file nodes are decoded into `CanvasFileReference` values and the
complete unknown-preserving JSON value is retained, but their
`resolved_document` field is not populated and their paths are copied without
rewrite. REQ-PAR-002 remains incomplete until Canvas uses the same resolution,
conflict, rewrite, provenance, and verifier boundary.

## Schema evolution

Every serialized IR document includes `schema_version`. Readers must support
explicitly listed older versions through pure migrations and reject unknown
newer major versions. Migration MUST preserve IDs and provenance unless the
version notes define an intentional identity break through an ADR. The current
`0.1.0` reader accepts only schema version 1; no older-version migrations are
implemented yet.

## Invariants

1. Every IR item resolves to exactly one immutable snapshot.
2. Every source span lies within the hashed source bytes and uses byte offsets.
3. Ordered collections have defined sort keys; map iteration order never affects output.
4. Original and normalized forms are distinct fields.
5. Invalid UTF-8 is either rejected or represented under an explicit binary policy; lossy decoding is forbidden.
6. Unknown Canvas fields are preserved through round trips.
