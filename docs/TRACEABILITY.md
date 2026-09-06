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
locally” means macOS arm64 with Rust 1.97.1. The follow-up also runs the Rust
suite on Linux x86_64 GNU under Docker emulation; neither is all-host release
evidence.

| Requirement | Production implementation | Direct automated evidence | State |
|---|---|---|---|
| REQ-SNP-001 immutable inputs | private source/snapshot path and pinned [`source_io.rs`](../crates/okc-core/src/source_io.rs) behind [`CorpusBuilder`](../crates/okc-core/src/corpus.rs) | corpus immutability, source-handle and hardlink regressions; Python/Node and PTY source-immutability E2E | local bytes/selected metadata cases pass; full platform/race matrix pending |
| REQ-SNP-002 identities | [`identity.rs`](../crates/okc-core/src/identity.rs), snapshot and current integration domains | raw SHA-256 vectors, typed ID round trips, Rust/Python/Node shared artifact byte golden | existing Schema 3 IDs preserved locally |
| REQ-SRC-001 origin neutrality | `SourceSpec`, private snapshot/parser, no MCP-origin field | corpus order/exclusion test; source hygiene | verified locally |
| REQ-SRC-002 canonical source set | sorted snapshot/corpus sealing, sealed membership policy and duplicate content rejection | corpus order/ignore properties; unsafe archives; 10-Vault/100k-note ingestion probe | bounded properties and ingestion probe pass; full semantic scale pending |
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
| REQ-CMP-001 no-replace publication | integration `compile`, stage lifetime/tree sync and app output-resolution guard | deterministic failure checkpoints and real publish-barrier collisions, including two distinct concurrent plans | macOS and emulated Linux regressions pass; ancestor/crash/platform matrix pending |
| REQ-CMP-002 future Pack | no current implementation or CLI surface | CLI help/parse absence and source-hygiene checks | correctly absent; future gate open |
| REQ-CMP-003 current-only schema | [`ArtifactService`](../crates/okc-app/src/artifact_service.rs), interop error map, CLI adapters | service/interop/CLI and Python/Node temporary-marker tests | current Schema 3 accepted; recognizable 1/2 explicit unsupported; no migration |
| REQ-SEC-001 hostile inputs | snapshot/archive/path parser, artifact detector, output publisher | corpus traversal/symlink tests; materialized tamper/symlink rejection; artifact symlink/mixed/malformed/oversized tests; provider/CLI/binding safety | substantial local coverage; fuzz/TOCTOU/platform gaps remain |
| REQ-SEC-002 disclosure | scanner/authorization in [`project_state.rs`](../crates/okc-app/src/project_state.rs) | category/location/hash-only and forced-local tests; binding consent failures | partial: persisted exception/broader corpus pending |
| REQ-SEC-003 credentials | [`provider_service.rs`](../crates/okc-app/src/provider_service.rs), redacted `okc-ai`, binding profile validation | keychain lifecycle/no-fallback, option/Debug/error redaction, missing-env tests | locally verified; native-platform keychain CI pending |
| REQ-INT-001 taxonomy | `TaxonomyProposal`, IntegrationService review | exact coverage, taxonomy approval/reseal, binding workflow | implemented locally; scalable candidate path pending |
| REQ-INT-002 dispositions | integration disposition validators | missing/duplicate disposition, integrated citation, omission approval tests | verified for Markdown blocks/metadata |
| REQ-INT-003 evidence/contradiction | synthesis and contradiction validators | foreign/forged evidence, preserved materialization, E2E output golden | verified locally |
| REQ-INT-004 critic | critic report and cluster approval/service | stale/major finding refusal, minor waiver and regeneration tests | gate implemented; manual amendment pending |
| REQ-INT-005 immutable approvals | [`project_state.rs`](../crates/okc-app/src/project_state.rs) append-only journal, configuration commit order and integration plan objects | resume/invalidation/regeneration hashes, manifest/journal fault injection, core plan validation | caught failures fail closed; process-crash and cross-store recovery pending |
| REQ-INT-006 directory output | integration compile/verify/explain | two-directory equality, current manifest/provenance, `sdk_fixture_inventory_matches_language_golden`, Python/Node shared inventory SHA-256 | verified for Markdown notes/stubs; non-Markdown and Pack pending |
| REQ-SDK-001 current Rust/CLI | [`lib.rs`](../crates/okc-core/src/lib.rs), [`main.rs`](../crates/okc/src/main.rs), [`commands.rs`](../crates/okc/src/commands.rs) | [`cli_contract.rs`](../crates/okc/tests/cli_contract.rs), corpus builder test, help/parse absence | implemented current-only surface |
| REQ-SDK-002 Python/Node | [`okc-interop`](../crates/okc-interop/src/lib.rs), [Python adapter](../bindings/python/okc/__init__.py), [Node adapter](../bindings/node/index.cjs) | Python/Node public API/E2E/typing tests, schema-2 DTO and byte golden, package smoke | locally implemented; remote native/version matrix pending |
| REQ-APP-001 CLI/TUI sharing | `okc-app`, [`tui.rs`](../crates/okc/src/tui.rs), thin CLI commands | reducer/render/secret tests; [`tui_pty_smoke.py`](../tests/tui_pty_smoke.py) complete synthetic-provider approval/compile/verify/restart | local POSIX PTY passes; long cancellation, real providers and all-host coverage pending |
| REQ-APP-002 cwd/worker | [`workspace_bootstrap.rs`](../crates/okc-app/src/workspace_bootstrap.rs), [`worker.rs`](../crates/okc-app/src/worker.rs) | 0/1/multiple discovery, nested/managed/limit, worker cancellation/barrier | verified locally; crash/platform evidence pending |
| REQ-REL-001 release | [`dist-workspace.toml`](../dist-workspace.toml), [`RELEASE.md`](RELEASE.md), SDK CI | local checks and package smoke | blocked on remote matrix, QG-006, signing/publication |
| REQ-MCP-001 adapter | [`mcp-adapter.md`](specs/mcp-adapter.md) | none | future only |
| REQ-OBS-001 plugin | [`obsidian-plugin.md`](specs/obsidian-plugin.md) | none | future only |
| REQ-PERF-001 scale | bounded limits/workspace persistence; obsolete corpus inspection dropped | [`corpus_probe.rs`](../crates/okc-core/examples/corpus_probe.rs) 10-Vault/100k-note/25.6-MB ingestion: 174.569 s, 5.36 GB RSS | diagnostic only; full 20 GB semantic workload and QG-006 not passed |
| REQ-MEM-001 experiments | algorithm registry/experimental docs | promotion checklist only | excluded from default path |

