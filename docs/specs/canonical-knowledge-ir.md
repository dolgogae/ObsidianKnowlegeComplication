---
title: Canonical Knowledge Intermediate Representation
status: normative
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0008
  - ADR-0012
  - ADR-0016
  - ADR-0022
  - ADR-0024
  - ADR-0027
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Canonical Knowledge Intermediate Representation

## Purpose

Canonical IR separates hostile source syntax from semantic and output policy.
Original bytes remain owned by immutable snapshots. Parsed comparison text,
frontmatter values, exact source spans, and domain-separated identities are
sealed into a deterministic corpus; raw source copies MUST NOT enter a plan or
Compiled Vault audit directory.

## Identity hierarchy

```text
SourceId
└── SnapshotId
    └── SourceFileId
        ├── DocumentId
        │   ├── SectionId
        │   └── BlockId
        ├── CanvasId
        ├── BaseArtifactId
        └── AssetId
```

Display names and absolute source paths are not identities. Existing stored
Schema 3 identities and `okc:*:v3\0` domains MUST remain unchanged. The
retained private snapshot/parser stages keep their established domains where
changing them would alter the resulting Schema 3 corpus.

## Source records

- `SourceSpec` is a typed directory, ZIP, or `tar.zst`/`.tzst` descriptor with
  a stable `SourceId` and optional display-only owner.
- Each accepted file retains exact portable UTF-8 spelling, NFC logical path,
  kind, byte length, raw SHA-256, content hash, and source identity.
- Filesystem time, inode, hostname, absolute root, input order, and MCP origin
  MUST NOT affect semantic identity or output.
- Source symlinks are not followed. Lossy path decoding is forbidden.
- `SourceId` construction and deserialization enforce the same bounded ASCII
  identifier grammar; `.` and `..` are forbidden. Unknown `SourceSpec` fields
  fail instead of being silently ignored.

## Parsed records

The private parser represents Markdown documents, headings, sections, blocks,
frontmatter, wikilinks, embeds, ordinary links, tags, callouts, code, and math
with source byte spans and comparison forms. Original and normalized forms are
distinct. Frontmatter has an original span for preservation and typed values
for policy.

Canvas nodes, edges, file references, and unknown JSON fields remain typed in
the parser. Base files remain opaque. These records support hostile-input
validation but do not imply current Canvas/Base materialization.

## Integration corpus

`CorpusBuilder::build` returns `PreparedCorpus` containing a sealed
`IntegrationCorpus`, stable block text map, and source count. The corpus
contains every Markdown document with:

- source and document identities, original relative path, and document hash;
- ordered `SourceBlock` records with block ID, content hash, and retained text;
- individual `MetadataValue` records with key, value index, typed value, and
  content hash.

Corpus documents, blocks, and metadata are sorted by explicit keys before the
corpus hash is sealed. Source order MUST NOT affect the value.

Each document has at most one metadata value for a given `(key, value_index)`;
different hashes do not make conflicting values in one slot valid. Metadata
JSON keys are source data: language DTO name conversion MUST NOT rewrite them.

## Integration records

- `TaxonomyProposal` and `TaxonomyCluster` assign every document exactly once.
- `SynthesisSection` cites exact ordered `SectionEvidence`.
- `SourceDisposition` gives every block and metadata value one of
  `integrated`, `preserved_verbatim`, or `omission_proposed`.
- `RelatedLink` and `ContradictionSet` retain cross-topic and conflicting
  evidence without inventing a winner.
- `CriticReport`, `OmissionApproval`, `FindingWaiver`, `ClusterApproval`, and
  `ApprovedClusterRevision` bind explicit review authority.
- `ApprovedIntegrationPlan` is the complete offline compilation input.

Section bodies permit LF and tab for ordinary multiline Markdown while
rejecting other control characters and enforcing the 16 MiB body bound.
Headings, IDs, and paths retain their separate stricter control rules.

Integrated blocks MUST be cited by a section or contradiction. Preserved
content MUST remain materialized. An omission has no effect without exact
curator-bound approval. Critical or major critic findings block approval; a
minor finding requires a hash-bound waiver.

## Evolution

Current readers accept only Schema 3 project and artifact data. Unknown or
mixed schemas fail closed. Recognizable Schema 1 or Schema 2 artifacts are
identified only far enough to return `ARTIFACT_SCHEMA_UNSUPPORTED`; they are
not migrated or decoded into current IR. Any future schema change requires an
ADR, explicit migration policy, new golden vectors, and traceability updates.

## Invariants

1. Every current IR item resolves to one immutable source snapshot.
2. Every source span is within the hashed source bytes and uses byte offsets.
3. Ordered collections have declared keys; map iteration never affects bytes.
4. Original and normalized values remain distinct.
5. Lossy text or path decoding is forbidden.
6. Duplicate logical paths and ambiguous duplicate Canvas node IDs fail.
7. Every Markdown document belongs to exactly one taxonomy cluster.
8. Every source block and frontmatter value has exactly one disposition.
9. Every generated section and contradiction claim has locally validated,
   cluster-owned evidence.
10. Experimental claim/memory records remain outside the default compiler.
