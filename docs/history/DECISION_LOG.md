---
title: Decision Log
status: historical
owners:
  - release-maintainer
last_updated: 2026-09-06
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
  - ADR-0015
  - ADR-0016
  - ADR-0017
  - ADR-0018
  - ADR-0019
  - ADR-0020
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
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

## 2026-08-16 — Generated-note derivation commitments

- Completed the pre-release EvidenceId byte formula with fixed-width raw
  identities, an explicit file/block/span mode byte, and big-endian exact span
  offsets so host endianness and display strings cannot affect evidence IDs.
- Added a required tagged materialization to every approved proposal. Generated
  notes seal destination, canonical emitted-body hash, complete rendered-output
  hash, proposal-order EvidenceId values, and operation ID; advisory proposals
  are explicitly non-materializing.
- Made compilation and independent verification reconstruct the exact generated
  note from the approved proposal and require equality across materialization,
  frontmatter, provenance, and output bytes. Pre-materialization development
  approvals fail closed without a compatibility default.
- Extended the partial provenance projection with canonical EvidenceId values
  and exact evidence/source linkage. The full ALG-PRV-001 typed graph,
  administrative envelope closure, and attribution records remain separate
  release work.
- Added four generated-provenance integration tests, including literal identity
  vectors and attacks that reseal body, frontmatter source IDs, provenance,
  manifest, checksums, and artifact identity. The local suite increased from 71
  to 75 tests and remains warning-free and formatted.

## 2026-08-17 — Typed provenance graph and audit envelope

- Accepted ADR-0010 and replaced the unpublished flat provenance projection
  with versioned, content-addressed Source, Operation, Decision, Proposal,
  Approval, Output, and Edge records. Record IDs, strict record ordering,
  relation/cardinality rules, reachability, and acyclicity are deterministic
  and fail closed.
- Split provenance into a stored graph for content and pre-envelope audit files
  plus deterministic virtual records for provenance, manifest, checksums, and
  an explicitly queried canonical VaultPack. This removes self-hash cycles
  without silently excluding administrative outputs from explanation.
- Retained recognized Markdown frontmatter author/license declarations as
  original canonical typed values with declared, not-declared, opaque, and
  not-applicable states. The compiler does not infer SPDX meaning or legal
  compatibility.
- Added bounded, cursor-bound provenance pages and directory/VaultPack parity.
  Unknown JSON enum fields, duplicate or non-canonical records, legacy flat
  ledgers, cross-subject cursors, and fully resealed semantic graph changes now
  fail closed.
- Required canonical outer VaultPack bytes before naming the deterministic
  profile. Alternate compression/header encodings and sealed-level mismatch are
  rejected; extraction now bounds outer size and declared/streamed expansion
  ratio and copies members through a fixed-size buffer.
- Added nine typed-provenance integration tests, two additional VaultPack
  contract tests, and CLI pagination/package coverage. The macOS arm64 local
  suite increased from 75 to 87 tests and passes workspace tests, all-target
  warning-free Clippy, rustfmt, and diff checks.

## 2026-08-17 — SDK augmentation recording and offline replay

- Accepted ADR-0011 and made `vaultc::augmentation` the single semantic owner
  for sealed projection construction, live provider authorization, canonical
  recording, redaction hydration, deterministic validation, and offline replay.
- Added an opaque, non-serializable pre-disclosure authorization. External
  transports negotiate capabilities without source content, obtain this token
  under the sealed remote policy and per-call consent, and consume it when
  recording the response. The CLI no longer maintains a second authorization
  or transcript implementation.
- Required exactly four provider transcript records for every recorded exchange
  and rejected non-empty validations without that transcript. Approval now
  reruns proposal validation and compares proposal, content hash, verdict, and
  reasons before producing an `ApprovedPlan`.
- Added strict schema-1 augmentation JSONL with canonical lines, typed nested
  payload checks, header/provider/plan/projection binding, redacted-request
  reconstruction, line/file/proposal limits, and provider-free byte-identical
  replay. Immutable recording state is shared across replay clones to avoid a
  second full decoded graph allocation.
- Added CLI `replay`, strict recursive NDJSON duplicate/unknown-field rejection,
  pre-request remote authorization, no-clobber publication, and stable exit
  families. The canonical SDK and CLI recording bytes are identical.
- Added 14 SDK augmentation/replay tests and 3 CLI replay tests and migrated all
  proposal-bearing fixtures to canonical transcripts. The macOS arm64 suite
  increased from 87 to 105 tests and passes workspace tests, all-target
  warning-free Clippy, rustfmt, and diff checks.

## 2026-08-18 — Original source spelling and Unicode normalization boundary

- Accepted ADR-0012 and completed the unpublished schema-1 `SourceFile` with a
  required exact portable UTF-8 `original_path`, NFC `logical_path`, and tagged
  UTF-8 encoding. Semantic file/snapshot identities remain logical-path based;
  Plan, workspace projection, and typed provenance identities bind the observed
  spelling, so a spelling-only source rename invalidates a sealed plan.
