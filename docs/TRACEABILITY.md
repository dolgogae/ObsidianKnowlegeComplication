---
title: Requirements Traceability Matrix
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-09-05
decision_refs:
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
source_refs:
  - HIST-COMPILER-PLAN
---

# Requirements Traceability Matrix

This matrix describes the frozen V1/V2 readers and the `0.3.0` V3 development
slice. “Local” means macOS arm64 with the pinned Rust 1.97.1 toolchain; it is
not cross-platform release evidence.

| Requirement | Production implementation | Direct automated evidence | State |
|---|---|---|---|
| REQ-SNP-001 immutable inputs | `okc_core::{source,snapshot,compile}` | `immutable_source_bytes`, `source_bytes_permissions_and_modified_times_are_immutable`, stale-source and spelling cases | verified locally for bytes, Unix permissions, and modified time; supported-platform metadata matrix pending |
| REQ-SNP-002 V2 identities | `okc_core::{identity,snapshot,workspace}` with `okc:*:v2` domains, raw SHA-256, source-independent Vault content ID | identity unit tests, V2 literal Record/Evidence/Pack goldens | implemented locally; supported-platform literal matrix pending |
| REQ-SRC-001 MCP-origin neutrality | source descriptors contain only source ID, optional owner, path, and bytes; compiler has no MCP-origin field | `heterogeneous_mcp_style_vaults_are_origin_neutral_and_order_invariant`, absolute-root determinism and cache exclusions | verified locally across five heterogeneous styles; maximum-scale fixture pending |
| REQ-SRC-002 canonical source set | sorted snapshots plus duplicate whole-Vault content rejection | five-source `heterogeneous_mcp_style_vaults_are_origin_neutral_and_order_invariant`, two-source full-Pack order test, duplicate whole-Vault/source-ID cases | verified locally through five sources; reverse-order 10-Vault fixture pending |
| REQ-PAR-001 Markdown/IR | `okc_core::{parse,ir,plan}` including SectionId, heading paths, ordered blocks, callout/math/list/quote/code kinds and byte spans | parser and normalization suites | implemented locally; complete corpus/round-trip matrix pending |
| REQ-PAR-002 Canvas | typed nodes/edges/targets and unknown-field preservation in `okc_core::{ir,parse,plan}` | [`canvas_contract.rs`](../crates/okc-core/tests/canvas_contract.rs) | implemented locally |
| REQ-PAR-003 opaque Base | `okc_core::{parse,plan,compile}` | `compiled_layout_excludes_raw_sources_and_preserves_v2_artifacts` | verified locally |
| REQ-DED-001 exact dedup/provenance | `okc_core::{dedup,plan,provenance}` | `exact_duplicates_and_attachments_unify_with_complete_provenance` | verified locally |
| REQ-DED-002 near review only | bounded MinHash/LSH in `okc_core::dedup` | threshold vector and pipeline retention cases | implemented; complete vector set pending |
| REQ-CNF-001 deterministic conflicts | `okc_core::{plan,approval,provenance}` | path/case/NFC suffix, Markdown/Canvas ambiguity, stale/duplicate decision tests | implemented locally |
| REQ-MAT-001 typed immutable materialization | `okc_core::{approval,materialization,plan,compile,verify,provenance}` | `sealed_markdown_target_action_derives_and_verifies_one_materialization`, Canvas selected-target/unknown-field lifecycle, sealed-candidate rejection and semantic reseal tests | verified locally for Markdown and Canvas actions |
| REQ-PRV-001 typed provenance | `okc_core::{provenance,generated,verify}` | typed/generated provenance suites, directory/Pack explanation parity | verified locally; external authenticity intentionally excluded |
| REQ-AI-001 provider neutrality | `okc-protocol`, `okc_core::{provider,augmentation}`, `okc::command_provider` | capability, SDK replay, CLI replay/cancellation suites | implemented locally; vendor conformance pending |
| REQ-AI-002 untrusted proposals | schema/evidence validation and individual approvals in core | provider approval, generated provenance, stale/forged cases | verified locally for V2 proposal kinds |
| REQ-AI-003 record/replay | canonical four-record schema-2 JSONL in `okc_core::augmentation` | 14 SDK and 3 CLI replay cases | verified locally |
| REQ-AI-004 mandatory V3 AI integration | `okc-ai`, shared `okc_app::{integration_execution,ProviderService,IntegrationService}`, thin `okc::v3_cli` adapter, `okc_core::integration` | provider, schema, journal, approval/regeneration and provider-free compile unit/contract tests | partial: organizer/synthesis/critic/revision vertical slice works; command adapter, manual amendment, real provider smoke and hierarchical large-cluster flow pending |
| REQ-SEC-002 V3 disclosure gate | `okc_app::v3::{scan_sensitive_block,authorize_disclosure}`, bounded `okc_ai::UreqTransport` | scanner redaction/local-route, cancellation, URI credential, vendor shape and normalized error tests | partial: deterministic scanner/routing implemented; persisted hash-bound exceptions and broader scanner corpus pending |
| REQ-INT-001 complete taxonomy | shared integration service, TUI rename/path/move/merge/split review, and `okc_core::integration::TaxonomyProposal` | taxonomy exact-coverage, edited-rationale and portable organizer-schema tests | partial: complete assignment/reseal validation works; deterministic chunk/HNSW/candidate-union path is incomplete |
| REQ-INT-002 complete dispositions/evidence | `okc_core::integration` | singleton closure, missing disposition, forged evidence, uncited integrated block and omission approval cases | locally implemented for Markdown blocks/frontmatter values |
| REQ-INT-003 evidence and contradiction closure | `okc_core::integration::{SynthesisProposal,ContradictionSet}` | missing/foreign evidence, unsupported section, and contradiction-source validation tests | locally implemented for current Markdown integration records |
| REQ-INT-004 critic gate | `CriticReport`, `ClusterApproval`, shared integration review service and TUI three-pane individual review | stale critic, major/critical rejection, exact minor waiver and feedback revision validation | partial: gate and feedback regeneration implemented; manual-amendment and PTY evidence pending |
| REQ-INT-005 immutable approvals | application schema-4 append-only taxonomy/cluster/plan pointers and core approval hashes | source/taxonomy/revision invalidation, omission and waiver binding tests | partial: immutable current approvals and latest-plan lookup implemented; crash-injection E2E pending |
| REQ-INT-006 V3 materialization and provenance | `compile_approved_integration`, `verify_v3_directory`, `explain_v3_directory` | deterministic two-directory compile, missing recording/approval rejection, note/stub provenance | locally implemented for V3 directories; V3 Pack and non-Markdown carry-through pending |
| REQ-CMP-001 atomic publication | `okc_core::{compile,pack}` native no-replace publishers | directory/Pack fault, race, symlink, containment suites | verified on macOS arm64; Linux/Windows execution pending |
| REQ-CMP-002 deterministic OKCPack | `okc_core::pack`, profile `okc-tar-zstd-deterministic-v2` | literal complete-Pack SHA-256 and canonical outer-byte tests | verified on current host; remote matrix pending |
| REQ-CMP-003 V3/V2/V1 boundary | schema-3 writers, `okc-legacy-v2`, `okc-legacy-v1`, auto-detected verify/explain, `project upgrade --out` | frozen legacy goldens, legacy-reader unit, V3 verify/explain test, V2 source-binding upgrade test | partial: read-only V1/V2 artifact dispatch and non-mutating upgrade work, but development CLI V2 writer commands remain exposed for the regression harness |
| REQ-SEC-001 hostile inputs | strict snapshot/parser/control/provider/publication checks | security, normalization/archive, Canvas and publication suites | substantial local coverage; descriptor-relative opening, fuzz, Windows reparse pending |
| REQ-SDK-001 phased SDK/CLI | `OkcCompiler` and sole `okc` binary with all declared subcommands; schema-3 flow in the [`V3 integration guide`](../guide/v3-integration.md), frozen V2 regression in the [`Quickstart`](../guide/index.md), and command details in the [`CLI guide`](../guide/cli.md) | CLI lifecycle/exits, SDK parity, guide production build, repository-relative link test | partial: current V3 directory/TUI and frozen V2 flows are documented and pass locally; V3 Pack, provider-backed PTY and stable release evidence are pending |
| REQ-APP-001 project/TUI | `okc-app::{ProjectStore,IntegrationService}`, `okc::tui` V3 reducer/effects and terminal guard; [`TUI guide`](../guide/tui.md) | project layout/source-set/worker and TUI reducer/render/secret tests | implemented locally for the Markdown directory flow; provider-backed PTY, V1 import and four-platform evidence pending |
| REQ-APP-002 cwd workspace and worker | `okc_app::{WorkspaceBootstrap,IntegrationService,worker::Worker}` and ten-screen V3 TUI | explicit/0/1/multiple discovery, archive/symlink/nesting/limit/exclusion, source-set and bounded worker tests | implemented locally; crash injection and PTY/platform evidence remain |
| REQ-SEC-003 opaque credentials | `okc_app::ProviderService`, keyring 4.2.0 v1 adapter, zeroized/redacted secret types, provider-client secret injection | mock store lifecycle/locked/unavailable and serialization/Debug/fixed-mask tests | implemented locally; supported-platform native keychain CI remains |
| REQ-REL-001 release | [`../dist-workspace.toml`](../dist-workspace.toml), [`RELEASE.md`](RELEASE.md), axoupdater integration | updater target/unit compile checks; CI definition | blocked: no protected native-signing/notarization or remote release evidence |
| REQ-MCP-001 thin MCP adapter | future adapter contract only | future | documented; outside the current executable |
| REQ-OBS-001 generic plugin | future adapter contract only | future | documented; outside the current executable |
| REQ-PERF-001 100k/20GB | core limits, resource estimate, SQLite schema-2 migration | bounded resource/archive tests | not verified at reference workload; accepted files still buffered |
| REQ-MEM-001 experimental memory | research documents only | calibration gates only | excluded from default compiler |

