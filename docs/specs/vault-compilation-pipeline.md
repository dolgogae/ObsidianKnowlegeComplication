---
title: Vault Compilation Pipeline
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-09-03
decision_refs:
  - ADR-0003
  - ADR-0005
  - ADR-0008
  - ADR-0009
  - ADR-0012
  - ADR-0014
  - ADR-0016
  - ADR-0017
  - ADR-0022
  - ADR-0023
  - ADR-0024
source_refs:
  - HIST-COMPILER-PLAN
---

# Vault Compilation Pipeline

## Schema-3 integration stages

1. Run the existing safe snapshot, parse, exact-duplicate, and link analysis.
2. Seal every Markdown block and frontmatter value into an integration corpus.
3. Run sensitive-data preflight before any provider disclosure and authorize
   each role/cluster against its local or remote boundary.
4. Generate and validate ordered embeddings, then record the complete semantic
   candidate set.
5. Ask the organizer for exactly-one document clustering and safe canonical
   paths; locally validate and obtain one whole-taxonomy human approval.
6. For every cluster, including singletons, obtain typed synthesis sections,
   evidence, dispositions, related links, and contradiction sets.
7. Run a separate critic comparison. `critical` or `major` findings block the
   revision; only `minor` findings may receive curator/rationale-bound waivers.
8. Obtain one exact cluster approval, including every omission approval.
9. Seal taxonomy, proposals, critic reports, approvals, and provider recording
   hashes into `ApprovedIntegrationPlan`.
10. Compile offline into canonical notes and source-path redirect stubs, write
    audit metadata, publish atomically, and independently verify reproduction.

`okc integrate` journals preflight, embedding, local candidates, organizer,
synthesis, and critic tasks and resumes complete task responses. `review
taxonomy` and `review cluster` expose the implemented approval boundary. The
current development slice does not yet implement block-size chunk batching,
HNSW/candidate-union parity, manual amendments,
Canvas/attachment/Base carry-through, or V3 Pack publication. These missing
stages MUST NOT be represented as a stable or complete V3 compiler.

## Frozen schema-2 ordered stages

1. **Open safely:** resolve input kind, establish resource limits, reject unsafe archives and paths.
2. **Enumerate:** traverse with deterministic logical-path ordering and exclusions.
3. **Snapshot:** stream hashes, build manifest, and seal snapshot identity.
4. **Parse:** decode Markdown/frontmatter/Canvas and inventory opaque assets and Bases.
5. **Normalize:** compute comparison forms without replacing source bytes.
6. **Resolve:** build source-local and cross-source link candidates.
7. **Analyze:** group exact duplicates, generate near-duplicate candidates, detect conflicts.
8. **Plan:** allocate source-derived output paths, deterministic copy/rewrite operations, diagnostics, conflicts, and required decisions. Approved generated outputs enter only through the later approval overlay.
9. **Augment optionally:** ask providers for evidence-bound proposals and capture a transcript.
10. **Validate and approve:** reject invalid/stale proposals; validate typed actions against sealed candidates.
11. **Derive materialization:** compose source-span-ordered actions and approved proposals into one immutable effective operation set.
12. **Compile:** reverify source hashes, stage output, materialize effective operations, write audit metadata and checksums.
13. **Publish:** atomically commit the complete sibling staging directory with
    a supported no-replace primitive; never substitute a check followed by a
    replacing rename.
14. **Verify:** independently rederive materialization and validate paths, hashes, manifests, provenance closure, and resolvable rewrites.

Stages are resumable only when their input identity and semantic configuration hash match.

## Source policies

V2 excludes `.obsidian/**`, `.git/**`, known executable file classes, named secret files, and any external symlink traversal. Symlinks are not followed by default. A logical path containing an absolute root, drive prefix, NUL, or `..` traversal is rejected.

Every accepted source entry retains its exact portable UTF-8 spelling before
NFC and its NFC logical path under ADR-0012. Duplicate logical paths within one
source after NFC normalization are rejected. Across sources, exact, case-fold,
and Unicode-normalization collisions are typed from the original request
spellings and resolved only through the sealed deterministic layout rule;
silent last-writer-wins is forbidden. Source rereads must match both sealed
path forms as well as the content identity.

Archive extraction is virtual/streamed when possible. Limits MUST cover compressed bytes, expanded bytes, file count, per-file size, path length, nesting, and compression ratio. The planner does not need to execute or import uploaded plugin JavaScript.

