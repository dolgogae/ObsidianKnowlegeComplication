---
title: Requirements Traceability Matrix
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-09-01
decision_refs:
  - ADR-0001
  - ADR-0004
  - ADR-0009
  - ADR-0010
  - ADR-0011
  - ADR-0012
  - ADR-0013
  - ADR-0014
source_refs:
  - HIST-COMPILER-PLAN
---

# Requirements Traceability Matrix

This matrix describes the current `0.1.0` tree. `verified` is used only when
the named behavior has direct automated evidence; platform-limited evidence is
called out explicitly. Broader release gates remain in
[`CURRENT_STATE.md`](CURRENT_STATE.md).

| Requirement | Contract and algorithm | Production implementation | Automated evidence | State |
|---|---|---|---|---|
| REQ-SNP-001 immutable inputs | [`vault-compilation-pipeline.md`](specs/vault-compilation-pipeline.md), ALG-SNP-001, ADR-0003, ADR-0012 | `vaultc::{source,snapshot,compile}` including sealed original/NFC path rereads | source immutability; source-change; spelling-only rename; sealed-plan immutability tests | implemented; metadata/permission/timestamp preservation is not directly tested |
| REQ-SNP-002 stable content identity | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md), ALG-SNP-001, ADR-0012 | `vaultc::{identity,snapshot,workspace}`; semantic IDs use NFC logical paths while Plan/provenance bind original spelling | identity unit tests; `canonical_path_ids_ignore_spelling_but_plan_and_provenance_bind_it`; deterministic inspection/packing | implemented; supported-platform golden ID matrix pending |
| REQ-PAR-001 Obsidian Markdown parsing | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md), ALG-NRM-001, ADR-0008, ADR-0012 | `vaultc::{parse,ir,plan,compile,verify}`; pinned NFC/full-fold tables, raw spans, sealed recipes, reverse reconstruction | parser unit tests; normalization corpus covers BOM, CRLF/lone CR, full fold, containment, and multi grow/shrink rewrites; output-hash/reseal tests | implemented; complete escaped/delimiter corpus and platform matrix pending |
| REQ-PAR-002 JSON Canvas parsing and rewrite | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md), ALG-NRM-001, ADR-0008 | strict/unknown-preserving parsing in `vaultc::parse`; typed reference state/targets in `vaultc::ir`; output maps, conflicts, and `RewriteCanvas` in `vaultc::plan`; materialization/provenance/independent reconstruction in `vaultc::{compile,provenance,verify}` | all 5 tests in [`canvas_contract.rs`](../crates/vaultc/tests/canvas_contract.rs), including `canvas_references_resolve_rewrite_and_reparse`, ambiguity/waiver, unsafe escape, duplicate-node, duplicate-key, semantic-reseal, and missing-target cases; pipeline unknown-field check | implemented and locally verified for Document/Asset/Canvas/Base targets on macOS arm64; complete golden corpus and platform matrix pending |
| REQ-PAR-003 `.base` opaque preservation | [`vault-compilation-pipeline.md`](specs/vault-compilation-pipeline.md), ADR-0008 | `vaultc::{parse,plan,compile}` | `compiled_layout_excludes_raw_sources_and_preserves_v1_artifacts` checks warning and byte preservation | verified on macOS arm64 |
| REQ-DED-001 exact duplicate unification | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-DED-001 | `vaultc::{dedup,plan,provenance}` | `exact_duplicates_and_attachments_unify_with_complete_provenance` | verified on macOS arm64 |
| REQ-DED-002 near duplicate review only | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-DED-002 | bounded MinHash/LSH in `vaultc::dedup`; review conflicts in `vaultc::plan` | `dedup::tests::threshold_boundary_matches_v1_vectors` | implemented; full candidate vectors and non-auto-merge integration pending |
| REQ-CNF-001 deterministic conflict layout | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-CNF-001, ADR-0009, ADR-0010, ADR-0012 | `vaultc::{plan,approval,provenance}` including typed subjects and original-spelling-aware portable allocation | path-kind unit tests; NFC/NFD, full-fold, and allocated-suffix collision regressions; Canvas/Markdown ambiguity lifecycles | implemented; full title/alias/frontmatter/link corpus and typed resolution actions pending |
| REQ-PRV-001 output-to-source provenance | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-PRV-001, ADR-0010, ADR-0012 | `vaultc::{identity,generated,approval,provenance,compile,verify}` implements typed graph/envelope records, exact derivation, original/NFC path pairs, attribution, and bounded explanation | 9 typed-provenance and 4 generated-provenance cases; normalization path-pair/resealed-spelling attacks; exact dedup; directory/pack/package parity | implemented and locally verified on macOS arm64; cross-platform matrix, legal license interpretation, and detached authenticity remain out of scope |
| REQ-AI-001 provider-neutral interfaces | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md), ADR-0004, ADR-0011 | traits in `vaultc::provider`; projection, preflight, recording, and replay in `vaultc::augmentation`; schemas in `vaultc-protocol`; CLI `CommandProvider` | `capabilities_round_trip`; SDK live/local/remote/cancellation cases; strict subprocess schema and pre-disclosure tests | implemented; named multi-provider conformance suite pending |
| REQ-AI-002 explicit proposal approval | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md), ADR-0004, ADR-0011 | `vaultc::{provider,augmentation,approval,compile}` | explicit approval, stale/forged proposal, post-approval mutation, generated YAML injection, exact file/block evidence, empty-transcript rejection, and direct forged-validation rejection | implemented and locally verified for V1 proposal kinds |
| REQ-AI-003 record/replay determinism | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md), ADR-0004, ADR-0011 | canonical record codec, redaction/hydration, fresh replay validation, and transport preflight in `vaultc::augmentation`; CLI consumes the same owner; audit comparison in `vaultc::verify` | all 14 [`sdk_augmentation_replay_contract.rs`](../crates/vaultc/tests/sdk_augmentation_replay_contract.rs) cases; all 3 CLI replay cases; transcript reseal security test; SDK/CLI approved artifact and VaultPack byte parity | implemented and locally verified on macOS arm64; vendor conformance/platform matrix pending |
| REQ-CMP-001 atomic new-output compile | [`compiled-vault-and-vaultpack.md`](specs/compiled-vault-and-vaultpack.md), ALG-CNF-001, ADR-0003, ADR-0013, ADR-0014 | sibling-staged and independently verified directory compilation with target-native no-replace commit; verified sibling-staged pack publication; explicit cleanup/retention and post-commit states; `compile_with_options` in `vaultc::{compile,pack}` | 10 SDK atomic-directory cases; 9 directory fault/race unit cases; 11 SDK atomic-pack cases; 3 pack fault-order unit cases; 16 CLI lifecycle cases | directory and pack publication locally verified on macOS arm64; Linux/Windows execution and process-crash matrix pending |
| REQ-CMP-002 deterministic VaultPack | [`compiled-vault-and-vaultpack.md`](specs/compiled-vault-and-vaultpack.md), ADR-0004, ADR-0007, ADR-0010, ADR-0013 | deterministic tar/zstd writer, canonical outer-byte recreation/stream comparison, and atomic publisher in `vaultc::pack`; host-native CI matrix | 5 pack-format tests including literal complete-pack SHA-256 golden, plus `separate_and_compile_with_options_publish_byte_identical_results` and concurrent single-winner verification | verified on current host/toolchain; workflow defined but remote cross-platform execution and cross-version evidence pending |
| REQ-SEC-001 hostile input isolation | [`security-and-trust-boundaries.md`](specs/security-and-trust-boundaries.md), ADR-0003, ADR-0012, ADR-0013, ADR-0014 | `vaultc::{snapshot,parse,plan,provider,approval,compile,pack,verify}` and CLI guards; strict raw archives, complete accounting, verified no-clobber directory and pack publication | 18 security, 22 normalization/archive, 10 atomic-directory, 9 directory fault/race unit, 11 atomic-pack, and 3 pack fault-order cases; symlink, alias, race, provenance, Canvas, provider, and archive attacks | implemented; fuzz/property campaigns, Windows reparse evidence, and platform matrix pending |
| REQ-SDK-001 inspect-to-explain workflow | [`public-sdk-and-cli.md`](specs/public-sdk-and-cli.md), ADR-0001, ADR-0011, ADR-0013, ADR-0014 | `vaultc::VaultCompiler`, `compile_with_options`, `vaultc::augmentation`, `vaultc-cli`, `vaultc-protocol`; exhaustive nested error-family mapping | 16 CLI lifecycle/exits; 14 SDK augmentation/replay cases; 3 CLI replay cases; SDK/CLI plan, recording, approved artifact, atomic directory state, and VaultPack parity | implemented for the Rust/CLI V1 surface on macOS arm64; other language bindings and platform matrix pending |
| REQ-MCP-001 thin MCP adapter | [`mcp-adapter.md`](specs/mcp-adapter.md), ADR-0002 | future `vaultc-mcp` | future MCP contract tests | documented |
| REQ-OBS-001 generic review/install plugin | [`obsidian-plugin.md`](specs/obsidian-plugin.md), ADR-0002 | future generic plugin | future plugin E2E and permission tests | documented |
| REQ-PERF-001 bounded V1 workload | [`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md), ADR-0005 | hard limits, bounded candidate sets, SQLite workspace; large blobs still buffered | resource-limit and compression-ratio tests | implemented machinery; 100k-note/20 GB benchmark and RSS target unverified |
| REQ-MEM-001 experimental memory retrieval | [`algorithms/README.md`](algorithms/README.md), ALG-MEM-001..007 | future `vaultc-memory`; documentation only | calibration gates only | documented; excluded from default compiler |

## Current suite inventory

- `vaultc`: 27 unit, 5 Canvas, 10 atomic-directory-publication, 1
  documentation-integrity, 4 generated-provenance, 22 normalization/archive,
  5 pack, 11 atomic-pack-publication, 8 pipeline, 5 provider/approval, 14 SDK
  augmentation/replay, 18 security, and 9 typed-provenance tests;
- `vaultc-cli`: 13 unit, 3 augmentation/replay, and 16 lifecycle tests;
- `vaultc-protocol`: 2 unit tests;
- doctests: 0 examples, all harnesses pass.

See [`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md) for
release-wide gates that cannot be inferred from a green local suite.