## Correctness audit regression mapping — 2026-09-06

These extend the matrix above without declaring the remaining release gates
complete. Findings and exact validation scope are recorded in the
[correctness audit](history/summaries/2026-09-06-correctness-audit.md).
`project_routes_use_default_and_validate_language` additionally requires rejected
default/role-specific route edits to leave the prior configuration unchanged.

| Requirement / algorithm | Changed implementation | New direct regression evidence |
|---|---|---|
| REQ-SNP-001, REQ-SEC-001 / ALG-SNP-001 | core `workspace::validate_destination`, project managed-path/object guards | `workspace_cannot_be_created_inside_a_source`; `workspace_rejects_source_aliases_and_symlinked_databases`; `project_open_rejects_symlinked_managed_paths_before_writing`; `object_access_rejects_replaced_directories_and_object_symlinks` |
| REQ-SRC-001, REQ-INT-002 / ALG-INT-001 | strict `SourceId` deserialization; corpus metadata slot validation | `source_deserialization_cannot_bypass_identifier_or_field_validation`; `metadata_slots_cannot_contain_conflicting_values` |
| REQ-MAT-001, REQ-INT-006 / ALG-CNF-001 | portable full-path/prefix registry and separate body validator | `integration_paths_reject_portable_hazards_without_rewriting_source_spelling`; `taxonomy_rejects_unicode_and_file_directory_path_collisions`; `multiline_markdown_sections_compile_without_changing_their_body` |
| REQ-PRV-001, REQ-SEC-001 / ALG-PRV-001 | plan-derived bounded inventory and no-follow root checks | `verification_rejects_unplanned_files_even_with_resealed_inventory`; `core_verification_rejects_a_symlinked_artifact_root` |
| REQ-INT-005 / ALG-INT-001 | fresh configuration runs; current approval/regeneration checks; verified-output plan binding | `changed_configuration_invalidates_approved_authority_immediately`; `changed_taxonomy_and_pending_regeneration_invalidate_stored_plans`; `verified_outputs_are_bound_to_the_current_approved_plan` |
| REQ-AI-003 / ALG-INT-001 | provider/candidate cache identities and event ordering | `provider_cache_keys_bind_endpoint_kind_and_bounds_without_credentials`; expanded `journal_resumes_complete_tasks_without_deleting_history` stage/request mismatch cases |
| REQ-SEC-002 | shared versioned block/metadata scanner and disclosure gate | `scanner_detects_pem_keys_and_preserves_exact_email_byte_ranges`; `metadata_only_sensitive_documents_force_local_routes_without_recording_secrets` |
| REQ-AI-001/002, REQ-SEC-001/002 | parsed loopback IPs, shared strict JSON decoder, total deadline | `loopback_names_cannot_be_spoofed_by_dns_prefixes`; `provider_json_rejects_duplicate_keys_including_nested_structured_output`; `retry_after_cannot_exceed_the_total_provider_deadline` |
| REQ-APP-002, REQ-SDK-002 | independent completion slot, bounded SDK work queue and shared reservations | `a_full_progress_queue_cannot_block_worker_shutdown`; `scheduler_queue_is_bounded_and_drop_does_not_wait_for_a_full_queue`; `distinct_clients_share_project_reservations_and_release_before_completion` |
| REQ-SDK-002 | Node DTO conversion preserves arbitrary source maps | `camelCase DTO conversion preserves arbitrary source metadata keys`; fresh native Python/Node E2E byte golden |
| REQ-CMP-001, REQ-SDK-001 | absent parent handling; optional core module/import/helper gates | `offline_compile_accepts_a_single_component_relative_destination`; no-default/archives-only/sqlite-only library Clippy checks |

