---
title: Current State
status: normative-v1
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
source_refs:
  - HIST-CURRENT-PLAN
---

# Current State

## Snapshot: 2026-08-16

The repository is in the documentation baseline phase. There is no product implementation, package manifest, CI workflow, release artifact, or executable test suite yet.

Completed documentation baseline:

- product identity, V1 scope, non-goals, and platform evolution;
- framework architecture and canonical IR boundaries;
- deterministic compilation, output, provenance, and conflict rules;
- provider-neutral AI proposal and approval protocol;
- planned Rust SDK, CLI, MCP adapter, and Obsidian plugin interfaces;
- stable and experimental algorithm specifications;
- security, quality gates, roles, ADRs, history, and source provenance.

## Next implementation slice

Implement the smallest deterministic vertical slice:

1. Create the Rust workspace (`vaultc`, `vaultc-protocol`, `vaultc-cli`).
2. Define stable identifiers, manifest schemas, diagnostics, and canonical IR.
3. Inspect a directory Vault without mutation; hash files and apply exclusion rules.
4. Parse Markdown/frontmatter/wikilinks and JSON Canvas into IR.
5. Produce a deterministic plan and compile a trivial new Vault atomically.
6. Verify checksums and provenance through CLI integration tests.

No AI, MCP, graph retrieval, marketplace, or neuroscience-inspired runtime behavior belongs in this first slice.

## Status vocabulary

- `documented`: contract exists but no code is present.
- `implemented`: production code exists.
- `verified`: required automated checks pass.
- `blocked`: a named external decision or dependency prevents progress.

All requirements in [`TRACEABILITY.md`](TRACEABILITY.md) are currently `documented` only.
