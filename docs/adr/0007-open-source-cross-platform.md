---
title: ADR-0007 — Open Source and Cross-Platform V1
status: normative-v1
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0007
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0007: Open Source and Cross-Platform V1

## Status

Accepted on 2026-08-15.

## Context

A foundational knowledge compiler needs broad embedding rights and must work where Vault users work. Platform-specific path behavior is also a correctness risk.

## Decision

License the public framework under dual MIT OR Apache-2.0, follow SemVer, and support Linux/macOS/Windows x86_64 plus macOS arm64 in V1. Release archives, checksums, licenses, SBOM/provenance, and platform determinism tests are required.

## Consequences

Dependencies must be license-compatible. Cross-platform filename policy may be stricter than any one filesystem. Release automation and security maintenance are part of the core scope.
