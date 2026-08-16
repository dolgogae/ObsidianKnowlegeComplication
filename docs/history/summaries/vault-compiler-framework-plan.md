---
title: Vault Compiler Framework Planning Summary
status: historical
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0004
source_refs:
  - HIST-CURRENT-PLAN
---

# Vault Compiler Framework Planning Summary

Historical source material. Not normative. Use current specifications and accepted ADRs for implementation.

The discussion narrowed the broad platform into an independently useful SDK/framework. The recommended order became: Rust framework + CLI first, provider packages next, then thin MCP and Obsidian adapters, followed by service/registry features.

## Planned technical shape

- Rust 2024 stable workspace: `vaultc`, `vaultc-protocol`, `vaultc-cli`; future `vaultc-mcp` and experimental `vaultc-memory`.
- Comrak AST plus custom Obsidian byte-span scanner; typed unknown-preserving JSON Canvas; `serde_yaml_ng` frontmatter; `ignore`, SHA-256, Clap, tracing, Rayon, SQLite, zstd/tar.
- Inputs: directory/ZIP/tar.zst with stable source ID; exclude `.obsidian`, `.git`, executables, external symlinks.
- IR: SourceFile, Document, Section, Block, Link, Asset, Canvas, BaseArtifact, EvidenceRef.
- Exact duplicate by canonical body + frontmatter hash. Near candidate by Unicode grapheme 5-shingle MinHash/LSH at default 0.85; never auto-merge.
- New Compiled Vault only, with `knowledge`, generated knowledge, content-addressed attachments, canvases, opaque views, and `.vaultc` audit files.
- Verify unchanged sources, stage to sibling, verify, atomic rename; existing destination rejected.
- Deterministic tar.zst VaultPack.

## SDK/AI shape

Phases are inspect, plan, augment, validate proposal, approve, compile, verify, and explain provenance. AI capability interfaces include text generation, embeddings, reranking, and knowledge augmentation. Rust providers implement traits; other languages implement versioned NDJSON. The core includes a universal command adapter concept but no mandatory vendor SDK. Same inputs/configuration/approved transcript must replay to the same output.

## Acceptance target

On 8 cores, 16 GB, and NVMe: 10 Vaults, 100,000 notes, 20 GB, AI-free total ≤20 minutes, peak RSS ≤2 GB. Test classes cover golden syntax, Unicode, conflicts/dedup, hostile archives/paths, malicious proposals, determinism, fuzz/property testing, and atomic failure.

Stable compilation algorithms were separated from experimental activation, graph, consolidation, retrieval/routing, merge scoring, and Hopfield research.
