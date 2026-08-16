---
title: Product and Scope Specification
status: normative-v1
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0009
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Product and Scope Specification

## Mission

Compile heterogeneous Obsidian Vault snapshots into safe, deterministic, evidence-traceable knowledge artifacts. The framework provides library primitives and a CLI; agents and user interfaces orchestrate those primitives without owning merge policy.

## Users and jobs

| User | Job | V1 surface |
|---|---|---|
| Rust developer | Embed inspection, planning, compilation, and verification | `vaultc` crate |
| Build/release engineer | Run reproducible local or CI compilation | `vaultc` CLI |
| AI integration developer | Connect any local or hosted model without core changes | traits and NDJSON protocol |
| Knowledge curator | Review conflicts, proposals, and provenance | plan/conflict/provenance files; future UI |
| Coding agent | Inspect and compile through explicit tools | future MCP adapter |
| Obsidian user | Install and inspect a pack safely | future generic plugin |

## Functional requirements

- **REQ-SNP-001:** The compiler MUST NOT modify input Vault files, metadata, permissions, or timestamps.
- **REQ-SNP-002:** Every input file and snapshot MUST have a stable, versioned content identity.
- **REQ-PAR-001:** V1 MUST parse Markdown, YAML frontmatter, wikilinks, embeds, headings, block references, tags, callouts, code, math, and ordinary Markdown links without destroying source spans.
- **REQ-PAR-002:** V1 MUST parse and rewrite JSON Canvas file references while preserving unknown fields.
- **REQ-PAR-003:** V1 MUST preserve `.base` files opaquely and emit a diagnostic that references are not semantically validated.
- **REQ-DED-001:** Exact duplicates MUST be unified according to deterministic tie-break rules while retaining provenance from every source.
- **REQ-DED-002:** Near duplicates MUST be reported as candidates and MUST NOT be semantically auto-merged in V1.
- **REQ-CNF-001:** Path, case-fold, title, alias, frontmatter, and link-target conflicts MUST be represented and resolved deterministically or require approval.
- **REQ-PRV-001:** Every materialized file MUST have provenance; generated knowledge MUST name supporting evidence.
- **REQ-AI-001:** AI integration MUST be provider-neutral and capability-driven.
- **REQ-AI-002:** AI output MUST be treated as an untrusted proposal, schema-validated, evidence-validated, and explicitly approved before use.
- **REQ-AI-003:** Provider requests and responses used by a build MUST be recordable and replayable.
- **REQ-CMP-001:** Compilation MUST stage to a sibling temporary directory and atomically publish a new output; existing output MUST be rejected by default.
- **REQ-CMP-002:** A `.vaultpack` built from identical inputs, configuration, approvals, transcript, and toolchain version MUST be byte-identical.
- **REQ-SEC-001:** Archives, symlinks, paths, Markdown, metadata, and provider output MUST be treated as hostile data.
- **REQ-SDK-001:** SDK and CLI MUST expose inspect, plan, augment, validate, approve, compile, verify, and provenance explanation operations.
- **REQ-MCP-001:** The future MCP server MUST remain a stateless, least-authority adapter over framework calls.
- **REQ-OBS-001:** One generic future Obsidian plugin MUST review and install many data packs; pack-specific executable plugins are forbidden.
- **REQ-PERF-001:** The AI-free V1 workload of 10 Vaults, 100,000 notes, and 20 GB MUST complete within 20 minutes and 2 GB peak RSS on the reference machine.

## Inputs and outputs

Accepted inputs are filesystem directories, ZIP archives, and deterministic `tar.zst` archives. Every source is paired with a stable `source_id`; accidental source-ID reuse with changed identity is an error unless recorded as a new snapshot version.

The primary output is a new Compiled Vault. Optional distribution output is a `.vaultpack`. Reports may be emitted without materializing content.

## Product evolution

The framework is phase zero of a broader Knowledge Compilation Platform:

1. Framework and CLI: deterministic inspection, planning, compilation, verification.
2. Optional AI proposal protocol and provider packages.
3. Thin MCP adapter and Obsidian review/install plugin.
4. Vault benchmarking, claim reconciliation, and calibrated retrieval experiments.
5. On-premise pack registry, search, compatibility recommendations, and marketplace governance.

Later phases do not relax V1 invariants.

## Success criteria

A cold-start developer can compile two Vault fixtures, inspect every conflict, reproduce the same output, trace each byte of derived Markdown to inputs or approved proposals, and switch AI providers without changing compiler code.
