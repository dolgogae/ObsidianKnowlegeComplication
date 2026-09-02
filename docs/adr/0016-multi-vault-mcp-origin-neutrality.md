---
title: ADR-0016 — Multi-Vault MCP-Origin-Neutral Compilation
status: normative-v1
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0002
  - ADR-0016
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0016: Multi-Vault MCP-Origin-Neutral Compilation

## Status

Accepted on 2026-09-02.

## Context

OKC combines the durable Vault files produced by many people and tools. MCP
server identity is neither stable input data nor compilation policy and would
make equal Vault bytes produce different results.

## Decision

A project accepts at most ten directory, ZIP, or `tar.zst` Vault snapshots.
Each has a stable project-unique `source_id` and an optional display-only owner
name. Source order is canonicalized by source identity and content identity.
Registering identical whole-Vault content twice, or reusing one `source_id` for
a changed snapshot without explicit project rebind/invalidation, fails closed.

Compiler inputs, identities, policies, plans, and manifests MUST NOT contain an
MCP implementation name. `.obsidian/plugins/**`, MCP databases, caches,
indexes, and executables are excluded. Configured nonstandard files may be
preserved as opaque assets or explicitly excluded with diagnostics.

Link resolution is source-local first: exact relative path; local normalized
path/stem/title/alias; then cross-source normalized candidates. One candidate
is rewritten, multiple candidates become `LINK_AMBIGUITY`, and no candidate
preserves only a root-contained source spelling with a warning. Attachments
are never guessed from a foreign same-name file; byte-identical attachments
may unify with all provenance edges retained.

## Consequences

- Equal `source_id + Vault bytes + policy` yields equal V2 planning and output
  regardless of the MCP that created the files.
- Source-local meaning wins over a foreign homonym.
- Near duplicates and title/alias similarities remain review-only and are not
  semantic auto-merges.
