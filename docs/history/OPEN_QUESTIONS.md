---
title: Open Questions
status: normative-future
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
  - ADR-0004
  - ADR-0009
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Open Questions

These questions are not permission to improvise defaults. Resolve a
behavior-changing answer through an ADR and specification update. Current
implementation gaps are also summarized in
[`../CURRENT_STATE.md`](../CURRENT_STATE.md).

## Stable V1 implementation

- Freeze the complete Comrak/Unicode/normalization compatibility contract and
  golden vectors, including Unix raw filenames, invalid UTF-8, NFC/NFD, and
  every supported filesystem.
- Complete the remaining Markdown/Canvas golden corpus required by ALG-NRM-001,
  including broader Unicode/escaping, self-reference, mixed-target ambiguity,
  and supported-filesystem vectors.
- Freeze full MinHash seed/candidate vectors and short-document behavior beyond
  the implemented threshold arithmetic test.
- Decide the public SDK augmentation surface: add a plan-to-projection request
  builder and/or `VaultCompiler::augment`, define transcript capture for
  in-process `KnowledgeAugmentor`, and remove or connect unused
  `CompileOptions`.
- Define typed conflict actions for selected link targets, path mappings, and
  other real `user_resolved` operations. Until then, ADR-0009 permits only an
  explicit policy waiver overlay.
- Complete ALG-PRV-001 typed `source/operation/decision/proposal/approval/output/edge`
  records, canonical record/evidence IDs, administrative-file closure, and
  author/license attribution. The current flat output ledger is only a partial
  projection and must not redefine the stable algorithm.
- Define the non-circular provenance boundary for self-referential
  `manifest.json`, `provenance.jsonl`, and `checksums.txt` administrative files,
  including which plan/config/toolchain record produces each one.
- Decide which trust anchor authenticates a wholly resealed but internally
  consistent unsigned artifact. Source-derived Copy, Markdown, Canvas, and
  approved generated-note outputs now carry and verify sealed commitments.
- Close filesystem race hardening: descriptor-relative/no-follow source opens
  and a portable atomic no-clobber directory publication primitive.
- Complete archive accounting for compressed bytes, excluded/duplicate member
  counts, nesting, and outer VaultPack zstd expansion ratio.
- Define SQLite migrations, reload/resume/cleanup, batching, and workspace
  encryption policy; the current schema is reset-and-write-only version 1.
- Define the deterministic tar/zstd compatibility contract across compressor,
  library, architecture, and operating-system versions.
- Define schema migration/version compatibility for IR, plan, protocol,
  manifest, decisions, transcript, and provenance records.
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
- Replace the placeholder repository URL and add CI, vulnerability/license
  policy, SBOM, build provenance, release archives/checksums, and clean-room
  reproducibility before the first public release.

## Closed on 2026-08-16

The following questions from the documentation baseline are now answered for
schema/API version 1. Reopening one requires an ADR or versioned compatibility
change.

- Public crates are `vaultc`, `vaultc-protocol`, and `vaultc-cli`; the CLI
  commands are `inspect`, `plan`, `augment`, `approve`, `compile`, `verify`, and
  `explain`.
- Exit codes are frozen as `0`, `2`, `3`, `4`, `5`, `6`, `7`, and `70` with
  the families specified in
  [`../specs/public-sdk-and-cli.md`](../specs/public-sdk-and-cli.md).
- V1 proposal kinds are `create_generated_note` and `explain_conflict`.
- The default near-duplicate configuration is 128 MinHash components in 32×4
  bands, threshold `0.85`, and at most 100 candidates per document; the full
  golden-vector question above remains open.
- Default safety limits are 10 sources, 250,000 files, 40 GiB total input, 2
  GiB per file, 16 MiB structured text, 100× archive expansion, 1,024-byte
  logical paths, and 240-byte components.
- SQLite uses WAL, foreign keys, `FULL` synchronous mode, schema
  `user_version = 1`, deterministic transaction order, and Unix mode `0600`.
  Migration/resume/encryption remain open.
- Conflict approvals are immutable plan/content-hash-bound overlays and V1
  accepts only `waived_by_policy` (ADR-0009).
- V1 evidence is either file/body-level with no block/span, or block-level with
  the exact block content hash and an omitted or exact block span. Arbitrary
  byte spans are rejected. `DocumentProjection.snapshot_id` makes that evidence
  constructible by a stateless provider and is revalidated during approval.
- A documentation defect found during REQ-PAR-002 work named
  `BaseArtifactId` in the canonical IR but omitted its stable formula.
  ALG-SNP-001 now defines Document, Canvas, and Base file-level identities with
  separate domains over the sealed `SnapshotId` and `SourceFileId`; the Base
  formula is `H("vaultc:base:v1\0" || lp(raw(SnapshotId)) ||
  lp(raw(SourceFileId)))`.
- Canvas file references resolve through sealed source-local indexes to typed
  Document, Asset, Canvas, or Base targets. Zero candidates preserve a
  root-contained raw path with a diagnostic; multiple candidates create a
  node-scoped required conflict and V1 can publish it only through an explicit
  policy waiver. Rewritten Canvas seals an expected output hash and is
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
- The current `.vaultpack` is deterministic but unsigned. The lower-precedence
  project-context phrase that called it signed conflicted with the output
  specification; it was corrected in favor of the normative future signing
  profile.