- Replaced lowercase lookup with pinned full Unicode case folding after NFC and
  classified cross-source exact, Unicode-normalization, and case-fold
  collisions from their actual allocation spellings. The compiler asserts
  Unicode 17.0.0 NFC data and Unicode 16.0.0 case-fold data.
- Made directory, ZIP, tar, and PAX names strict UTF-8 portable paths. ZIP raw
  central-directory names are scanned and bound to the reader entry before any
  Unicode-extra/CP437 behavior; tar rejects malformed, duplicate, invalid UTF-8,
  or disagreeing PAX/GNU path carriers.
- Bound source archive container size, every effective member, declared
  aggregate size, and the complete `tar.zst` decompressed stream. Excluded,
  directory, link, metadata, and trailing bytes cannot evade resource budgets.
- Froze BOM-at-byte-zero, frontmatter-adjacent BOM, CRLF/lone-CR, Markdown span,
  Canvas raw-value, and preserved-reference containment behavior. Human CLI
  provenance sanitizes untrusted original spellings while JSON remains exact.
- Added 22 normalization/archive tests and two CLI regressions. The macOS arm64
  suite increased from 105 to 129 tests and passes workspace tests, all-target
  warning-free Clippy, rustfmt, and diff checks.

## 2026-08-18 — Atomic VaultPack publication contract

- Accepted ADR-0013 and assigned SDK and CLI VaultPack publication to the same
  core path: a restrictive sibling file is written, finished, synchronized,
  independently verified, and atomically published without replacement.
- Required `.vaultpack` destinations to be disjoint from the Compiled Vault in
  both containment directions and required every existing file, directory,
  live symlink, dangling symlink, or publication-race winner to remain
  untouched.
- Connected the existing `CompileOptions.create_pack` design through an
  additive `VaultCompiler::compile_with_options` facade while retaining two
  honest ordered publications. A later pack failure keeps the valid Compiled
  Vault; an all-or-nothing release requires a future single-root bundle.
- Distinguished pre-commit failure, which leaves no vaultc-created requested
  pack destination, from post-commit parent-sync failure, which retains the
  complete pack and reports `PublishedButDurabilityUncertain`.
- Kept pack bytes and artifact schemas unchanged. Portable no-replace
  publication of the Compiled Vault directory, descriptor-relative ancestor
  hardening, orphan-stage cleanup, and physical power-loss qualification remain
  separate work.
- Implemented the public SDK publisher, `compile_with_options`, shared CLI
  orchestration, explicit publication-state errors, and deterministic private
  fault checkpoints. Added 11 SDK publication cases, 3 fault-order unit cases,
  and 4 CLI lifecycle/unit cases; the macOS arm64 suite increased from 129 to
  148 tests with workspace tests, Clippy, rustfmt, and diff checks green.

## 2026-08-18 — Atomic Compiled Vault directory publication contract

- Accepted ADR-0014 to replace the destination check plus replace-capable
  directory rename with a single native no-replace commit on Linux, macOS, and
  Windows. Unsupported primitives and filesystems must fail closed without a
  weaker fallback.
- Required every existing file, directory, live/dangling symlink or reparse
  point, and late publication-race winner to remain untouched. A private
  immediately-before-publish barrier is the required regression boundary.
- Required Compiled Vault/source and integrated Pack/source disjointness before
  staging so requested publications cannot place temporary or final compiler
  state inside an immutable source.
- Required explicit cleanup or synchronized marked retention of failed stages,
  with a structured error retaining both the original and disposition failure.
- Kept public compile return types and serialized artifacts unchanged. This
  entry records the accepted contract; implementation and platform evidence
  are reported separately after the feature gates pass.

## 2026-09-01 — Atomic Compiled Vault directory publication implementation

- Replaced the final check-plus-rename boundary with target-native no-replace
  publication: `rustix` `NOREPLACE` on Linux/macOS and a non-replacing
  `MoveFileExW` wrapper on Windows. Unsupported primitives fail closed; there
  is no replace-capable fallback.
- Kept the restrictive sibling stage alive through materialization, recursive
  synchronization, independent verification, and the final namespace commit.
  A late file, directory, live symlink, dangling symlink, or concurrent
  compiler winner is preserved and reported as `OutputExists`.
- Added source/output/integrated-pack disjointness checks before staging,
  explicit remove-or-mark-and-retain disposition, and
  `StagingDispositionFailed` so the original error and cleanup/retention error
  are both observable. Post-commit parent-sync failure retains a verified Vault
  as `PublishedButDurabilityUncertain` and does not begin optional packing.
- Added 10 public SDK directory-publication cases, 9 deterministic private
  checkpoint/race/fault cases, and 5 CLI lifecycle cases. The macOS arm64 local
  suite increased from 148 to 172 tests; workspace tests, all-target Clippy
  with warnings denied, rustfmt, and diff checks pass.
