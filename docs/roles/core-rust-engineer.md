---
title: Core Rust Engineer Role Guide
status: normative
owners:
  - core-rust-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0001
  - ADR-0003
  - ADR-0005
  - ADR-0008
  - ADR-0015
  - ADR-0017
  - ADR-0019
source_refs:
  - HIST-COMPILER-PLAN
---

# Core Rust Engineer Role Guide

## Responsibilities

Implement stable identifiers, safe source readers, parsers/scanners, canonical IR, SQLite workspace, deduplication, conflict planning, approval validation, output materialization, pack generation, verifier, Rust API, and CLI.

## Non-responsibilities

Do not embed vendor LLM clients in core, implement Obsidian UI/MCP transport policy, enable experimental memory algorithms by default, follow unsafe symlinks, or modify source Vaults.

## Mandatory reading

Read [`../../AGENTS.md`](../../AGENTS.md), [`../../PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md), [`../CURRENT_STATE.md`](../CURRENT_STATE.md), the architecture/IR/pipeline/SDK/output/provenance/security/test files in [`../specs/`](../specs/), all [`../algorithms/stable/`](../algorithms/stable/) files, and ADR-0001/0003/0004/0005/0006/0007/0008.

## Owned interfaces and invariants

- public `okc-core` and sole `okc` executable behavior, plus the deprecated
  current-only public surface and explicit retired-schema unsupported boundary;
- source byte immutability and streaming limits;
- deterministic ordered serialization and IDs;
- stage-verify-atomic-publish workflow;
- independent artifact verification.

## Required tests

Golden Markdown/Canvas/link vectors, identity vectors, conflict/dedup fixtures, property/fuzz tests, malicious paths/archives/proposals, interruption tests, provenance closure, cross-platform deterministic builds, and the reference performance benchmark.

## Handoff

Expose stable typed interfaces and protocol schemas to adapter engineers. Document diagnostic codes, feature gates, platform limitations, test commands, and any semantic version impact in traceability/current state.
