---
title: Requirements Traceability Matrix
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0015
  - ADR-0016
  - ADR-0017
  - ADR-0018
  - ADR-0019
  - ADR-0020
  - ADR-0021
source_refs:
  - HIST-COMPILER-PLAN
---

# Requirements Traceability Matrix

This matrix describes the `0.2.0` V2 tree. “Local” means macOS arm64 with the
pinned Rust 1.97.1 toolchain; it is not cross-platform release evidence.

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
| REQ-CMP-001 atomic publication | `okc_core::{compile,pack}` native no-replace publishers | directory/Pack fault, race, symlink, containment suites | verified on macOS arm64; Linux/Windows execution pending |
| REQ-CMP-002 deterministic OKCPack | `okc_core::pack`, profile `okc-tar-zstd-deterministic-v2` | literal complete-Pack SHA-256 and canonical outer-byte tests | verified on current host; remote matrix pending |
| REQ-CMP-003 V2/V1 boundary | V2 writers plus `okc-legacy-v1` dispatch in `okc verify/explain` | all frozen legacy core goldens plus V2 schema/domain goldens | read-only artifact compatibility implemented; project migration UI pending |
| REQ-SEC-001 hostile inputs | strict snapshot/parser/control/provider/publication checks | security, normalization/archive, Canvas and publication suites | substantial local coverage; descriptor-relative opening, fuzz, Windows reparse pending |
| REQ-SDK-001 phased SDK/CLI | `OkcCompiler` and sole `okc` binary with all declared subcommands | CLI lifecycle/exits and SDK parity | implemented locally |
| REQ-APP-001 project/TUI | `okc-app::ProjectStore`, progress/cancellation types; `okc::tui` reducer and terminal guard | project layout/lock/rebind and TUI reducer/render tests | partial: storage and shell implemented; real worker effects, PTY flows and V1 import pending |
| REQ-REL-001 release | [`../dist-workspace.toml`](../dist-workspace.toml), [`RELEASE.md`](RELEASE.md), axoupdater integration | updater target/unit compile checks; CI definition | blocked: no protected native-signing/notarization or remote release evidence |
| REQ-MCP-001 thin MCP adapter | future adapter contract only | future | documented; out of V2 executable |
| REQ-OBS-001 generic plugin | future adapter contract only | future | documented; out of V2 executable |
| REQ-PERF-001 100k/20GB | core limits, resource estimate, SQLite schema-2 migration | bounded resource/archive tests | not verified at reference workload; accepted files still buffered |
| REQ-MEM-001 experimental memory | research documents only | calibration gates only | excluded from default compiler |

## Current test locations

- V2 core: [`crates/okc-core/tests/`](../crates/okc-core/tests/)
- V2 CLI: [`crates/okc/tests/`](../crates/okc/tests/)
- application project/TUI units: [`crates/okc-app/src/lib.rs`](../crates/okc-app/src/lib.rs) and [`crates/okc/src/tui.rs`](../crates/okc/src/tui.rs)
- frozen V1 reader/goldens: [`crates/okc-legacy-v1/tests/`](../crates/okc-legacy-v1/tests/)

Release-wide gate status is authoritative in
[`CURRENT_STATE.md`](CURRENT_STATE.md) and
[`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md).
