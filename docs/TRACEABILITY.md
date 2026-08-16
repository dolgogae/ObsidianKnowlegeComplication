---
title: Requirements Traceability Matrix
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-08-17
decision_refs:
  - ADR-0001
  - ADR-0004
  - ADR-0009
  - ADR-0010
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
| REQ-SNP-001 immutable inputs | [`vault-compilation-pipeline.md`](specs/vault-compilation-pipeline.md), ALG-SNP-001, ADR-0003 | `vaultc::{source,snapshot,compile}` | `immutable_source_bytes`; `source_change_after_plan_fails_without_publishing`; `sealed_plan_operations_cannot_change_before_materialization` in [`pipeline_contract.rs`](../crates/vaultc/tests/pipeline_contract.rs) | implemented; metadata/permission/timestamp preservation is not directly tested |
| REQ-SNP-002 stable content identity | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md), ALG-SNP-001 | `vaultc::{identity,snapshot}` | identity unit tests; `inspect_plan_and_compile_are_deterministic`; `absolute_source_locations_do_not_affect_artifact_or_vaultpack_bytes` in [`pack_contract.rs`](../crates/vaultc/tests/pack_contract.rs) | implemented; cross-platform golden ID matrix pending |
| REQ-PAR-001 Obsidian Markdown parsing | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md), ALG-NRM-001, ADR-0008 | `vaultc::{parse,ir,plan,compile,verify}`; planner-derived recipes and expected hashes; compiler source-slice/hash checks; verifier reverse reconstruction and semantic reparse | parser span/link/frontmatter unit tests; `resolved_links_are_rewritten_without_touching_inline_code`; `verifier_rejects_resealed_markdown_rewrite_output_that_disagrees_with_expected_output_hash`; malformed UTF-8 security test | implemented; complete normative Markdown golden corpus and platform matrix pending |
| REQ-PAR-002 JSON Canvas parsing and rewrite | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md), ALG-NRM-001, ADR-0008 | strict/unknown-preserving parsing in `vaultc::parse`; typed reference state/targets in `vaultc::ir`; output maps, conflicts, and `RewriteCanvas` in `vaultc::plan`; materialization/provenance/independent reconstruction in `vaultc::{compile,provenance,verify}` | all 5 tests in [`canvas_contract.rs`](../crates/vaultc/tests/canvas_contract.rs), including `canvas_references_resolve_rewrite_and_reparse`, ambiguity/waiver, unsafe escape, duplicate-node, duplicate-key, semantic-reseal, and missing-target cases; pipeline unknown-field check | implemented and locally verified for Document/Asset/Canvas/Base targets on macOS arm64; complete golden corpus and platform matrix pending |
| REQ-PAR-003 `.base` opaque preservation | [`vault-compilation-pipeline.md`](specs/vault-compilation-pipeline.md), ADR-0008 | `vaultc::{parse,plan,compile}` | `compiled_layout_excludes_raw_sources_and_preserves_v1_artifacts` checks warning and byte preservation | verified on macOS arm64 |
| REQ-DED-001 exact duplicate unification | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-DED-001 | `vaultc::{dedup,plan,provenance}` | `exact_duplicates_and_attachments_unify_with_complete_provenance` | verified on macOS arm64 |
| REQ-DED-002 near duplicate review only | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-DED-002 | bounded MinHash/LSH in `vaultc::dedup`; review conflicts in `vaultc::plan` | `dedup::tests::threshold_boundary_matches_v1_vectors` | implemented; full candidate vectors and non-auto-merge integration pending |
| REQ-CNF-001 deterministic conflict layout | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-CNF-001, ADR-0009, ADR-0010 | `vaultc::{plan,approval,provenance}` including typed Markdown-link and Canvas-reference conflict subjects | path-kind unit tests; `portable_path_collisions_are_stable_and_typed`; Canvas ambiguity lifecycle; `markdown_and_canvas_waivers_bind_distinct_typed_subjects`; conflict decision and CLI required-conflict tests | implemented; full title/alias/frontmatter/link corpus and typed resolution actions pending |
| REQ-PRV-001 output-to-source provenance | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md), ALG-PRV-001, ADR-0010 | `vaultc::{identity,generated,approval,provenance,compile,verify}` implements typed RecordIds, source/operation/decision/proposal/approval/output/edge records, exact content reconstruction, frontmatter attribution retention, the stored graph, virtual audit envelope, and bounded explanation pages | all 9 [`typed_provenance_contract.rs`](../crates/vaultc/tests/typed_provenance_contract.rs) cases; all 4 generated-provenance cases; exact dedup; fully resealed graph attacks; directory/pack/package pagination parity | implemented and locally verified on macOS arm64; cross-platform matrix, legal license interpretation, and detached authenticity remain out of scope |
| REQ-AI-001 provider-neutral interfaces | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md), ADR-0004 | traits/validation in `vaultc::provider`; schemas in `vaultc-protocol`; CLI `CommandProvider` | `capabilities_round_trip`; redacted subprocess round trip; bounded reader/cancellation tests | implemented; multi-provider conformance suite pending |
| REQ-AI-002 explicit proposal approval | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md), ADR-0004 | `vaultc::{provider,approval,compile}` | explicit approval, stale/forged proposal, post-approval mutation, generated YAML injection, exact file/block evidence policy, and snapshot-bound provider lifecycle tests | implemented; broader provider mismatch matrix pending |
| REQ-AI-003 record/replay determinism | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md), ADR-0004 | transcript capture/redaction/hydration in CLI and `vaultc::approval`; audit comparison in `vaultc::verify` | `cli_round_trips_a_redacted_command_provider_transcript`; `verifier_rejects_transcript_audit_mismatch_even_when_artifact_is_resealed` | implemented; transcript replay output-equality test pending |
| REQ-CMP-001 atomic new-output compile | [`compiled-vault-and-vaultpack.md`](specs/compiled-vault-and-vaultpack.md), ALG-CNF-001, ADR-0003 | sibling staging/fsync/rename in `vaultc::compile`; sealed Copy/Markdown/Canvas output commitments in `vaultc::{plan,verify}`; CLI no-clobber pack publication | existing-destination, stale-source/no-publish, sealed-plan tamper, `verifier_rejects_resealed_copy_output_that_disagrees_with_plan`, Markdown rewrite reseal, and CLI lifecycle tests | implemented; crash/fault-injection interruption test pending |
| REQ-CMP-002 deterministic VaultPack | [`compiled-vault-and-vaultpack.md`](specs/compiled-vault-and-vaultpack.md), ADR-0004, ADR-0010 | deterministic tar/zstd writer plus canonical outer-byte recreation/stream comparison in `vaultc::pack` | all 5 tests in [`pack_contract.rs`](../crates/vaultc/tests/pack_contract.rs), including alternate encoding, sealed compression level, expansion ratio, determinism, and source-location independence | verified on current host/toolchain; cross-version/platform contract pending |
| REQ-SEC-001 hostile input isolation | [`security-and-trust-boundaries.md`](specs/security-and-trust-boundaries.md), ADR-0003 | `vaultc::{snapshot,parse,plan,provider,approval,compile,pack,verify}` and CLI process/control-file guards | 18 cases in [`security_contract.rs`](../crates/vaultc/tests/security_contract.rs); typed provenance schema/semantic reseal cases; Canvas duplicate-key/node and escape cases; provider approval/cancellation; VaultPack canonical and streamed expansion-ratio tests | implemented; fuzz/property campaigns and platform matrix pending |
| REQ-SDK-001 inspect-to-explain workflow | [`public-sdk-and-cli.md`](specs/public-sdk-and-cli.md), ADR-0001 | `vaultc::VaultCompiler`, `vaultc-cli`, `vaultc-protocol`; exhaustive plan error-family mapping | `cli_plan_matches_sdk_and_full_artifact_lifecycle`; `approve_rejects_pre_hash_markdown_plan_as_decision_error`; CLI exit/conflict/provider tests | implemented partially; SDK has provider traits and validation but no façade `augment()`/projection builder |
| REQ-MCP-001 thin MCP adapter | [`mcp-adapter.md`](specs/mcp-adapter.md), ADR-0002 | future `vaultc-mcp` | future MCP contract tests | documented |
| REQ-OBS-001 generic review/install plugin | [`obsidian-plugin.md`](specs/obsidian-plugin.md), ADR-0002 | future generic plugin | future plugin E2E and permission tests | documented |
| REQ-PERF-001 bounded V1 workload | [`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md), ADR-0005 | hard limits, bounded candidate sets, SQLite workspace; large blobs still buffered | resource-limit and compression-ratio tests | implemented machinery; 100k-note/20 GB benchmark and RSS target unverified |
| REQ-MEM-001 experimental memory retrieval | [`algorithms/README.md`](algorithms/README.md), ALG-MEM-001..007 | future `vaultc-memory`; documentation only | calibration gates only | documented; excluded from default compiler |

## Current suite inventory

- `vaultc`: 15 unit, 5 Canvas, 4 generated-provenance, 5 pack, 8 pipeline, 5
  provider/approval, 18 security, and 9 typed-provenance tests;
- `vaultc-cli`: 10 unit and 6 integration tests;
- `vaultc-protocol`: 2 unit tests;
- doctests: 0 examples, all harnesses pass.

See [`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md) for
release-wide gates that cannot be inferred from a green local suite.
