---
title: Vault Compilation Pipeline
status: normative
owners:
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0005
  - ADR-0008
  - ADR-0012
  - ADR-0014
  - ADR-0016
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0027
source_refs:
  - HIST-COMPILER-PLAN
---

# Vault Compilation Pipeline

## Current ordered stages

1. Validate typed source descriptors and resource limits.
2. Enumerate directories or archives in deterministic logical-path order,
   excluding forbidden/private paths and refusing unsafe members.
3. Hash immutable bytes, seal snapshots, parse Markdown/frontmatter and safety
   metadata, and reject duplicate whole-Vault content.
4. Run the private deterministic analysis path and seal an
   `IntegrationCorpus` through `CorpusBuilder::build`.
5. Scan content before disclosure and authorize each role/cluster against its
   local or remote boundary.
6. Record and validate embeddings/candidates; obtain a complete taxonomy
   proposal and explicit taxonomy approval.
7. For every cluster, including a singleton, obtain synthesis sections,
   evidence, dispositions, related links, and contradictions.
8. Run a separate critic. Critical/major findings block; minor findings need
   exact curator waivers.
9. Obtain cluster approvals, including every omission decision, and seal all
   source, taxonomy, proposal, critic, approval, and recording hashes into one
   `ApprovedIntegrationPlan`.
10. Compile without a provider into a sibling staging directory, emit
    canonical Markdown and redirect stubs plus audit files, independently
    verify the stage, and publish the absent destination without replacement.
11. Verify or explain a published directory by reproducing plan-derived bytes
    and exact provenance.

Stages resume only when every input and semantic configuration identity
matches. Project services append task state and immutable objects; they never
rewrite prior approval authority.

Changing language or routes after downstream work exists starts a fresh active
run without deleting history. Changing approved taxonomy or requesting a newer
cluster revision makes cached plans unavailable immediately. A pending
regeneration MUST NOT be resealed using its older approval. Latest verified
output is bound to the current approved plan, and compile/verify hold the project
writer lock through recording the result.

Source/configuration changes commit journal invalidation before replacing the
manifest; in-memory configuration changes only after the manifest write succeeds.
A failed journal commit leaves the manifest unchanged. A later manifest failure
may leave the old configuration with a fresh empty run, requiring integration
again, but MUST NOT restore old approvals as current. This conservative ordering
is not an atomic transaction across SQLite and the filesystem, nor automatic
manifest recovery after a power loss.

## Source safety

The scanner excludes `.obsidian/**`, `.git/**`, configured executable and
secret classes, and every symlink. It rejects absolute roots, drive prefixes,
NUL, parent traversal, non-portable/reserved names, duplicate NFC logical
paths, unsupported source kinds, and lossy archive names.
Source membership comes only from the sealed inclusion/exclusion policy.
Ambient parent/global ignore files and source `.ignore` directives MUST NOT
silently exclude Markdown or alter snapshot identities.

Linux/macOS pin the source directory and resolve each member component with
descriptor-relative no-follow opens, checking regular-file type on the opened
handle. Source-root and archive-leaf symlinks are rejected. This protects source
content reads; it does not qualify path-based enumeration, managed SQLite
mutation, output ancestors, or Windows reparse-point races.

Archive bounds cover compressed and expanded bytes, file count, per-file size,
path length/depth, and expansion ratio. ZIP and `tar.zst` members are validated
before parser use. Nested archives remain opaque. Accepted entries are still
buffered under bounds, so streaming accepted-file storage remains required for
the full 20 GB gate.

The default policy serialization, inspection order, source identity, parser
behavior, corpus sealing, and optional build-workspace SQLite writes are part
of current Schema 3 reproducibility. Their retained historical version
literals MUST NOT be cosmetically rewritten when that would alter bytes.

## Semantic validation

- Taxonomy coverage is exactly once over every corpus document.
- Disposition coverage is exactly once over every block and metadata value.
- Evidence belongs to the current cluster and content hash.
- Integrated content has output evidence; preserved content remains visible;
  omission requires exact approval.
- Contradictions contain independently evidenced sides.
- Critic input covers the complete source inventory and current proposal.
- Approval targets and policy/curator bindings are exact and current.
- Every provider recording named by the plan is present and hash-matched.

Provider output is never inserted directly into output and never supplies
approval. Missing, duplicate, stale, malformed, foreign, or over-limit records
refuse plan sealing or publication.

## Materialization and failure semantics

The current materializer writes:

- `knowledge/<canonical-path>.md`;
- `legacy/<source-id>/<original-path>.md` redirect stubs;
- `.okc/integration-plan.json`;
- `.okc/provenance.jsonl`;
- `.okc/manifest.json`;
- `.okc/checksums.txt`.

The `legacy/` name is a current output contract, not a compatibility reader.
Attachment, Canvas, Base, full link rewriting, and OKCPack publication are not
implemented and MUST NOT appear as successful output capabilities.

The destination must be absent. Staging uses a known sibling path, writes new
files only, verifies before publication, and uses an atomic no-replace
primitive. A race winner is preserved. Pre-publication failure exposes no
requested destination. A parent synchronization failure after publication
retains the verified directory and reports
`PublishedButDurabilityUncertain`.

## Determinism

Traversal, documents, blocks, metadata, candidates, diagnostics, manifest
files, JSON records, checksums, and materialized paths use explicit order.
Locale, wall clock, random process seed, hostname, source absolute root,
filesystem inode, input order, and thread scheduling MUST NOT affect semantic
IDs or output bytes. Parallel work is permitted only with deterministic
collection and reduction.
