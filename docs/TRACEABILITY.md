---
title: Requirements Traceability Matrix
status: normative
owners:
  - qa-security-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0019
  - ADR-0020
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-COMPILER-PLAN
---

# Requirements Traceability Matrix

This matrix describes the current Schema 3 `0.3.0` development tree. “Verified
locally” means macOS arm64 with Rust 1.97.1; it is not all-host release
evidence.

| Requirement | Production implementation | Direct automated evidence | State |
|---|---|---|---|
| REQ-SNP-001 immutable inputs | private source/snapshot path behind [`CorpusBuilder`](../crates/okc-core/src/corpus.rs) | `current_corpus_is_order_invariant_immutable_and_workspace_backed`; Python/Node source-immutability E2E | verified locally for bytes; full metadata/platform matrix pending |
| REQ-SNP-002 identities | [`identity.rs`](../crates/okc-core/src/identity.rs), snapshot and current integration domains | raw SHA-256 vectors, typed ID round trips, Rust/Python/Node shared artifact byte golden | existing Schema 3 IDs preserved locally |
| REQ-SRC-001 origin neutrality | `SourceSpec`, private snapshot/parser, no MCP-origin field | corpus order/exclusion test; source hygiene | verified locally |
| REQ-SRC-002 canonical source set | sorted snapshot/corpus sealing and duplicate content rejection | `current_corpus_is_order_invariant_immutable_and_workspace_backed`; `duplicate_vault_bytes_and_unsafe_archive_paths_fail_closed` | verified for bounded fixtures; 10-Vault scale pending |
| REQ-PAR-001 Markdown/frontmatter | private [`parse.rs`](../crates/okc-core/src/parse.rs) and [`ir.rs`](../crates/okc-core/src/ir.rs) | wikilink/code/fence, embed, ordinary-link span, duplicate-frontmatter tests; corpus E2E | implemented locally; broader corpus/fuzz pending |
| REQ-PAR-002 Canvas safety | private parser/IR path retained behind corpus construction | existing parser units and source fixtures | parsing retained; current materialization deliberately absent |
| REQ-PAR-003 opaque Base | private source/parser classification | source fixtures and current-output golden | ingestion retained; current materialization deliberately absent |
| REQ-DED-001 exact identity | [`dedup.rs`](../crates/okc-core/src/dedup.rs) and corpus sealing | threshold/reference and shared corpus/output byte tests | current deterministic identity retained |
| REQ-DED-002 proposal-only similarity | local candidate generation plus integration approval closure | semantic-candidate application tests and core closure tests | partial: scalable HNSW/union path pending |
| REQ-CNF-001 contradiction preservation | [`integration.rs`](../crates/okc-core/src/integration.rs) contradiction/evidence validators | complete singleton and forged-evidence cases; binding E2E | implemented for current Markdown records |
| REQ-MAT-001 approved materialization | `ApprovedIntegrationPlan` validation and [`compile`](../crates/okc-core/src/integration.rs) | missing recording/approval rejection; deterministic compile/verify | verified locally |
| REQ-PRV-001 provenance | `ProvenanceRecord`, compile/verify/explain in integration module | deterministic compile/verify/explain, forged evidence/stale critic, Python/Node E2E | verified for current Markdown notes/stubs |
| REQ-AI-001 provider neutrality | [`okc-ai`](../crates/okc-ai/src/lib.rs), app provider service | portable schema, all vendor-shape, local-boundary, error tests | locally implemented; real-provider matrix pending |
| REQ-AI-002 hostile proposals | integration validators and independent critic/approval gates | missing disposition, forged evidence, major finding and approval tests | verified locally |
| REQ-AI-003 recording/replay | project immutable objects/journal and integration execution | journal resume, response identity, cache/revision tests; binding E2E | implemented for current project flow |
| REQ-AI-004 mandatory integration | [`integration_execution.rs`](../crates/okc-app/src/integration_execution.rs), [`integration_service.rs`](../crates/okc-app/src/integration_service.rs) | portable pipeline schema, complete Python/Node workflows | partial: chunk/HNSW/hierarchical path pending |
| REQ-CMP-001 no-replace publication | integration `compile`, app output-resolution guard | deterministic double compile and output alias/no-clobber tests | verified on macOS arm64; platform/race matrix pending |
| REQ-CMP-002 future Pack | no current implementation or CLI surface | CLI help/parse absence and source-hygiene checks | correctly absent; future gate open |
| REQ-CMP-003 current-only schema | [`ArtifactService`](../crates/okc-app/src/artifact_service.rs), interop error map, CLI adapters | service/interop/CLI and Python/Node temporary-marker tests | current Schema 3 accepted; recognizable 1/2 explicit unsupported; no migration |
| REQ-SEC-001 hostile inputs | snapshot/archive/path parser, artifact detector, output publisher | corpus traversal/symlink tests; materialized tamper/symlink rejection; artifact symlink/mixed/malformed/oversized tests; provider/CLI/binding safety | substantial local coverage; fuzz/TOCTOU/platform gaps remain |
| REQ-SEC-002 disclosure | scanner/authorization in [`project_state.rs`](../crates/okc-app/src/project_state.rs) | category/location/hash-only and forced-local tests; binding consent failures | partial: persisted exception/broader corpus pending |
| REQ-SEC-003 credentials | [`provider_service.rs`](../crates/okc-app/src/provider_service.rs), redacted `okc-ai`, binding profile validation | keychain lifecycle/no-fallback, option/Debug/error redaction, missing-env tests | locally verified; native-platform keychain CI pending |
| REQ-INT-001 taxonomy | `TaxonomyProposal`, IntegrationService review | exact coverage, taxonomy approval/reseal, binding workflow | implemented locally; scalable candidate path pending |
| REQ-INT-002 dispositions | integration disposition validators | missing/duplicate disposition, integrated citation, omission approval tests | verified for Markdown blocks/metadata |
| REQ-INT-003 evidence/contradiction | synthesis and contradiction validators | foreign/forged evidence, preserved materialization, E2E output golden | verified locally |
| REQ-INT-004 critic | critic report and cluster approval/service | stale/major finding refusal, minor waiver and regeneration tests | gate implemented; manual amendment pending |
| REQ-INT-005 immutable approvals | [`project_state.rs`](../crates/okc-app/src/project_state.rs) append-only journal and integration plan objects | resume/invalidation/regeneration hash tests, core plan validation | implemented locally; crash injection pending |
| REQ-INT-006 directory output | integration compile/verify/explain | two-directory equality, current manifest/provenance, `sdk_fixture_inventory_matches_language_golden`, Python/Node shared inventory SHA-256 | verified for Markdown notes/stubs; non-Markdown and Pack pending |
| REQ-SDK-001 current Rust/CLI | [`lib.rs`](../crates/okc-core/src/lib.rs), [`main.rs`](../crates/okc/src/main.rs), [`commands.rs`](../crates/okc/src/commands.rs) | [`cli_contract.rs`](../crates/okc/tests/cli_contract.rs), corpus builder test, help/parse absence | implemented current-only surface |
| REQ-SDK-002 Python/Node | [`okc-interop`](../crates/okc-interop/src/lib.rs), [Python adapter](../bindings/python/okc/__init__.py), [Node adapter](../bindings/node/index.cjs) | Python/Node public API/E2E/typing tests, schema-2 DTO and byte golden, package smoke | locally implemented; remote native/version matrix pending |
| REQ-APP-001 CLI/TUI sharing | `okc-app`, [`tui.rs`](../crates/okc/src/tui.rs), thin CLI commands | project layout and TUI reducer/render/secret tests | implemented locally; provider-backed PTY pending |
| REQ-APP-002 cwd/worker | [`workspace_bootstrap.rs`](../crates/okc-app/src/workspace_bootstrap.rs), [`worker.rs`](../crates/okc-app/src/worker.rs) | 0/1/multiple discovery, nested/managed/limit, worker cancellation/barrier | verified locally; crash/platform evidence pending |
| REQ-REL-001 release | [`dist-workspace.toml`](../dist-workspace.toml), [`RELEASE.md`](RELEASE.md), SDK CI | local checks and package smoke | blocked on remote matrix, QG-006, signing/publication |
| REQ-MCP-001 adapter | [`mcp-adapter.md`](specs/mcp-adapter.md) | none | future only |
| REQ-OBS-001 plugin | [`obsidian-plugin.md`](specs/obsidian-plugin.md) | none | future only |
| REQ-PERF-001 scale | bounded limits and workspace persistence | bounded unit fixtures only | 100k/20 GB gate not run |
| REQ-MEM-001 experiments | algorithm registry/experimental docs | promotion checklist only | excluded from default path |

