---
title: ADR-0001 — Rust Compiler Core
status: normative-v1
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0001: Rust Compiler Core

## Status

Accepted on 2026-08-15; documented on 2026-08-16.

## Context

The compiler must process up to 100,000 notes with bounded memory, expose a reusable SDK and CLI, preserve deterministic behavior, parse hostile inputs safely, and run across Linux, macOS, and Windows. Python is productive for model experiments, and TypeScript is natural for Obsidian, but neither should own the canonical compiler implementation.

## Decision

Build the public compiler core in stable Rust 2024 Edition. Start with `vaultc`, `vaultc-protocol`, and `vaultc-cli`. Python/TypeScript integrations call stable protocols or later bindings rather than reimplement compilation.

## Consequences

We gain strong types, predictable memory, portable native binaries, and one source of truth. We accept a steeper contributor learning curve and must isolate libraries with unstable or unsafe behavior. No nightly-only language feature may enter V1.