The current scanner retains every accepted entry's bytes in memory before
sealing/parsing and treats nested archives as opaque assets. ZIP and `tar.zst`
containers are bounded before parser construction, all effective members count
toward limits, and the complete `tar.zst` decoder stream is expansion-bounded.
Streaming accepted-file storage and recursive archive policy remain
implementation gaps, not alternate V2 behavior.

## Analysis policy

- Exact duplicates follow ALG-DED-001.
- Near duplicates follow ALG-DED-002 and only create review candidates.
- Link, path, title, alias, and frontmatter collisions become typed conflicts.
- Attachments use the domain-separated
  `ContentHash = SHA-256("okc:content:v2\0" || bytes)` and are deduplicated
  independently of note identity. Artifact checksum entries use raw SHA-256.
- `.base` files are copied to `views/` and receive an `OPAQUE_BASE_UNVALIDATED` diagnostic.

## Draft plan structure

A normative `DraftPlan` contains:

- plan/schema/compiler versions;
- complete ordered snapshot IDs and configuration hash;
- output operations with stable operation IDs;
- input-to-output path map and link rewrite map;
- exact duplicate groups and canonical member selection;
- near-duplicate candidates;
- conflicts, stable conflict content hashes, and required-decision states;
- expected output hashes where computable;
- diagnostics and resource estimates.

Provider proposals are not inserted into `DraftPlan`. Validation records,
approved proposals, the provider transcript, and conflict-decision overlays are
carried by `ApprovedPlan`. Typed actions plus approved proposals derive a
`MaterializationPlan` with action/proposal set hashes, effective operations,
and `MaterializationId`. Plans are immutable values: any plan payload change
yields a new `plan_id`. Proposal approvals bind to the plan and proposal
content hash; conflict decisions bind to the plan, conflict ID, conflict
content hash, curator, policy, and an ADR-0017 typed action.

The current `0.2.0` `DraftPlan` seals schema/compiler version, plan/projection/
inspection identities, policy, snapshots, canonical workspace, Document,
Asset, Canvas, and Base output maps, operations, duplicate reports, conflicts,
resource estimates, and diagnostics. Canvas and Markdown rewrite operations seal their ordered
rewrites and expected output hashes. Markdown planning reopens each affected
source once, validates sealed source slices, applies the recipe, reparses the
result, and discards the bytes after hashing. The artifact verifier reverses
the recipe against output bytes to reconstruct a source candidate and compare
its sealed source hash without embedding raw source copies. Approved generated proposals carry a required tagged
materialization that seals destination, canonical emitted-body hash, complete
rendered-output hash, ordered EvidenceId values, and operation ID. Compilation
and verification independently rebuild that value; pre-materialization
approval files without it fail closed.

## Failure semantics

No partial output may become the requested destination. On failure, the
staging directory may be retained only under an explicit debug option and MUST
be clearly marked incomplete. The default behavior explicitly removes the
exact known staging directory after safe validation; it never recursively
deletes an unresolved path. Failure to mark or remove the exact stage MUST
report the staging path, original failure, requested disposition, and
disposition failure rather than being silently ignored.

The final directory namespace commit follows ADR-0014. Every existing leaf and
race winner is preserved as `OutputExists`; an unsupported no-replace
primitive fails closed without a replacing fallback. Before staging, the
output and optional integrated Pack are required to be disjoint from every
immutable source path under resolved-ancestor, lexical, NFC, and full-case-fold
comparison. A
post-publication parent-sync failure retains the verified directory and reports
`PublishedButDurabilityUncertain`; optional Pack publication does not begin.

Warnings may permit compilation if policy allows. Errors prevent approval or
publication. Hard ingestion and safety resource limits are errors.
ALG-DED-002's bounded per-document candidate cap may truncate only
near-duplicate candidate generation and MUST emit an explicit diagnostic; it
is not an ingestion or compilation fallback.

## Determinism

All traversal, candidate, diagnostic, manifest, JSON line, and archive member
orderings are explicit. Locale, wall clock, random process seed, hostname,
absolute input path, filesystem inode, and thread scheduling MUST NOT affect
semantic identities or emitted Compiled Vault/OKCPack bytes. Runtime source
locators may appear only in non-semantic build control state and MUST be
redacted from artifacts. Parallel processing may be used only with
deterministic collection and reduction.
