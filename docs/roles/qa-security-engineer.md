---
title: QA and Security Engineer Role Guide
status: normative
owners:
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0007
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# QA and Security Engineer Role Guide

## Responsibilities

Own the threat model, hostile-input corpus, quality gates, deterministic cross-platform verification, fuzz/property testing, supply-chain checks, performance harness, privacy review, and incident/fail-closed behavior.

## Non-responsibilities

Do not mark unverifiable behavior safe, weaken limits to make tests pass, treat signatures as content safety, or approve experimental promotion without held-out evidence and rollback.

## Mandatory reading

Read [`../../AGENTS.md`](../../AGENTS.md), [`../../PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md), [`../TRACEABILITY.md`](../TRACEABILITY.md), [`../specs/security-and-trust-boundaries.md`](../specs/security-and-trust-boundaries.md), [`../specs/testing-and-quality-gates.md`](../specs/testing-and-quality-gates.md), pipeline/output/provenance specs, every changed algorithm, and relevant ADRs.

## Owned interfaces and invariants

- stable diagnostic taxonomy for security failures with core engineer;
- resource-limit and platform support matrices;
- release gate evidence and security exceptions;
- corpus provenance and safe handling.

## Required tests

All QG-001..008 checks, especially source immutability, path/symlink/archive attacks, malformed parsers, prompt injection, proposal forgery, atomic interruption, provenance closure, pack corruption, cross-platform byte/semantic equality, and 100k-note performance.

## Handoff

Report exact reproduction, affected requirement IDs, severity/exploit boundary, fixture hash, expected fail-closed result, and regression test. Accepted exceptions require expiry, owner, compensating control, and decision-log/ADR treatment.
