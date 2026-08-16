---
title: Decision Log
status: historical
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0004
  - ADR-0005
  - ADR-0006
  - ADR-0007
  - ADR-0008
source_refs:
  - HIST-SHARED-CHAT
  - HIST-CURRENT-PLAN
---

# Decision Log

Append-only. Normative details live in specifications and accepted ADRs.

## 2026-08-14 — MCP roles and platform reframing

- Researched Obsidian MCP options with Claude Code and Codex as primary clients.
- Assigned complementary roles to `lstpsche/obsidian-mcp` (retrieval), `totocaster/arrowhead` (graph/discovery), `bitbonsai/mcpvault` (safe materialization concepts), and `blacksmithers/vaultforge` (topic/theme compression).
- Reframed the system from “Vault Merge Platform” to “Knowledge Compilation Platform.”
- Chose immutable raw snapshots, canonical knowledge representation, derived/rebuildable indexes, evidence-bearing claims, and topic-relative multidimensional benchmarks.
- Rejected public cloud and initial Kubernetes; selected ordinary Linux VMs with Docker Compose as the platform direction.

## 2026-08-15 — Framework product boundary

- Chose a framework-first product: Rust library and CLI before MCP/plugin/web platform.
- Chose stable Rust 2024, dual MIT OR Apache-2.0, SemVer, and Linux/macOS/Windows V1 support.
- Set V1 target to 10 Vaults, 100,000 notes, 20 GB, ≤20 minutes and ≤2 GB peak RSS on the reference machine.
- Made Markdown/frontmatter/Obsidian links/attachments/JSON Canvas first-class; `.base` opaque with warnings; `.obsidian/**` excluded.
- Decided input Vaults are immutable and output is a new integrated Compiled Vault without raw source copies.
- Chose a deterministic core with provider-neutral optional AI proposals, deterministic validation, explicit approval, and record/replay.
- Chose Rust traits plus versioned NDJSON for cross-language providers, with no mandatory vendor adapter in core.
- Separated brain-inspired memory/retrieval models into future experimental `vaultc-memory` work.

## 2026-08-16 — Documentation baseline

- Established Markdown as the cold-start implementation contract.
- Added precedence rules, role routing, requirement/algorithm IDs, stable and experimental algorithm templates, ADRs, source/transcript preservation, and traceability obligations.
- Recorded publicly recoverable source material; inaccessible tool outputs, custom instructions, and image bytes remain explicitly unavailable rather than reconstructed.
