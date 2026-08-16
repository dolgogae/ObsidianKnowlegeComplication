---
title: Vault Compiler Framework Project Context
status: normative-v1
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0004
  - ADR-0009
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Vault Compiler Framework

The project compiles multiple Obsidian Vault snapshots into a new, deterministic, auditable Vault. It is a knowledge compiler rather than a folder merger: Markdown, Canvas, attachments, links, and metadata are parsed into a canonical intermediate representation (IR), conflicts and duplication are planned explicitly, optional AI proposes evidence-bound enrichments, and an approved plan is materialized as a Compiled Vault or deterministic `.vaultpack`. Signing belongs to a future versioned profile and is not part of the current V1 implementation.

## Product boundary

The first deliverable is an open-source Rust framework and CLI. MCP servers, Obsidian plugins, web services, and a marketplace are adapters or later products around the compiler. The canonical model and compilation policy belong in the framework, not in any LLM, MCP server, UI, or search index.

The framework is intended for:

- local developers integrating Vaults through the Rust SDK;
- scripts and build systems using the CLI and JSON/NDJSON protocols;
- any LLM provider implementing capability-based interfaces;
- future Codex, Claude Code, Cursor, VS Code, and Obsidian integrations;
- a future on-premise Knowledge Package Registry.

## V1 contract

- Rust 2024 Edition on stable Rust; Linux, macOS, and Windows, including macOS arm64.
- Up to 10 input Vaults, 100,000 notes, and 20 GB on 8 CPU cores, 16 GB RAM, and NVMe storage.
- Directory, ZIP, and `tar.zst` inputs with stable source IDs.
- Markdown, YAML frontmatter, wikilinks, embeds, block references, attachments, and JSON Canvas are first-class. `.base` files are preserved opaquely with warnings. `.obsidian/**` is excluded.
- Inputs remain byte-for-byte unchanged. Compilation creates a new output and never stores raw `_sources` inside it.
- Exact duplicates may be unified deterministically. Near duplicates are candidates for review and are never auto-merged in V1.
- AI is optional and provider-neutral. It returns proposals, never direct filesystem mutations. Only validated and explicitly approved proposals enter a build.
- Every output is traceable to immutable input content hashes and, for generated content, explicit evidence references.
- The AI-free path is reproducible across runs and supported platforms.

## Non-goals for V1

- Determining objective truth by majority vote.
- Executing uploaded plugins, scripts, or code.
- Losslessly interpreting Obsidian `.base` semantics.
- Automatic semantic merging of near duplicates.
- A hosted marketplace, billing, collaborative editing, or Kubernetes deployment.
- Making neuroscience claims about human cognition.
- Shipping vendor-specific LLM SDKs in the compiler core.

## Durable principles

1. Immutable snapshots are the identity boundary.
2. The canonical IR is the source of truth; BM25, vectors, graphs, and MCP indexes are derived and replaceable.
3. Conflicting claims coexist with provenance, time, and context until an explicit policy or human decision resolves their presentation.
4. Determinism is the default; nondeterministic augmentation is captured as a replayable transcript.
5. Capability negotiation replaces vendor coupling.
6. Safe materialization occurs only after planning, validation, and approval.
7. Brain-inspired formulas are engineering hypotheses with calibration gates, not facts about brains.

## Read next

- Navigation and roles: [`docs/INDEX.md`](docs/INDEX.md)
- Current implementation status: [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md)
- Product requirements: [`docs/specs/product-and-scope.md`](docs/specs/product-and-scope.md)
- Architecture: [`docs/specs/framework-architecture.md`](docs/specs/framework-architecture.md)
- Algorithms: [`docs/algorithms/README.md`](docs/algorithms/README.md)
- Decision history: [`docs/history/DECISION_LOG.md`](docs/history/DECISION_LOG.md)
