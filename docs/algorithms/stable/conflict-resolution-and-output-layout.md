---
title: ALG-CNF-001 — Conflict Resolution and Output Layout
status: normative
owners:
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0006
  - ADR-0012
  - ADR-0024
  - ADR-0027
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-CNF-001: Conflict Resolution and Output Layout

## Objective and current scope

Allocate portable paths and validate source references in the retained private
analysis beneath `CorpusBuilder`. Its `DraftPlan`, path maps, operations, and
conflicts are intermediate safety data. They do not approve or publish a
current artifact. `ApprovedIntegrationPlan` alone authorizes current
materialization under ALG-INT-001.

The former public decision-overlay and `MaterializationId` workflow is
historical in [`ADR-0017`](../../adr/0017-typed-conflict-actions-and-materialization.md).
Private attachment/Canvas/Base analysis does not imply current output support.
This algorithm cannot choose truth or automatically approve a semantic merge.

## Inputs, outputs, and ordering

Input is ordered canonical IR, exact duplicate groups, conflicts, and the
retained portable path policy. Output is the sealed internal Draft Plan with
path maps, operations, candidate sets, and diagnostics.

```text
T(d) = (N_path(preferred_path(d)), SourceId(d), SnapshotId(d), DocumentId(d))
suffix(d) = "~" || first_collision_free_prefix(base32(DocumentId(d)), q)
```

`d` is a retained document. `N_path` is the pinned NFC path normalization from
ALG-NRM-001. Sort text by unsigned UTF-8 bytes and IDs by raw bytes. Start the
visible Base32 prefix length `q` at 8 and extend deterministically up to 52.
The minimum `T` selects an exact-group representative while retaining every
source origin. Distinct colliding documents receive stable suffixed paths.

## Private allocation

```text
collapse exact groups for private allocation, retaining all member origins
for each representative in T order:
    validate the preferred path with the portable component policy
    compare NFC/full-Unicode-case-fold collision keys
    allocate the requested path or a deterministic identity suffix
group assets by SHA-256 and allocate hash-derived attachment paths
allocate safe Canvas and Base paths
resolve references against the complete source namespace
record every ambiguous candidate set; never guess by discovery order
validate rewritten spans, containment, paths, and identities
seal the internal DraftPlan
project all documents, blocks, and metadata into the integration corpus
```

Private path maps reserve `knowledge/`, `attachments/`, `canvases/`, and
`views/` namespaces. They are not the final current taxonomy. Exact-duplicate
analysis does not remove documents from the Schema 3 corpus or exempt them
from synthesis, critic, and curator approval.

## Current output allocation

The approved taxonomy supplies one canonical Markdown path below `knowledge/`
per cluster. Every source document supplies a redirect below
`legacy/<source-id>/<original-path>`. Both paths must pass the same portable
component rules as source paths. Original source spelling is retained; NFC and
full case folding are comparison keys, not a rewrite of stored source names.
Exact, Unicode/case-fold, and file-versus-directory collisions must fail before
publication, including inconsistent spellings of a shared directory prefix.

Taxonomy edits create a new whole-taxonomy hash and approval. They invalidate
dependent proposals, critics, and cluster approvals. No internal Draft Plan
or historical conflict action supplies current compilation authority.

## Complexity and edge cases

For `D` documents and `L` references, private allocation requires
`O(D log D)` ordering and `O(L + total bytes)` rewriting plus map lookups.
Current output-path validation uses bounded component keys and ordered sets.

Reject absolute/traversing paths, empty or dot components, reserved Windows
names, trailing dots/spaces, unsafe delimiters, overlong components/paths,
symlinks, and containment escapes. Full Unicode folding is required even on
case-sensitive hosts. A filename cannot also be an ancestor directory.
Unrepresentable source-reference rewrites must fail rather than invent escapes.

## Examples and regression vectors

Private analysis of distinct `Topic.md` documents selects one unsuffixed path
and one identity-suffixed path independently of input order. Final current
notes instead use explicitly approved taxonomy paths.

| Case | Expected |
|---|---|
| identical body and metadata in two documents | private exact group; both remain in the integration corpus |
| `Straße.md` and `STRASSE.md` | portable collision |
| NFC and NFD spellings of the same output | normalization collision |
| `Topic.md` and `Topic.md/Child.md` | file/directory collision |
| ambiguous source link | complete typed candidate set; no guessed winner |
| source order reversed | identical private analysis and sealed corpus |

## Correctness and rollback

Path allocation must be total, deterministic, portable, and contained. Invalid
identities, unsafe paths, or failed reparses stop corpus construction. Invalid
taxonomy paths or current approval closure stop publication. No last-writer
wins policy or unapproved semantic fallback is permitted.