- This local evidence does not qualify Linux/Windows filesystems, Windows
  reparse points, descriptor-relative ancestor swaps, process crashes, or
  physical power-loss durability. Those remain explicit release work.

## 2026-09-01 — Cross-platform CI evidence baseline

- Selected the ADR-0007 host matrix as Linux x86_64, Windows x86_64, macOS
  x86_64, and macOS arm64, all running the repository-pinned Rust toolchain,
  locked dependencies, and the complete all-feature workspace suite.
- Required a literal full-VaultPack SHA-256 golden on every host so independent
  jobs prove byte equality against one shared identity instead of merely
  proving two runs agree with themselves.
- Required a separate warning-free format/Clippy/documentation-integrity job,
  read-only workflow permissions, full commit pinning for reusable actions, and
  `fail-fast: false` for complete platform evidence.
- A workflow file is implementation, not verification evidence. Until remote
  jobs complete, Linux/Windows/macOS x86_64 status remains explicitly
  unverified in current-state and traceability documents.

## 2026-09-01 — Cross-platform CI implementation

- Added a read-only GitHub Actions workflow for Ubuntu 24.04 x86_64, Windows
  2025 x86_64, macOS 15 x86_64, and macOS 15 arm64. Every host runs the pinned
  Rust 1.97.1 toolchain, locked dependencies, and the complete all-feature
  workspace suite with matrix fail-fast disabled.
- Added a separate Linux quality job for rustfmt and all-target warning-free
  Clippy. The only reusable action is GitHub's checkout v6.0.2, pinned to the
  reviewed full commit `de0fac2e4500dabe0009e67214ff5f5447ce83dd` with stored
  credentials disabled.
- Added LF checkout normalization, a direct repository-relative Markdown-link
  regression, and the literal basic-Vault VaultPack SHA-256
  `e017fa28359eb9d49458fecda42b27a759c58a7546ecbd7c272d58eb8b230890`.
- The local macOS arm64 suite increased from 172 to 173 tests and remains green
  with locked workspace tests, Clippy, rustfmt, workflow YAML parsing, and diff
  checks. No remote is configured in this checkout, so the four hosted jobs are
  implemented but not yet execution evidence.

## 2026-09-02 — OKC V2 application, materialization, and release baseline

- Accepted ADR-0015 through ADR-0020: renamed the writable format family and
  identity domains to OKC schema 2, made multi-Vault inputs MCP-origin-neutral,
  introduced sealed typed Markdown/Canvas ambiguity actions and immutable
  materialization, froze V1 as verify/explain-only, selected one `okc` CLI/TUI
  executable and `.okc-project` format, and established cargo-dist/updater and
  native-signing release policy.
- Reorganized the workspace into `okc-core`, `okc-protocol`, `okc-app`, and the
  sole `okc` binary. Retained frozen internal V1 packages and a deprecated
  non-executable `vaultc` Rust facade for one minor version.
- Implemented V2 manifests, domains, audit layout and deterministic OKCPack;
  expanded IR with sections, typed blocks, typed Canvas members, raw hashes,
  media/size data, Vault content identity and plan resource estimates; added
  project schema 2 storage, private object writes, source rebinding and
  downstream invalidation.
- Implemented typed target selections over sealed candidate sets without
  mutating Draft Plans. Approval, compilation, audit verification, provenance,
  and independent artifact verification now bind the same Materialization ID.
  Added direct Markdown and Canvas compile/verify regressions, including
  suffix/display and unknown-field preservation.
- Added the twelve-screen Ratatui/Crossterm reducer shell, terminal-control
  escaping/restoration guard, English/Korean and accessibility modes, project
  navigation, `doctor`, `validate`, and receipt-aware `update` commands. Real
  TUI worker effects and PTY workflows remain release work.
- Discovered a same-change normative conflict: axoupdater 0.10.0's blocking API
  internally creates a private current-thread Tokio runtime while ADR-0019 had
  unqualified “no Tokio” wording. Recorded the defect and accepted ADR-0021,
  limiting the exception to explicit updater calls and forbidding Tokio in the
  TUI/core/provider worker architecture.
- Validated `dist-workspace.toml` with the official cargo-dist 0.32.0 binary and
  its published SHA-256. The plan contains all four native archives, shell and
  PowerShell installers, checksums, source archive, CycloneDX SBOM, and GitHub
  attestations.
- On macOS arm64 with Rust 1.97.1, 323 tests plus all doctests pass; locked
  all-feature/all-target Clippy is warning-free, rustfmt is clean, and all
  repository-relative Markdown links resolve. This is local development
  evidence only. Stable `0.2.0` remains prohibited until the full four-host
  matrix succeeds twice on one SHA, the 20 GB performance gate passes, and
  Apple/Windows native signing and notarization evidence exists.