## Current test locations

- V2 core: [`crates/okc-core/tests/`](../crates/okc-core/tests/)
- V2 CLI: [`crates/okc/tests/`](../crates/okc/tests/)
- application project/TUI units: [`crates/okc-app/src/lib.rs`](../crates/okc-app/src/lib.rs), [`crates/okc-app/src/integration_execution.rs`](../crates/okc-app/src/integration_execution.rs), [`crates/okc-app/src/workspace_bootstrap.rs`](../crates/okc-app/src/workspace_bootstrap.rs), [`crates/okc-app/src/provider_service.rs`](../crates/okc-app/src/provider_service.rs), [`crates/okc-app/src/worker.rs`](../crates/okc-app/src/worker.rs), and [`crates/okc/src/tui.rs`](../crates/okc/src/tui.rs)
- frozen V1 reader/goldens: [`crates/okc-legacy-v1/tests/`](../crates/okc-legacy-v1/tests/)
- V3 AI/providers: [`crates/okc-ai/src/lib.rs`](../crates/okc-ai/src/lib.rs)
- V3 journal/routing: [`crates/okc-app/src/v3.rs`](../crates/okc-app/src/v3.rs)
- V3 integration/materialization: [`crates/okc-core/src/integration.rs`](../crates/okc-core/src/integration.rs)

Release-wide gate status is authoritative in
[`CURRENT_STATE.md`](CURRENT_STATE.md) and
[`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md).
