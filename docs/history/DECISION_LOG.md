---
title: Decision Log
status: historical
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0004
  - ADR-0005
  - ADR-0006
  - ADR-0007
  - ADR-0008
  - ADR-0009
source_refs:
  - HIST-SHARED-CHAT
  - HIST-CURRENT-PLAN
---

# Decision Log

Append-only. Normative details live in specifications and accepted ADRs.

## 2026-08-14 — MCP roles and platform reframing

- Researched Obsidian MCP options with Claude Code and Codex as primary clients.
- Assigned complementary roles to `lstpsche/obsidian-mcp` (retrieval), `totocaster/arrowhead` (graph/discovery), `bitbonsai/mcpvault` (safe materialization concepts), and `blacksmithers/vaultforge` (topic/theme compression).
- Reframed the system from “Vault Merge Platform” to “Knowledge Compilation Platform.”
- Chose immutable raw snapshots, canonical knowledge representation, derived/rebuildable indexes, evidence-bearing claims, and topic-relative multidimensional benchmarks.
- Rejected public cloud and initial Kubernetes; selected ordinary Linux VMs with Docker Compose as the platform direction.

## 2026-08-15 — Framework product boundary

- Chose a framework-first product: Rust library and CLI before MCP/plugin/web platform.
- Chose stable Rust 2024, dual MIT OR Apache-2.0, SemVer, and Linux/macOS/Windows V1 support.
- Set V1 target to 10 Vaults, 100,000 notes, 20 GB, ≤20 minutes and ≤2 GB peak RSS on the reference machine.
- Made Markdown/frontmatter/Obsidian links/attachments/JSON Canvas first-class; `.base` opaque with warnings; `.obsidian/**` excluded.
- Decided input Vaults are immutable and output is a new integrated Compiled Vault without raw source copies.
- Chose a deterministic core with provider-neutral optional AI proposals, deterministic validation, explicit approval, and record/replay.
- Chose Rust traits plus versioned NDJSON for cross-language providers, with no mandatory vendor adapter in core.
- Separated brain-inspired memory/retrieval models into future experimental `vaultc-memory` work.

## 2026-08-16 — Documentation baseline

- Established Markdown as the cold-start implementation contract.
- Added precedence rules, role routing, requirement/algorithm IDs, stable and experimental algorithm templates, ADRs, source/transcript preservation, and traceability obligations.
- Recorded publicly recoverable source material; inaccessible tool outputs, custom instructions, and image bytes remain explicitly unavailable rather than reconstructed.

## 2026-08-16 — Compiler vertical slice and approval hardening

- Implemented the Rust 2024 workspace with `vaultc`, `vaultc-protocol`, and
  `vaultc-cli` under the pinned Rust 1.97.1 toolchain.
- Implemented safe directory/ZIP/`tar.zst` snapshotting, domain-separated
  identities, Markdown/Canvas/Base/attachment IR, exact and bounded
  review-only near deduplication, portable path/conflict planning, Markdown
  rewrites, atomic sibling staging, checksums, deterministic VaultPack creation,
  provenance output, independent verification, and SQLite inspection state.
- Implemented provider-neutral Rust traits and protocol V1, a bounded NDJSON
  command provider, explicit document disclosure, remote-provider double
  consent, proposal/evidence validation, content-hash-bound approvals,
  transcript redaction/commitment, deadline/SIGINT cancellation, and process
  cleanup.
- Accepted ADR-0009: `DraftPlan` remains immutable and conflict decisions are a
  plan/content-hash-bound `ApprovedPlan` overlay. V1 external conflict decisions
  are explicit `waived_by_policy` records only; typed resolution actions remain
  future work.
- Hardened serialized control-file validation and independent artifact
  verification against plan, manifest, checksum, provenance, proposal,
  conflict, and transcript tampering. Preserved provenance for every exact note
  and attachment occurrence and redacted absolute source locators from build
  artifacts.
- Froze CLI exit families `0`, `2`, `3`, `4`, `5`, `6`, `7`, and `70` and the
  version 1 proposal/decision wire shapes.