## 2026-09-02 — Public README synchronized with the V2 boundary

- Updated the repository entry point to explain MCP-origin-neutral multi-Vault
  compilation, deterministic link/deduplication behavior, schema-2 format
  literals, and the V1 verify/explain-only boundary.
- Documented the sole `okc` CLI/TUI surface, `.okc-project` layout and plaintext
  trust boundary, local build/verification commands, receipt-aware updates, and
  the distinction between unsigned user Packs and native-signed application
  releases.
- Made the development status fail-closed: local macOS arm64 evidence is stated
  separately from the missing TUI, performance, hardening, remote CI, and
  native-signing gates. No normative behavior or release status changed.

## 2026-09-02 — Current CLI and TUI operator guide

- Added a Korean task-oriented guide for building the development binary and
  running the complete AI-free CLI lifecycle from inspection through
  provenance explanation, with optional NDJSON augmentation and remote-consent
  guidance.
- Documented the exact decision-file shapes, no-clobber destinations, command
  outputs, global-option scope, exit families, and the special `plan` exit-4
  review branch.
- Made the TUI boundary explicit: it currently provides twelve-screen
  navigation, existing-project opening, keyboard/accessibility settings, and
  safe rendering, but it cannot create a project or execute/persist real
  inspect, review, provider, compile, or verify work yet.
- Linked the guide from the README and documentation index and mapped it to
  REQ-SDK-001 and REQ-APP-001. No implementation, schema, architectural
  decision, or release status changed.

## 2026-09-03 — Task-oriented web guide

- Replaced the single operator walkthrough with Markdown source pages for a
  five-step Quickstart, CLI, TUI, conflict review, AI provider, troubleshooting,
  and current implementation status. The information architecture follows the
  goal, prerequisites, numbered steps, expected result, and next-guide pattern
  of a task-oriented Quickstart while retaining OKC-specific wording and visual
  identity.
- Added a VitePress 1.6.4 site with local search, copyable platform-specific
  command groups, responsive navigation, dark/light themes, and an explicit
  development-status warning. Vite is overridden to 6.4.3; the production
  build passes and `npm audit --audit-level=high` reports zero vulnerabilities.
- Extended the repository-relative Markdown link contract to include `guide/`
  and synchronized the README, current state, index, traceability matrix, and
  documentation quality-gate wording. No compiler behavior, schema,
  architectural decision, or release status changed.
- Running the Quickstart verbatim exposed a current macOS/Linux relative-output
  path normalization defect. The operator examples now use absolute output and
  Pack destinations, and the limitation is recorded in current state and
  troubleshooting; fixing compiler behavior remains separate implementation
  work.

## 2026-09-03 — V3 AI-required integration development slice

- Recorded the V2/V3 normative conflict before code changes and accepted
  ADR-0022 through ADR-0024 for schema-3 identities, frozen V1/V2 readers,
  provider profiles/disclosure/recording, and evidence-complete synthesis with
  critic and human approval gates.
- Added `okc-ai`, `okc-legacy-v2`, schema-3 protocol envelopes, schema-3 project
  manifests, append-only run/task/exchange/revision/approval journals, sensitive
  preflight routing, and source-binding-only V2 project upgrade.
- Added an initial resumable CLI path from embedding and deterministic semantic
  candidates through organizer taxonomy, taxonomy review, per-cluster synthesis,
  critic review, cluster approval, and sealed integration-plan creation.
- Added provider-free V3 directory compilation, independent byte regeneration,
  canonical-note and legacy-redirect provenance explanation, and strict closure
  validation for every block/frontmatter disposition, section evidence,
  contradiction, omission, critic finding, waiver, approval, and recording.
- Kept release status explicitly partial. Deterministic block chunking/batching,
  fixed-seed HNSW and complete candidate union, hierarchical/revision/manual
  synthesis flows, sensitive exceptions, the schema-3 command supervisor, V3
  Pack and non-Markdown materialization/link rewriting, shared TUI workers, scale,
  fuzz/PTY, cross-platform, and signing evidence remain required.
- Corrected the relative-directory publication sibling comparison and added a
  regression so relative and absolute destinations follow the same no-replace
  path.

## 2026-09-03 — V3 command-surface validation follow-up

- Exercised schema-3 project creation, source addition, default and
  role-specific AI routing, and JSON status with the built CLI. This exposed a
  Clap runtime assertion caused by an optional positional role before a
  required profile.
- Kept the documented `ai-route set [ROLE] PROFILE` contract by decoding one or
  two required positional values explicitly, added parser coverage for both
  forms and the missing-value failure, and repeated the CLI smoke successfully.
- Recorded that the development binary still exposes V2 writer commands for
  the regression harness. ADR-0022's public legacy read-only boundary remains
  a stable-release blocker; no new architectural decision was introduced.

