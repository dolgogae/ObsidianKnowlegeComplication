---
title: Open Questions
status: normative-future
owners:
  - architect
last_updated: 2026-09-02
decision_refs:
  - ADR-0002
  - ADR-0004
  - ADR-0009
  - ADR-0010
  - ADR-0011
  - ADR-0013
  - ADR-0014
  - ADR-0017
  - ADR-0019
  - ADR-0020
  - ADR-0021
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Open Questions

## Resolved documentation conflict — updater runtime boundary (2026-09-02)

ADR-0019 originally said “Tokio is not introduced,” while ADR-0020 required
the blocking API of `axoupdater 0.10.0`, which internally creates a private
current-thread Tokio runtime. ADR-0021 resolves the conflict narrowly: no
Tokio application/TUI/worker architecture is permitted, but the updater's
library-owned runtime may exist only for an explicit receipt-aware update
operation. A literal dependency-graph exclusion is no longer claimed.

These questions are not permission to improvise defaults. Resolve a
behavior-changing answer through an ADR and specification update. Current
implementation gaps are also summarized in
[`../CURRENT_STATE.md`](../CURRENT_STATE.md).

## Stable V2 implementation

- Freeze the complete Comrak/Unicode/normalization compatibility contract and
  golden vectors, including Unix raw filenames, invalid UTF-8, NFC/NFD, and
  every supported filesystem.
- Complete the remaining Markdown/Canvas golden corpus required by ALG-NRM-001,
  including broader Unicode/escaping, self-reference, mixed-target ambiguity,
  and supported-filesystem vectors.
- Freeze full MinHash seed/candidate vectors and short-document behavior beyond
  the implemented threshold arithmetic test.
- Decide whether a future schema may add typed actions beyond V2's sealed
  Markdown/Canvas link-target selection. Path mappings and semantic merges are
  intentionally not V2 curator actions.
- Move typed-provenance explanation indexing from the bounded in-memory V2
  implementation to a bounded on-disk/streaming representation before the
  large-Vault performance gate can pass.
- Decide which trust anchor authenticates a wholly resealed but internally
  consistent unsigned artifact. Source-derived Copy, Markdown, Canvas, and
  approved generated-note outputs now carry and verify sealed commitments.
- Close descriptor-relative/no-follow source opens and pin output/staging
  ancestors against replacement races. ADR-0014 specifies final-leaf atomic
  no-replace directory publication; ancestor handle pinning remains separate.
- Define whether nested archives remain opaque permanently or gain a separately
  bounded recursive-inspection profile. Current source and OKCPack container,
  every-member, aggregate, and expansion-ratio accounting is complete for the
  non-recursive V2 boundary.
- Complete crash-resume checkpoints and orphan cleanup above the implemented
  explicit SQLite schema-2 migration. Project encryption is explicitly out of
  scope; plaintext warnings and private permissions remain mandatory.
- Define the deterministic tar/zstd compatibility contract across compressor,
  library, architecture, and operating-system versions.
- Define compatibility beyond the frozen V1 read-only verify/explain reader
  and source-relink V1-to-V2 reconstruction workflow.
- Define the signature profile and key lifecycle before marketplace
  distribution.

## AI and knowledge semantics

- Decide whether remote hosted LLM APIs are allowed under the “no public
  cloud” constraint, how users declare data residency, and what retention/log
  promises a provider must make.
- Define selective block disclosure; the current CLI sends every parsed block
  of each explicitly selected document.
- Define transcript secret handling and retention beyond current block-text
  redaction while preserving replay commitments.
- Define canonical ontology/entity resolution, claim negation,
  temporal/context semantics, human conflict adjudication, authority
  calibration, and dependency clustering for evidence.

## Benchmark and experimental models

- Define topic taxonomy, assignment governance, cohort minimum size, and fair
  percentile policy.
- Define ground-truth construction and bias/leakage controls for LLM-generated
  cases.
- Calibrate coefficients/normalization for activation, graph diffusion,
  consolidation, retrieval, routing, and merge scoring. Current numeric values
  are experiment examples only.
- Define promotion thresholds and minimum practical effects by task.

## Pack, plugin, registry, and release governance

- Define pack distribution metadata, dependencies, updates, uninstall after
  user edits, namespace collision, and reproducible rebuild compatibility.
- Define publisher identity, licensing/consent, moderation, takedown/deletion,
  revocation, and already-derived pack policy.
- Define backup/HA/DR and scheduling of isolated jobs for the on-premise
  registry.
- Define scale thresholds and the orchestrator choice after single/multi-host
  Compose becomes insufficient.
- Define OSS MCP compatibility, licensing, version pinning, and fallback
  maintenance.
- Finish protected native signing/notarization automation, vulnerability and
  license policy, and clean-room evidence before the first public release.

## Closed on 2026-08-16

The following questions from the documentation baseline are now answered for
schema/API version 1. Reopening one requires an ADR or versioned compatibility
change.

- The historical V1 public crates were `vaultc`, `vaultc-protocol`, and
  `vaultc-cli`; ADR-0019 and ADR-0018 supersede that package/executable layout
  for V2 while retaining read-only compatibility.
