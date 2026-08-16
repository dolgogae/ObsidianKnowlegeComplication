---
title: Requirements Traceability Matrix
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0004
source_refs:
  - HIST-COMPILER-PLAN
---

# Requirements Traceability Matrix

Implementation paths and test names are targets until code exists. Update this file with every behavior change.

| Requirement | Contract | Algorithm/decision | Planned component | Required verification | State |
|---|---|---|---|---|---|
| REQ-SNP-001 immutable inputs | [`vault-compilation-pipeline.md`](specs/vault-compilation-pipeline.md) | ALG-SNP-001, ADR-0003 | `vaultc::snapshot` | `immutable_source_bytes` | documented |
| REQ-SNP-002 stable content identity | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md) | ALG-SNP-001 | `vaultc::identity` | `cross_platform_snapshot_id` | documented |
| REQ-PAR-001 Obsidian Markdown parsing | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md) | ALG-NRM-001, ADR-0008 | `vaultc::parse` | Markdown golden corpus | documented |
| REQ-PAR-002 JSON Canvas parsing | [`canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md) | ALG-NRM-001, ADR-0008 | `vaultc::canvas` | Canvas golden corpus | documented |
| REQ-PAR-003 `.base` opaque preservation | [`vault-compilation-pipeline.md`](specs/vault-compilation-pipeline.md) | ADR-0008 | `vaultc::artifact` | `base_preserved_with_warning` | documented |
| REQ-DED-001 exact duplicate unification | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md) | ALG-DED-001 | `vaultc::dedup` | exact-duplicate vectors | documented |
| REQ-DED-002 near duplicate review only | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md) | ALG-DED-002 | `vaultc::dedup` | threshold/boundary vectors | documented |
| REQ-CNF-001 deterministic conflict layout | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md) | ALG-CNF-001 | `vaultc::planner` | path/case/title conflict corpus | documented |
| REQ-PRV-001 output-to-source provenance | [`provenance-and-conflicts.md`](specs/provenance-and-conflicts.md) | ALG-PRV-001 | `vaultc::provenance` | provenance closure property | documented |
| REQ-AI-001 provider-neutral interfaces | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md) | ADR-0004 | `vaultc-protocol` | provider conformance suite | documented |
| REQ-AI-002 explicit proposal approval | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md) | ADR-0004 | `vaultc::approval` | malicious/stale proposal tests | documented |
| REQ-AI-003 record/replay determinism | [`ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md) | ADR-0004 | `vaultc-protocol` | transcript replay equality | documented |
| REQ-CMP-001 atomic new-output compile | [`compiled-vault-and-vaultpack.md`](specs/compiled-vault-and-vaultpack.md) | ALG-CNF-001, ADR-0003 | `vaultc::compile` | interruption/atomicity tests | documented |
| REQ-CMP-002 deterministic VaultPack | [`compiled-vault-and-vaultpack.md`](specs/compiled-vault-and-vaultpack.md) | ADR-0004 | `vaultc::pack` | byte equality across runs | documented |
| REQ-SEC-001 hostile input isolation | [`security-and-trust-boundaries.md`](specs/security-and-trust-boundaries.md) | ADR-0003 | scanner/parser/compiler | traversal, symlink, archive bomb suite | documented |
| REQ-SDK-001 inspect-to-explain workflow | [`public-sdk-and-cli.md`](specs/public-sdk-and-cli.md) | ADR-0001 | `vaultc`, `vaultc-cli` | SDK/CLI integration suite | documented |
| REQ-MCP-001 thin MCP adapter | [`mcp-adapter.md`](specs/mcp-adapter.md) | ADR-0002 | future `vaultc-mcp` | MCP contract tests | documented |
| REQ-OBS-001 generic review/install plugin | [`obsidian-plugin.md`](specs/obsidian-plugin.md) | ADR-0002 | future plugin | plugin E2E and permission tests | documented |
| REQ-PERF-001 bounded V1 workload | [`testing-and-quality-gates.md`](specs/testing-and-quality-gates.md) | ADR-0005 | full compiler | 100k-note benchmark | documented |
| REQ-MEM-001 experimental memory retrieval | [`algorithms/README.md`](algorithms/README.md) | ALG-MEM-001..006 | future `vaultc-memory` | calibration gates only | documented |