## 2026-09-03 — V3 README and operator-documentation reconciliation

- Added an implementation-status matrix and the complete current schema-3 CLI
  loop to the repository README, including synthetic provider testing,
  taxonomy/cluster approval, offline compile, verify, and explain.
- Separated implemented V3 directory behavior from frozen V2 Pack and
  non-Markdown regression behavior in the README, documentation index,
  current-state guide, CLI contract, and traceability matrix.
- Promoted the V3 integration guide in the VitePress navigation and relabeled
  the existing five-minute guide as a V2 regression Quickstart. No normative
  behavior, algorithm status, quality-gate result, or release status changed.

## 2026-09-03 — TUI operator guide expansion

- Expanded the TUI guide from a key list into a task-oriented walkthrough for
  prerequisites, CLI project preparation, startup modes, screen layout, status
  meanings, runtime-only display settings, safe exit, and troubleshooting.
- Added an exact screen-by-screen table and CLI handoff map. The guide now
  distinguishes static placeholders and disconnected worker requests from real
  project state or failed source operations.
- Corrected the obsolete claim that the CLI could not create a project and made
  the current navigation-only boundary explicit. No TUI implementation,
  normative behavior, requirement state, or release status changed.

## 2026-09-05 — Cwd-first V3 TUI, shared services, and native credentials

- Recorded the REQ-INT-003–006 traceability mismatch and the ADR-0023
  environment-only credential conflict in `OPEN_QUESTIONS.md`, then accepted
  ADR-0025 and added REQ-APP-002/REQ-SEC-003 before implementation.
- Added bounded cwd project/Vault discovery, canonical absolute active source
  replacement, internal SQLite schema 4 append-only source-set/plan/output/
  feedback pointers, OS-keychain/environment credential resolution, zeroized
  fixed-mask secret handling, shared provider/integration services, and a
  bounded single-operation worker with publication-barrier cancellation.
- Replaced the navigation-only shell with the ten-screen V3 TUI for provider
  capability testing, Vault selection, local disclosure preflight, taxonomy
  editing, three-pane cluster review, hash-bound regeneration, provider-free
  compile, independent verify, and provenance/settings inspection.
- Routed CLI provider, taxonomy/cluster approval, latest-plan compile, cwd
  discovery, and TUI operations through the shared application boundaries.
  Cluster CLI waivers now require exact per-item rationale mappings; completed
  cache entries remain resumable without deleting historical runs or approvals.
- Kept schema-3 command providers, manual section amendment, V3 Pack,
  non-Markdown carry-through, scale, PTY, cross-platform, and release-signing
  evidence as explicit blockers; no stable-release claim was made.

## 2026-09-05 — Shared integration execution boundary completed

- Moved embedding, deterministic candidate generation, organizer, synthesis,
  critic, cache-resume, per-miss disclosure authorization, and plan sealing
  behind `IntegrationService::execute` in `okc-app`.
- Removed the TUI-to-CLI-module call. CLI and TUI now invoke the same
  application service directly, and progress identifies the active cluster.
- Made omission/minor rationales individually keyed in the TUI and restored
  remote-route awareness when an existing project is reopened.

## 2026-09-05 — V3 documentation and verification reconciliation

- Reconciled the README with the schema-3 project/artifact boundary and the
  private schema-4 application journal, and narrowed release blockers to the
  still-missing manual-review, command-adapter, provider-conformance, and TUI
  PTY/platform evidence.
- Updated release, traceability, and algorithm-policy wording for ADR-0025 and
  the current V3 default path. Removed two orphan workspace-member declarations
  for nonexistent or targetless crates that were absent from the lockfile and
  from the documented product surface.
- Re-ran the complete locked all-feature workspace test suite and doctests,
  warnings-as-errors Clippy, workspace rustfmt, repository Markdown-link test,
  VitePress production build, and high-severity npm audit. All passed locally;
  the demo inventory also matches its documented 112 Markdown files and 434
  wikilinks. No stable-release or cross-platform-evidence claim was added.

## 2026-09-06 — Python and Node.js library boundary

- Accepted ADR-0026 and added REQ-SDK-002 for one runtime-neutral Rust facade,
  thin PyO3/napi-rs adapters, bounded jobs, structured errors, explicit paths,
  per-call remote consent, and environment-variable-only binding credentials.
- Added the `okc-interop`, `okc-python`, and `okc-node` workspace packages.
  Moved V1/V2/V3 artifact detection and read-only verify/explain dispatch into
  a shared application service and feature-gated updater/native-keyring support
  so bindings import no CLI/TUI runtime behavior.
- Added typed `okc-compiler` Python and npm packages, Python 3.11 abi3 and
  Node-API 9 native builds, four platform-addon package definitions, wheel/
  sdist/npm clean-install checks, checksum/SBOM generation, and a pinned-action
  CI matrix without publication credentials.