## Current test locations

- Corpus/snapshot/parser/integration: [`crates/okc-core/src/`](../crates/okc-core/src/) and [`corpus_builder_contract.rs`](../crates/okc-core/tests/corpus_builder_contract.rs)
- Cross-language output bytes: [`sdk_output_golden.rs`](../crates/okc-core/tests/sdk_output_golden.rs) and its current approved-plan fixture
- Documentation links: [`documentation_contract.rs`](../crates/okc-core/tests/documentation_contract.rs)
- Providers: [`crates/okc-ai/src/lib.rs`](../crates/okc-ai/src/lib.rs)
- Projects, journal, services, artifact safety, worker, and TUI foundation: [`crates/okc-app/src/`](../crates/okc-app/src/)
- CLI surface: [`cli_contract.rs`](../crates/okc/tests/cli_contract.rs)
- Interop DTOs/jobs/errors: [`crates/okc-interop/src/lib.rs`](../crates/okc-interop/src/lib.rs)
- Python public API and typing: [`bindings/python/tests/`](../bindings/python/tests/)
- Node.js public API and declarations: [`bindings/node/tests/`](../bindings/node/tests/)
- SDK native package matrix: [`.github/workflows/sdk-bindings.yml`](../.github/workflows/sdk-bindings.yml)

## Current-only archive evidence

`git ls-remote --tags origin 'archive/v0.*'` MUST show peeled commits
`7181fc2dea54288f176b66a00e2335da7f58bdfd` and
`b9f9e88bc531095fbeb2ece4155c980bdf10708b` for `archive/v0.1.0` and
`archive/v0.2.0`. These tags provide source recoverability only; they are not
runtime compatibility or release evidence.

Release-wide gate status is authoritative in
[`CURRENT_STATE.md`](CURRENT_STATE.md) and
[`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md).
