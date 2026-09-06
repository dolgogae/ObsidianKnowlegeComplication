---
title: Release Maintainer Role Guide
status: normative
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0007
source_refs:
  - HIST-CURRENT-PLAN
---

# Release Maintainer Role Guide

## Responsibilities

Own SemVer, dual-license distribution, changelog/release notes, documentation link integrity, schema/version matrix, dependency/license review, reproducible binaries/packs, checksums/signing, SBOM/provenance, and current-state/traceability hygiene.

## Non-responsibilities

Do not publish with failed quality gates, silently rewrite history, treat a tag as reproducibility proof, promote experimental features, or change normative behavior without ADR/spec updates.

## Mandatory reading

Read [`../../AGENTS.md`](../../AGENTS.md), [`../../PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md), [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../TRACEABILITY.md`](../TRACEABILITY.md), [`../specs/testing-and-quality-gates.md`](../specs/testing-and-quality-gates.md), [`../specs/compiled-vault-and-vaultpack.md`](../specs/compiled-vault-and-vaultpack.md), ADR-0007, and all changes since the previous release.

## Owned interfaces and invariants

- crate/CLI/protocol/pack version mapping;
- release target support matrix and artifact naming;
- documentation precedence and historical immutability;
- signing-key process and revocation metadata when defined.

## Required tests

Clean-room builds, artifact checksum/byte comparison, install/run smoke tests on each target, license/SBOM audit, schema compatibility, Markdown link validation, traceability completeness, and signed-artifact verification.

## Handoff

Publish versioned artifacts with checksums, signature status, SBOM/provenance, migration/deprecation notes, known issues, security contact, source tag/commit, test evidence, and precise experimental-feature status.