- Added Python and Node.js complete V3 approval/compile/verify/explain tests.
  Equivalent loopback providers produce the same literal artifact inventory
  SHA-256 on local macOS arm64. The complete locked Rust workspace suite and
  both installed-package smoke paths pass locally.
- Kept all existing V3 blockers and protected remote-platform gates in force.
  Neither language package nor V3 is declared stable or published by this
  change.

## 2026-09-06 — Language-package security and lifecycle hardening

- Added frozen V1/V2 binding fixtures plus provider-failure, missing-secret,
  per-call remote-consent, output/no-clobber, source-immutability, and shared V3
  byte-golden coverage to both public language suites.
- Made artifact-family detection reject symlinked or oversized metadata and
  classify malformed manifests as verification failures. Canonicalized
  in-process project reservations across filesystem aliases and removed the
  last cwd lookup from explicit binding source mutations.
- Recursively rejected credential-bearing provider options and redacted a
  provider error that reflects an authorization value. Scheduler teardown now
  detaches bounded workers instead of joining them on a Python/Node finalizer
  thread, preventing runtime-lock deadlock while queued work drains normally.
- Rebuilt and clean-installed the macOS arm64 abi3 wheel, lockfile-bearing
  sdist, root npm tarball, and platform-addon tarball. Ten Python tests, ten
  Node.js tests, strict mypy/TypeScript checks, the complete locked Rust suite,
  warnings-as-errors Clippy, SBOM/checksum validation, and CJS/ESM package
  imports pass locally. Remote four-platform evidence remains pending.

## 2026-09-06 — Language-package documentation and release handoff

- Added source-checkout build/test entry points and the explicit-path,
  environment-secret, consent, approval, Job, and structured-error boundaries
  to the root README and Korean user guide.
- Extended the release procedure, roadmap, glossary, current state, and
  time-sensitive facts register with the Python/Node artifact set, clean-install
  and type gates, shared-fixture parity, registry revalidation, and future
  language ordering.
- Rechecked the official PyPI and npm JSON endpoints for `okc-compiler`; both
  returned HTTP 404 on 2026-09-06. This is temporary availability evidence,
  not a reservation or publication authorization.

## 2026-09-06 — Current Schema 3 single-source cleanup

- Accepted ADR-0027 and published annotated `archive/v0.1.0` and
  `archive/v0.2.0` tags. Their remote peeled commits are respectively
  `7181fc2dea54288f176b66a00e2335da7f58bdfd` and
  `b9f9e88bc531095fbeb2ece4155c980bdf10708b`; neither tag is a release.
- Removed the Schema 1/2 compiler/readers, retired protocol crates, deprecated
  facade, old command-provider surface, committed retired artifacts, and their
  workspace dependencies. All seven remaining crates use version `0.3.0`.
- Hid the retained hostile-input snapshot/Markdown/planning machinery behind
  `CorpusBuilder::build -> PreparedCorpus`, renamed current Rust APIs without
  generation suffixes, and preserved every current stored/hash/database/output
  contract and the existing project schema.
- Reduced CLI and `ArtifactService` to current directory operations. Python and
  Node.js now expose typed interop-schema-2 verification/explanation results,
  require an output path for explanation, and return a stable explicit error
  for recognizable Schema 1/2 markers while other invalid inputs fail closed.
- Normalized 56 active document statuses to `normative`, added current-only
  specifications/guides/traceability, and retained prior ADR bodies and
  immutable transcripts as historical records.
- Passed the locked seven-package Rust check, 86 tests and doc-test targets,
  warnings-as-errors Clippy, rustfmt, link test, and VitePress build. Fresh
  Python and Node native builds each passed 12 tests plus strict type/package
  smoke checks; Rust, Python, and Node retained artifact inventory SHA-256
  `452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5`.
- Remote platform, performance, fuzz/TOCTOU, non-Markdown/Pack, signing, and
  protected-publication gates remain open; stable `0.3.0` is still prohibited.

## 2026-09-06 — Correctness, trust-boundary, and lifecycle audit

- Paused implementation to record four current normative-document conflicts
  in OPEN_QUESTIONS. Reconciled reserved command-provider wording, retired
  provenance/decision authority, and unsupported materialization claims using
  already accepted ADR-0027 and current specifications. Retained historical
  ADR bodies, transcripts, current identities, and artifact bytes.
- Reproduced and fixed stale configuration/taxonomy/regeneration authority,
  unplanned-file verification with resealed inventory, normal multiline body
  rejection, Unicode/prefix/file-directory collisions, source-ID decoding and
  conflicting metadata slots. Held the project writer lock through compile/
  verification recording and bound verified output to the current plan.
- Centralized strict duplicate-key JSON, portable path checks, preflight
  recording, checksums, and manifest bounds. Corrected PEM/email scanning,
  added metadata-only document disclosure checks with explicitly versioned
  private scanner results, parsed real loopback endpoints, strengthened task
  cache identity/order, and enforced an end-to-end provider retry deadline.
