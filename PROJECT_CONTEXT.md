---
title: Obsidian Knowledge Compilation Project Context
status: normative-v1
owners:
  - architect
last_updated: 2026-09-05
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0004
  - ADR-0009
  - ADR-0011
  - ADR-0012
  - ADR-0013
  - ADR-0014
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
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Obsidian Knowledge Compilation (OKC)

The project compiles multiple Obsidian Vault snapshots into a new, deterministic, auditable Vault. It is a knowledge compiler rather than a folder merger: Markdown, Canvas, attachments, links, and metadata are parsed into a canonical intermediate representation (IR), every Markdown document is assigned to an approved semantic cluster, AI proposes evidence-complete synthesis, and a critic-checked human-approved materialization is emitted as a Compiled Vault. V3 provider exchanges are recorded so compile and verify remain offline and deterministic. User Packs are unsigned; application binaries still require native release signing.

## Product boundary

The first deliverable is an open-source Rust framework with one `okc` CLI/TUI executable. OKC compiles standard Vault files and never connects to, identifies, or federates the MCP server that produced them. MCP servers, Obsidian plugins, web services, and a marketplace are adapters or later products around the compiler. The canonical model and compilation policy belong in the framework, not in any LLM, MCP server, UI, or search index.

The framework is intended for:

- local developers integrating Vaults through the Rust SDK;
- scripts and build systems using the CLI and JSON/NDJSON protocols;
- any LLM provider implementing capability-based interfaces;
- future Codex, Claude Code, Cursor, VS Code, and Obsidian integrations;
- a future on-premise Knowledge Package Registry.

## V3 contract

- Every Markdown document belongs to exactly one approved taxonomy cluster and
  every source block/frontmatter value has exactly one disposition.
- Every cluster, including a singleton, requires structured synthesis, evidence
  closure, a passing critic report, and explicit curator approval.
- A V3 compile requires a complete `ApprovedIntegrationPlan` but never invokes
  a live provider. Stored recordings support provider-free replay.
- Sensitive-content preflight happens before disclosure. Remote calls require
  a sealed route plus one-run consent; sensitive semantic routing is local-only.
- Canonical notes live below `knowledge/`; source-specific redirect stubs below
  `legacy/` preserve navigation and provenance.
- V3 uses schema 3, `okc:*:v3\0` identity domains, append-only project journals,
  and content-addressed objects.
- `cd <workspace> && okc` discovers only bounded cwd candidates. CLI and TUI
  share application services; long work runs on one cancellable worker and
  native credentials use only opaque OS-keychain or environment references.
- V1 and V2 artifacts are read-only through `verify` and `explain`. Project
  upgrade creates a new V3 project and never carries approval authority.

## Preserved V2 compatibility contract

- Rust 2024 Edition on stable Rust; Linux, macOS, and Windows, including macOS arm64.
- Up to 10 input Vaults, 100,000 notes, and 20 GB on 8 CPU cores, 16 GB RAM, and NVMe storage.
- Directory, ZIP, and `tar.zst` inputs with stable source IDs and optional display-only owners; input order and MCP origin do not affect output.
- Markdown, YAML frontmatter, wikilinks, embeds, block references, attachments, and JSON Canvas are first-class. `.base` files are preserved opaquely with warnings. `.obsidian/**` is excluded.
- Inputs remain byte-for-byte unchanged. Compilation creates a new output and never stores raw `_sources` inside it.
- Exact duplicates may be unified deterministically. Near duplicates are candidates for review and are never auto-merged in V2.
- Link ambiguities may select only an exact sealed target or explicitly preserve the original through an immutable materialization overlay.
- AI was optional and provider-neutral in V2. This behavior remains meaningful
  only for frozen V2 verification/explanation and is not a V3 compile default.
- Every output is traceable to immutable input content hashes and, for generated content, explicit evidence references.
- The AI-free path is reproducible across runs and supported platforms.

## Non-goals for V3

- Determining objective truth by majority vote.
- Executing uploaded plugins, scripts, or code.
- Losslessly interpreting Obsidian `.base` semantics.
- Claiming that semantic synthesis determines objective truth or that model
  confidence is calibrated without the required evaluation.
- A hosted marketplace, billing, collaborative editing, or Kubernetes deployment.
- Making neuroscience claims about human cognition.
- Shipping vendor-specific LLM SDKs in the compiler core.

## Durable principles

1. Immutable snapshots are the identity boundary.
2. The canonical IR is the source of truth; BM25, vectors, graphs, and MCP indexes are derived and replaceable.
3. Conflicting claims coexist with provenance, time, and context until an explicit policy or human decision resolves their presentation.
4. Nondeterministic augmentation is captured as a replayable transcript;
   approval and compilation remain deterministic.
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
