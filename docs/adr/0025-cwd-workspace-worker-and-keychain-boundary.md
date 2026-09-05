---
title: ADR-0025 — CWD Workspace, Worker, and Keychain Boundary
status: normative-v1
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-09-05
decision_refs:
  - ADR-0019
  - ADR-0023
  - ADR-0024
  - ADR-0025
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0025: CWD Workspace, Worker, and Keychain Boundary

## Status

Accepted on 2026-09-05 to resolve the documented traceability and credential
contract conflicts before implementing the cwd-first V3 TUI.

## Context

The application shell required users to pre-create and pass a project, and its
TUI did not execute provider or compiler work. The intended desktop workflow
starts with `cd <workspace> && okc`, discovers safe Vaults, reviews every V3
checkpoint, and remains responsive while hostile-input, network, and compile
work runs. ADR-0023 also restricted credentials to environment-variable names,
while native desktop use requires an OS credential store without writing a
secret to TOML or project state.

## Decision

`okc-app` owns three shared services used directly by CLI and TUI:
`WorkspaceBootstrap`, `ProviderService`, and `IntegrationService`. Explicit
`--project` wins. Otherwise discovery is bounded to the cwd hidden default,
safe sibling workspace, and direct-child project directories. Vault discovery
is bounded to cwd, its direct non-symlink directories, and direct ZIP/tar.zst
archives; managed/output/project directories and nested or overlapping sources
are rejected. Registered source locators are absolute and lexical/canonical
containment checks keep projects and output outside every source.

The project/artifact format remains schema 3. The private SQLite state advances
to `user_version = 4` and appends source-set revisions and content-addressed
approved-integration-plan pointers. Replacing the active source set is atomic,
creates a new run identity, and makes prior approval authority stale without
deleting history.

Provider profiles may contain exactly one opaque credential reference:
`api_key_env` or `os_keychain`. The keychain service ID is fixed by OKC and the
account is the profile-scoped reference. Native secret storage uses keyring
4.2.0's `v1` API. Secret buffers are zeroized, have redacted `Debug`, and never
enter files, SQLite, recordings, hashes, diagnostics, or screens. A locked,
denied, or unavailable store offers an environment-variable reference or
cancellation only; plaintext fallback is forbidden.

Long operations use a bounded `std::thread` worker channel. At most one
mutating operation holds the project writer lock. `OperationControl` progress
and cancellation propagate into compiler and provider calls. Cancellation is
accepted before the atomic publication barrier; after that boundary the UI
must report that publication cannot be cancelled. Completed task cache entries
are reused and a failed/cancelled run resumes at the first incomplete task.

## Consequences

- `cd <workspace> && okc` is the primary interactive entry point while scripts
  retain explicit, fail-closed project and consent behavior.
- CLI and TUI cannot drift into separate provider, review, or compile policy.
- Native credential-store availability is a platform capability, not a reason
  to weaken storage security.
- Schema-4 migration is internal and does not change V3 artifact identity.
- Mouse input, model auto-selection, remote auto-consent, auto-approval, V3
  Pack, and non-Markdown carry-through remain outside this decision.
