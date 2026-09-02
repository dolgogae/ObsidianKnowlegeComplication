---
title: Obsidian Plugin Specification
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

# Obsidian Plugin Specification

## Product shape

Ship one generic OKCPack review and installation plugin. Knowledge packs contain data, manifests, provenance, and optional indexes; they do not contain pack-specific executable Obsidian plugins.

## Responsibilities

- Open a local `.okcpack` selected by the user.
- Verify schema, checksums, and signature before showing install actions.
- Display publisher, source/author/license attribution, requested changes, generated-content markers, conflicts, and permissions.
- Preview paths and overwrites against the current Vault.
- Install through the Obsidian Vault API with a recoverable journal.
- Show provenance for installed notes and permit pack-level uninstall/update where safe.

The plugin does not parse/merge source Vaults, choose AI providers, resolve semantic conflicts, host an MCP engine, or execute code contained in a pack.

## Installation policy

Default installation writes into a user-chosen namespace and refuses overwrites. Any overlay/update mode requires a preflight diff, user confirmation, backup or rollback journal, and conflict behavior defined by pack and plugin versions. Symlinks, executable code, `.obsidian/plugins`, and paths outside the active Vault are forbidden.

## Review UX invariants

- “Generated” is visible, not hidden in metadata only.
- Signature status and content integrity are distinct indicators.
- Unverified or unsigned packs may be rejected by policy; the user never sees them as verified.
- Permissions are concrete file operations, not vague trust language.
- Evidence and attribution remain reachable offline after installation.

## Open design work

Pack dependency/version resolution, signing-key lifecycle, update merge semantics, uninstall of user-edited files, registry revocation, and Obsidian mobile constraints remain open in [`../history/OPEN_QUESTIONS.md`](../history/OPEN_QUESTIONS.md).