## Stabilization follow-up regression mapping — 2026-09-06

The [follow-up report](history/summaries/2026-09-06-stabilization-follow-up.md)
records 20 additional Rust tests, both local OS runs, installed distribution
tests, deterministic mutation smoke, and explicitly unqualified gates.

| Requirement / algorithm | Changed implementation | New direct regression evidence |
|---|---|---|
| REQ-SNP-001, REQ-SEC-001 / ALG-SNP-001 | Unix managed-file/object/SQLite link-count guards | `workspace_rejects_hardlinked_database_and_sidecars_before_writing`; `project_open_rejects_hardlinked_managed_files_before_writing`; `object_access_rejects_hardlinked_objects` |
| REQ-SNP-001, REQ-SEC-001 / ALG-SNP-001 | Linux/macOS pinned source handles and relative no-follow opens | `pinned_source_root_does_not_follow_a_replacement_alias`; `pinned_source_rejects_replaced_leaf_and_ancestor_symlinks`; `source_root_and_archive_symlinks_are_rejected` |
| REQ-SRC-002 / ALG-SNP-001 | source membership independent of parent/global/Vault ignore files | `ambient_ignore_files_cannot_change_source_membership`; `corpus_creation_order_and_absolute_roots_do_not_change_sealed_bytes` |
| REQ-INT-005 / ALG-INT-001 | invalidate journal before manifest persist; in-memory commit after success | `failed_invalidation_keeps_manifest_and_in_memory_configuration_unchanged`; `failed_manifest_replacement_leaves_previous_approvals_invalidated`; `failed_source_manifest_replacement_cannot_resurrect_previous_plan` |
| REQ-CMP-001, REQ-SEC-001 | bottom-up stage directory sync, private checkpoints and explicit cleanup failure | `publication_checkpoint_order_and_precommit_cleanup_are_explicit`; `staged_verification_failure_never_publishes_or_leaves_a_stage`; `failed_stage_cleanup_reports_exact_path_and_both_causes`; `postcommit_failure_preserves_verified_output_and_disarms_old_stage` |
| REQ-CMP-001 / ALG-CNF-001 | guard through real no-replace publication | `publish_barrier_preserves_external_file_and_directory_winners`; `publish_barrier_preserves_live_and_dangling_symlink_winners`; `concurrent_distinct_plans_publish_exactly_one_unmixed_winner` |
| REQ-SEC-001, QG-004 | strict JSON and bounded archive entry points | `strict_json_fixed_seed_mutations_never_panic_or_disagree_on_accepted_values`; `zip_fixed_seed_header_and_truncation_mutations_are_bounded` (mutation smoke, not coverage-guided fuzz) |
| REQ-APP-001/002, REQ-AI-003, REQ-SNP-001 | existing TUI workflow, new POSIX PTY CI harness | synthetic loopback, four provider calls, explicit approvals, independent verify/restart, source SHA/mtime and terminal restoration assertions |
| REQ-SDK-002, REQ-REL-001, QG-008 | existing packaging plus epoch-controlled wheel CI | fresh wheel/sdist virtualenvs, 12 Python tests each, mypy; clean npm tarballs/CJS/ESM, 13 Node tests, TypeScript; repeated wheel bytes, SBOM/checksums |

## Current test locations

- Corpus/snapshot/parser/integration: [`crates/okc-core/src/`](../crates/okc-core/src/) and [`corpus_builder_contract.rs`](../crates/okc-core/tests/corpus_builder_contract.rs)
- Cross-language output bytes: [`sdk_output_golden.rs`](../crates/okc-core/tests/sdk_output_golden.rs) and its current approved-plan fixture
- Documentation links: [`documentation_contract.rs`](../crates/okc-core/tests/documentation_contract.rs)
- Fixed-seed mutation/properties: [`adversarial_contract.rs`](../crates/okc-core/tests/adversarial_contract.rs)
- Providers: [`crates/okc-ai/src/lib.rs`](../crates/okc-ai/src/lib.rs)
- Projects, journal, services, artifact safety, worker, and TUI foundation: [`crates/okc-app/src/`](../crates/okc-app/src/)
- CLI surface: [`cli_contract.rs`](../crates/okc/tests/cli_contract.rs)
- POSIX TUI workflow: [`tests/tui_pty_smoke.py`](../tests/tui_pty_smoke.py)
- Synthetic ingestion measurement: [`corpus_probe.rs`](../crates/okc-core/examples/corpus_probe.rs)
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
