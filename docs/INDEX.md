---
title: Documentation Index
status: normative
owners:
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0002
  - ADR-0015
  - ADR-0019
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-CURRENT-PLAN
---

# Documentation Index

## Start here

1. [`../AGENTS.md`](../AGENTS.md) — operating contract for every coding agent.
2. [`../PROJECT_CONTEXT.md`](../PROJECT_CONTEXT.md) — product identity and current Schema 3 boundary.
3. [`CURRENT_STATE.md`](CURRENT_STATE.md) — what exists now.
4. [`TRACEABILITY.md`](TRACEABILITY.md) — requirement, algorithm, implementation, and test map.
5. [`GLOSSARY.md`](GLOSSARY.md) — shared vocabulary.

## User guides

- [`AI integration`](../guide/integration.md) — configure providers,
  approve taxonomy and clusters, then compile, verify, and explain offline.
- [`Quickstart`](../guide/index.md) — build the development binary and create a
  current project from a source Vault.
- [`CLI`](../guide/cli.md) — current commands, global-option scope, and
  automation exit codes.
- [`TUI`](../guide/tui.md) — cwd discovery, provider/keychain setup, Vault
  selection, review/regeneration, worker cancellation, compile, and verify.
- [`Python · Node.js`](../guide/python-node.md) — explicit-path library setup,
  provider consent, complete approval flow, jobs, typed results, and errors.
- [`Conflict review`](../guide/conflicts.md) and
  [`AI Provider`](../guide/ai-provider.md) — explicit human-decision and
  untrusted-proposal workflows.

## Choose a role

| Role | Use when working on | Guide |
|---|---|---|
| Architect | boundaries, IR ownership, ADRs, cross-component behavior | [`roles/architect.md`](roles/architect.md) |
| Core Rust engineer | parser, planner, compiler, CLI, deterministic storage | [`roles/core-rust-engineer.md`](roles/core-rust-engineer.md) |
| Algorithms/AI engineer | provider protocol, proposals, evaluation, experimental retrieval | [`roles/algorithms-ai-engineer.md`](roles/algorithms-ai-engineer.md) |
| MCP adapter engineer | coding-agent tools and external engine adapters | [`roles/mcp-adapter-engineer.md`](roles/mcp-adapter-engineer.md) |
| Obsidian plugin engineer | review/install UX and Vault API materialization | [`roles/obsidian-plugin-engineer.md`](roles/obsidian-plugin-engineer.md) |
| QA/security engineer | hostile-input testing, determinism, performance, supply chain | [`roles/qa-security-engineer.md`](roles/qa-security-engineer.md) |
| Release maintainer | versioning, packaging, licenses, documentation integrity | [`roles/release-maintainer.md`](roles/release-maintainer.md) |

## Specifications

- [`specs/product-and-scope.md`](specs/product-and-scope.md)
- [`specs/framework-architecture.md`](specs/framework-architecture.md)
- [`specs/canonical-knowledge-ir.md`](specs/canonical-knowledge-ir.md)
- [`specs/vault-compilation-pipeline.md`](specs/vault-compilation-pipeline.md)
- [`specs/public-sdk-and-cli.md`](specs/public-sdk-and-cli.md)
- [`specs/ai-provider-and-augmentation.md`](specs/ai-provider-and-augmentation.md)
- [`specs/compiled-vault-and-vaultpack.md`](specs/compiled-vault-and-vaultpack.md)
- [`specs/provenance-and-conflicts.md`](specs/provenance-and-conflicts.md)
- [`specs/mcp-adapter.md`](specs/mcp-adapter.md)
- [`specs/obsidian-plugin.md`](specs/obsidian-plugin.md)
- [`specs/security-and-trust-boundaries.md`](specs/security-and-trust-boundaries.md)
- [`specs/testing-and-quality-gates.md`](specs/testing-and-quality-gates.md)
- [`specs/roadmap.md`](specs/roadmap.md)
- [`RELEASE.md`](RELEASE.md)

## Algorithms

The status matrix and promotion rules are in [`algorithms/README.md`](algorithms/README.md).
`ALG-SEM-001` and `ALG-INT-001` define the current semantic path; the other
stable algorithms define safe snapshot/parsing/materialization primitives.
Experimental algorithms cannot be enabled by default until their calibration
gates are met and an ADR promotes them.

## Decisions and history

- Accepted decisions: [`adr/`](adr/)
- Current boundary: [`ADR-0027`](adr/0027-current-schema-single-source.md),
  with the retained Schema 3 decisions in
  [`ADR-0022`](adr/0022-v3-ai-required-format-and-legacy-boundary.md),
  [`ADR-0023`](adr/0023-provider-profiles-disclosure-and-recording.md),
  [`ADR-0024`](adr/0024-evidence-complete-integration-and-critic-gate.md), and
  [`ADR-0025`](adr/0025-cwd-workspace-worker-and-keychain-boundary.md), plus
  [`ADR-0026`](adr/0026-python-node-library-boundary.md)
- Append-only decision log: [`history/DECISION_LOG.md`](history/DECISION_LOG.md)
- Open design questions: [`history/OPEN_QUESTIONS.md`](history/OPEN_QUESTIONS.md)
- Historical summaries: [`history/summaries/`](history/summaries/)
- Recoverable transcripts: [`history/transcripts/`](history/transcripts/)
- Sources: [`references/SOURCES.md`](references/SOURCES.md)
- Facts that require revalidation: [`references/TIME_SENSITIVE_FACTS.md`](references/TIME_SENSITIVE_FACTS.md)