- Verified 59 tests plus doctests, warning-free Clippy, and rustfmt on macOS
  arm64. Did not claim V1 release completion: Canvas rewrites, full typed
  provenance graph, platform/performance/compatibility/fuzz gates,
  race-hardened no-clobber publication, signatures, SBOM/release automation,
  MCP, plugin, registry, and experimental memory algorithms remain open.

## 2026-08-16 — Evidence span and projection binding

- Closed an evidence ambiguity by permitting only spanless file/body evidence
  or exact block-hash evidence with an omitted or exact sealed block span.
  Arbitrary document byte ranges now fail closed.
- Added the owning `snapshot_id` to protocol V1 `DocumentProjection`, allowing
  a stateless provider to construct an evidence reference from disclosed data.
  Approval and CLI replay revalidate the snapshot/document binding.
- Added regression coverage for six file/block evidence cases, required
  projection-field decoding, a real generated-note NDJSON lifecycle through
  explain, stale augmentation rejection, and tampered approved-transcript
  rejection. The local suite increased from 59 to 61 tests.

## 2026-08-16 — Typed Canvas reference rewriting

- Completed the pre-release schema 1 Canvas contract with typed pending,
  resolved, unresolved, and ambiguous reference states and Document, Asset,
  Canvas, and Base targets. Added `BaseArtifactId` using a distinct stable
  snapshot/file identity domain after resolving its missing normative formula.
- Allocated all four target output maps before resolution. Rewritten Canvas now
  records node-scoped target changes and an expected output hash in a sealed
  `RewriteCanvas` operation; unchanged Canvas is still copied byte-for-byte.
- Bound Canvas ambiguity conflicts to Canvas ID, node ID, and raw path. Zero
  candidates retain only root-contained raw paths with diagnostics; multiple
  candidates require an explicit V1 policy waiver; source/output-root escapes,
  duplicate JSON object keys, and duplicate node IDs fail closed.
- Extended compilation and independent verification to reconstruct canonical
  Canvas bytes, preserve unknown fields, validate typed target membership, and
  reject semantic changes or removed targets even after checksums, manifest,
  and artifact ID are resealed.
- Treated this as completion of unpublished `0.1.0` schema 1 rather than a
  compatibility migration. Earlier working-tree artifacts were never a
  published schema; formal migration/version matrices remain open.
- Added five Canvas integration tests. The macOS arm64 local suite increased
  from 61 to 66 tests and passes with warning-free Clippy and clean rustfmt.

## 2026-08-16 — Source-derived output commitments

- Added a required `expected_output_hash` to pre-release schema 1 Markdown
  rewrite operations. The operation identity now binds snapshot/source hash,
  destination, expected output hash, and the exact ordered replacement recipe;
  working-tree plans from before this field fail closed without a compatibility
  default.
- Planning derives the recipe from sealed link resolution, reads each affected
  source once, validates original link slices, applies replacements, reparses
  the result, and retains only the output hash. Compilation reopens the source,
  rederives the recipe, and independently reapplies and checks it.
- Generalized verification so every source-derived Copy, Markdown, and Canvas
  output must match its sealed commitment. Markdown verification reverses the
  replacements against the output to reconstruct a source candidate, compares
  its source hash, and reparses the resulting link semantics without embedding
  original source bytes in the artifact.
- Corrected the same-precedence IR wording that implied raw bytes lived in
  `Document` or `BaseArtifact`; immutable source snapshots own those bytes, and
  IR/Plan retains only identity, spans, semantic records, and references.
- Classified planning source/policy/decision/invariant failures as CLI exits
  `3`/`2`/`4`/`70`; malformed pre-hash plans passed to `approve` now return
  decision exit `4` instead of provider exit `5`.
- Added three security regressions, one CLI unit test, and one CLI integration
  test. The macOS arm64 local suite increased from 66 to 71 tests and passes
  with warning-free Clippy and clean rustfmt.