- Rejected workspace/source overlap and unsafe project/object/database
  symlinks before persistence. Fixed full-progress-queue worker deadlock,
  bounded language scheduler submissions, shared same-project reservations
  across clients, and released reservations before terminal notification.
  Preserved arbitrary Node.js source metadata keys and fixed optional core
  feature builds plus one-component relative CLI publication.
  Rejected profile edits now validate before mutating the in-memory route.
- Passed 110 locked all-feature Rust tests and all doc-test targets, workspace
  check/Clippy/rustfmt, three independent optional-feature library Clippy runs,
  and the documentation link/guide gates. Fresh Node native output passed 13
  tests, strict TypeScript, and six-file package dry-run. A fresh ABI3 Python
  wheel passed 12 tests and strict mypy from an isolated extraction; host pip
  installation errors are not counted as a successful clean install.
- Rust/Python/Node retained inventory SHA-256
  `452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5`.
  Added the detailed audit report and requirement-to-regression mappings.
  No dependency, public schema, accepted invariant, release status, commit,
  remote tag, or publication changed. Platform/fuzz/TOCTOU/crash/scale/native
  signing and clean-install gates remain open.

## 2026-09-06 — Local stabilization and installation follow-up

- Preserved the preceding audit changes and reproduced additional hardlink,
  ambient-ignore membership, and manifest/journal failure-order defects before
  fixing them. Unix managed files reject multiple links; Linux/macOS source
  content opens use pinned directory handles and per-component no-follow checks.
  Configuration/source updates invalidate journal authority before replacing
  the manifest and update in-memory configuration only on success.
- Synchronized staging directories bottom-up, held cleanup ownership through
  real atomic no-replace publication, disarmed it immediately on success, and
  reported failed cleanup with its exact path and both causes. Added private
  fault checkpoints and actual barrier/concurrent-winner regressions without
  adding a public fault API or changing output schema/bytes.
- Added fixed-seed JSON/ZIP mutation and creation-order properties, plus a
  POSIX synthetic-provider TUI harness covering explicit approvals, provider-free
  compile/verify/restart, source immutability and terminal restoration. Added the
  PTY smoke to POSIX CI; remote execution is not claimed.
- Passed 130 Rust tests and all doc-test targets on macOS arm64 and on Linux
  x86_64 GNU under Docker emulation, plus local check/Clippy/format/docs gates.
  Fresh pip wheel/sdist virtualenv installs each passed 12 Python tests;
  strict mypy and clean npm tarball/CJS/ESM installation, 13 Node tests and
  TypeScript checks passed. A process-local Homebrew Expat loader path resolved
  the host pip error without rewriting the earlier failed-install record.
- Validated SBOMs and package checksums. Repeated wheels initially differed
  because of generated SBOM UUID/time; setting the source-commit epoch retained
  the SBOM and produced identical repeated wheel bytes. Added that control and
  comparison to SDK CI. This cached packaging check is not independent native
  clean-build or two-pass remote release evidence.
- Added a bounded, temporary synthetic ingestion probe and dropped an obsolete
  corpus inspection before public projection. The 10-Vault/100k-note/25.6-MB
  run used 174.569 seconds for corpus construction and 5,360,336,896 bytes peak
  RSS, with no provider calls or semantic-candidate measurement. QG-006 is not
  passed; the historical 2-GB target is exceeded and current benchmark/reference
  decisions are explicitly recorded in OPEN_QUESTIONS.
- Updated current state, requirement/test traceability, normative behavior,
  release instructions and a detailed follow-up report. No dependency, current
  identity/artifact golden, accepted invariant, commit/tag, publication or
  stable-release status changed. Future materializers/Pack, scale, full
  cancellation/fuzz/ancestor-race protection, native matrices and signing remain.

## 2026-09-06 — Stabilization source checkpoint and proposed architecture

- On explicit user request, staged and committed the existing 48-file
  correctness/stabilization change as
  `f4c50ad71f61478316d5e68182807ef8612f4d2b`, then pushed it normally to
  `origin/main`. No force push, release tag, signature or package publication
  was performed. Earlier audit records stating that no commit/push occurred
  remain accurate records of those earlier work sessions.
- Drafted ADR-0028 for bounded semantic storage/execution and explicit replay/
  live performance qualification; ADR-0029 for human-origin amendments with
  fresh critic/approval and local-only scanner review; ADR-0030 for separately
  reviewable current Pack, bounded large control files and extended typed
  materialization; and ADR-0031 for the real cancellation barrier, configuration
  intents, orphan policy and filesystem/SQLite capability qualification.
- Added a proposal review index, requirement-to-design mappings, current-state
  notice and open-question links. All four decisions are `proposed`: no current
  schema, bound, algorithm default, privacy invariant, API, quality gate or
  release status was changed. Numeric profiles, possible journal/format
  versions and future compatibility choices require owner review.