- Exit codes are frozen as `0`, `2`, `3`, `4`, `5`, `6`, `7`, and `70` with
  the families specified in
  [`../specs/public-sdk-and-cli.md`](../specs/public-sdk-and-cli.md).
- V2 proposal kinds are `create_generated_note` and `explain_conflict`.
- The default near-duplicate configuration is 128 MinHash components in 32×4
  bands, threshold `0.85`, and at most 100 candidates per document; the full
  golden-vector question above remains open.
- Historical V1 default safety limits were 10 sources, 250,000 files, 40 GiB total input, 2
  GiB per file, 16 MiB structured text, 100× archive expansion, 1,024-byte
  logical paths, and 240-byte components.
- SQLite uses WAL, foreign keys, `FULL` synchronous mode, schema
  `user_version = 2`, deterministic transaction order, and Unix mode `0600`.
  Migration/resume/encryption remain open.
- Conflict approvals are immutable plan/content-hash-bound overlays. V2 adds
  sealed Markdown/Canvas target actions through ADR-0017 without changing V1
  waiver interpretation.
- V2 evidence is either file/body-level with no block/span, or block-level with
  the exact block content hash and an omitted or exact block span. Arbitrary
  byte spans are rejected. `DocumentProjection.snapshot_id` makes that evidence
  constructible by a stateless provider and is revalidated during approval.
- A documentation defect found during REQ-PAR-002 work named
  `BaseArtifactId` in the canonical IR but omitted its stable formula.
  ALG-SNP-001 now defines Document, Canvas, and Base file-level identities with
  separate domains over the sealed `SnapshotId` and `SourceFileId`; the V1 Base
  formula is `H("vaultc:base:v1\0" || lp(raw(SnapshotId)) ||
  lp(raw(SourceFileId)))`.
- Canvas file references resolve through sealed source-local indexes to typed
  Document, Asset, Canvas, or Base targets. Zero candidates preserve a
  root-contained raw path with a diagnostic; multiple candidates create a
  node-scoped required conflict and V2 requires either one exact sealed target
  selection or an explicit preserve-original waiver. Rewritten Canvas seals an expected output hash and is
  independently reconstructed; unchanged Canvas remains byte-identical.
- A same-precedence documentation ambiguity said `Document` and
  `BaseArtifact` contained source bytes, while the architecture contract and
  ADR-0006 assign original-byte ownership to immutable source snapshots and
  forbid raw copies in Compiled Vault audit data. The canonical IR now states
  that these records retain byte identities, spans, and source references only;
  planning/compilation reopens the snapshot when exact bytes are required.
- Markdown rewrite operations seal their source identity, ordered link-target
  replacements, and exact expected output hash. Planning reads each affected
  source once and discards bytes after hashing; compilation independently
  reapplies the recipe; verification checks the output commitment, reverses
  replacements to a source candidate, compares its sealed source hash, and
  reparses link semantics. Copy outputs now use the same commitment boundary.
- Pre-output-hash development plans have no compatibility default: the required
  field is absent, so SDK/CLI decoding fails closed. CLI planning classifies
  input/policy/decision/invariant failures as `3`/`2`/`4`/`70`, and `approve`
  classifies a malformed old plan as decision exit `4`.
- EvidenceId uses fixed-width raw snapshot/document/content hashes plus an
  explicit file, block-without-span, or block-with-exact-span mode byte; exact
  spans use unsigned 64-bit big-endian offsets. Generated-note approvals carry
  a required tagged materialization with destination, canonical body/output
  hashes, proposal-order EvidenceId values, and operation ID. Compile and
  verify reconstruct the exact note; older development approvals without this
  field fail closed.
- The V1 `.vaultpack` is deterministic but unsigned. The lower-precedence
  project-context phrase that called it signed conflicted with the output
  specification; it was corrected in favor of the normative future signing
  profile.
- ADR-0010 defines the complete typed provenance graph, `record_` identity and
  record order, dependent-to-prerequisite edge matrix, author/license
  declaration retention, and bounded cursor-bound explanation. Content plus
  plan/conflict/diagnostic/transcript outputs live in the stored graph.
  Provenance/manifest/checksums and an explicitly queried OKCPack package
  subject are deterministic virtual records constructed after final bytes
  exist, eliminating self-hash fixed points. Legacy flat development ledgers
  fail closed.
- ADR-0011 assigned V1 projection construction, provider exchange recording,
  canonical augmentation JSONL, redacted-request hydration, and offline replay
  to `vaultc::augmentation`. Live remote disclosure requires sealed policy and
  per-call consent; offline replay never calls a provider and needs no new
  consent. Non-empty validations require the exact four-record transcript, and
  canonical schema-1 recordings replay byte-for-byte. V2 retains the contract
  under schema 2 and `okc_core::augmentation`.
- ADR-0013 connects `CompileOptions.create_pack`, assigns verified atomic
  no-replace Pack publication to one SDK/CLI implementation, and distinguishes
  pre-commit absence from a complete post-commit publication with uncertain
  parent durability. Compiled Vault and Pack remain two honest ordered commits.
