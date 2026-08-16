---
title: Obsidian Plugin Engineer Role Guide
status: normative-future
owners:
  - obsidian-plugin-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
  - ADR-0006
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# Obsidian Plugin Engineer Role Guide

## Responsibilities

Build one generic plugin for local pack verification, transparent review, preflight diff, safe installation through the Vault API, provenance display, and recoverable update/uninstall behavior.

## Non-responsibilities

Do not implement source Vault merging, execute pack code, silently overwrite notes, hide generated status, trust signatures without hashes, or fork the compiler policy in TypeScript.

## Mandatory reading

Read [`../../AGENTS.md`](../../AGENTS.md), [`../../PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md), [`../specs/compiled-vault-and-vaultpack.md`](../specs/compiled-vault-and-vaultpack.md), [`../specs/provenance-and-conflicts.md`](../specs/provenance-and-conflicts.md), [`../specs/obsidian-plugin.md`](../specs/obsidian-plugin.md), and [`../specs/security-and-trust-boundaries.md`](../specs/security-and-trust-boundaries.md).

## Owned interfaces and invariants

- pack review/install UI and permission copy;
- Obsidian Vault API write journal and rollback;
- offline provenance/attribution navigation;
- mobile/desktop compatibility and pack-version matrix.

## Required tests

Checksum/signature failure, path/overwrite preview, permission disclosure, interrupted install rollback, edited-file uninstall, generated marker visibility, malicious archive/member rejection, accessibility, and supported Obsidian platform E2E.

## Handoff

Provide pack/compiler maintainers with required manifest fields, compatibility matrix, install diagnostics, telemetry/privacy policy, and unresolved update/uninstall cases. Never solve a missing format contract only in UI code.