- Kept accepted ADR bodies and historical transcripts intact. Drafting is not
  implementation, acceptance, experimental promotion, command-provider
  restoration, remote-model consent or protected-release authorization.
- Verified `git ls-remote origin refs/heads/main` resolves to the exact
  stabilization commit. After drafting, `cargo test --locked -p okc-core --test
  documentation_contract`, `npm run docs:build --prefix guide`, and
  `git diff --check` passed. The four draft frontmatters and proposal index
  parsed successfully as `proposed`. This documentation-only step does not
  claim a new runtime suite or remote CI qualification.

## 2026-09-06 — Windows SDK smoke-test portability

- The supplied Windows Actions log showed a successful wheel install and
  compilation followed by Python's literal path assertion failure. The shared
  application intentionally canonicalizes the output before overlap checks
  and publication; Rust's Windows canonical path uses extended-length syntax
  ([standard library contract](https://doc.rust-lang.org/std/fs/fn.canonicalize.html)).
  Kept that runtime behavior and changed Python to compare directory identity
  with `Path.samefile`. Node's project and compile assertions now compare both
  paths through `fs.realpathSync.native`. Both complete workflows reuse the
  returned compile path for verification and explanation.
- Also reproduced the next Windows fixture defect locally by forcing CRLF
  text writes before the fix: the artifact digest became
  `c338c419f967a82a2cda02455c30ce4a8291eb8525683810594103941fd4c69c`.
  The Python fixture now writes explicit LF bytes, matching Rust/Node and
  retaining the accepted `452ca067...413c7e5` inventory golden.
- No runtime source, dependency, schema, accepted algorithm, output path
  policy, approval authority or release status changed. This extends
  REQ-SDK-002/REQ-REL-001 and QG-001/002/007 evidence under ADR-0026/0027;
  ALG-SNP-001 and ALG-INT-001 remain normative and unchanged. Preserved the
  pre-existing proposed-ADR documentation edits and all historical entries.
- Built and pip-installed a fresh macOS arm64 wheel into
  `/private/tmp/okc-windows-ci-fix-python`; the Python suite passed **12/12**
  from `/private/tmp`, with imports confirmed from that installed target.
  The rebuilt Node addon passed **13/13**. Both unchanged artifact goldens,
  strict mypy and TypeScript checks passed. Windows native execution remains
  pending; these are local development results.

Exact successful build/install commands from the repository root:

```sh
SOURCE_DATE_EPOCH=1788697818 PATH=/opt/homebrew/opt/rustup/bin:/private/tmp/okc-sdk-smoke.TDLo5i/python-tools/bin:$PATH DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib /private/tmp/okc-sdk-smoke.TDLo5i/python-tools/bin/maturin build --manifest-path bindings/python/Cargo.toml --release --locked --offline --out /private/tmp/okc-windows-ci-fix-dist
DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib /private/tmp/okc-remaining.G9h3fx/python-release-wheel/bin/python -m pip install --no-index --no-deps --target /private/tmp/okc-windows-ci-fix-python /private/tmp/okc-windows-ci-fix-dist/okc_compiler-0.3.0-cp311-abi3-macosx_11_0_arm64.whl
PATH=/opt/homebrew/opt/rustup/bin:$PATH npm run build --prefix bindings/node
npm test --prefix bindings/node
npm run typecheck --prefix bindings/node
```

Exact successful Python test/type commands, with working directory
`/private/tmp`:

```sh
DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib PYTHONPATH=/private/tmp/okc-windows-ci-fix-python:/private/tmp/okc-sdk-smoke.TDLo5i/python-tools /private/tmp/okc-remaining.G9h3fx/python-release-wheel/bin/python -m pytest --import-mode=importlib /Users/sihun/workspace/projects/ObsidianKnowlegeComplication/bindings/python/tests -q
DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib PYTHONPATH=/private/tmp/okc-windows-ci-fix-python:/private/tmp/okc-sdk-final.fMdL93/python-tools /private/tmp/okc-remaining.G9h3fx/python-release-wheel/bin/python -m mypy --strict /Users/sihun/workspace/projects/ObsidianKnowlegeComplication/bindings/python/tests/typing_contract.py
```

The sandbox initially blocked localhost fixture binding and napi's process
inspection (`EPERM`). Those commands passed after approved execution outside
the sandbox. Pip disabled its unwritable user cache and still completed the
offline target install successfully.

Final repository checks also passed:

```sh
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo test --locked -p okc-core --test documentation_contract --test sdk_output_golden
npm run docs:build --prefix guide
git diff --check
```

The Rust command passed one documentation-link test and one unchanged
cross-language golden test; VitePress built successfully. An earlier Cargo
invocation without the Rust toolchain directory in `PATH` could not locate
`rustc`; the explicit `PATH` above resolved that host setup error.
