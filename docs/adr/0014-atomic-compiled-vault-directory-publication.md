---
title: ADR-0014 — Atomic Compiled Vault Directory Publication
status: normative
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-08-18
decision_refs:
  - ADR-0003
  - ADR-0007
  - ADR-0012
  - ADR-0013
  - ADR-0014
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0014: Atomic Compiled Vault Directory Publication

## Status

Accepted on 2026-08-18.

## Context

The unpublished `0.1.0` compiler creates and verifies a sibling staging
directory, checks the requested destination a second time, and then calls the
replace-capable generic filesystem rename. A process can create an empty
directory, file, or symlink between the check and rename. Some supported
platforms may then replace that race winner, violating REQ-CMP-001 and the
create-new trust boundary.

The staging lifetime also becomes unmanaged before the final rename. Caught
cleanup failures and incomplete-marker failures are ignored, so the API cannot
tell an operator where a failed stage remains. Separately, an output nested
inside a source directory places the sibling stage inside that source and
modifies an input before publication.

ADR-0013 closed the equivalent VaultPack file boundary. A directory requires
native directory-rename semantics rather than the hard-link fallback that is
valid only for regular files.

## Decision

### One exclusive directory commit

`VaultCompiler::compile` and `compile_with_options` continue to share the core
directory publisher. The public methods, artifact schemas, and successful
return types do not change. The initial destination metadata check remains a
fast-fail optimization only; the final namespace operation is the security
boundary.

The compiler MUST perform these ordered steps:

1. validate the approved plan, source/publication separation, destination, and
   any optional Pack preflight before creating a stage;
2. create one restrictive unique sibling staging directory in the output
   parent;
3. materialize every output with create-new semantics and synchronize every
   regular file;
4. synchronize the completed staging directory tree where the platform policy
   supports directory synchronization;
5. independently verify the staged Compiled Vault;
6. publish the staging directory with one atomic no-replace operation;
7. synchronize the output parent where the platform policy supports it; and
8. only after successful directory durability handling, begin optional Pack
   publication.

The production implementation MUST NOT perform a second `exists` check
followed by a replacing rename. It MUST NOT retry through a weaker primitive.
If the platform, kernel, or filesystem does not support the required
no-replace primitive, publication fails closed as an I/O error with unsupported
semantics and the requested output remains absent unless another actor won the
name.

### Supported-platform primitives

- Linux uses safe `rustix::fs::renameat_with` with
  `RenameFlags::NOREPLACE`, backed by `renameat2(RENAME_NOREPLACE)`.
- macOS uses the same safe Rust API, backed by
  `renameatx_np(RENAME_EXCL)`.
- Windows uses safe `atomicwrites::move_atomic` for the sibling directory,
  backed by `MoveFileExW` with write-through and without
  `MOVEFILE_REPLACE_EXISTING`.
- Other targets fail closed until an ADR-qualified no-replace directory
  primitive and test matrix are added.

Unix commits use an opened common-parent directory and the source/destination
basenames. Windows staging is already constrained to the same parent and
therefore the same volume. No unsafe block is added to `vaultc`; workspace
`unsafe_code = "forbid"` remains in force.

Any existing destination leaf or winner that appears before the exclusive
commit is `OutputExists`, including a regular file, empty or non-empty
directory, live symlink, dangling symlink, or supported Windows reparse point.
The compiler never follows the leaf, creates a dangling referent, empties a
directory, or replaces the winner. Native unsupported errors and unrelated I/O
errors are not disguised as `OutputExists` merely because an earlier check
observed absence.

### Source/output separation

Before creating an output/Pack parent or staging entry, the compiler resolves
each source locator and every requested publication path through their existing
ancestor, performs lexical normalization, and compares the V1 portable
NFC/full-case-fold component keys. The Compiled Vault and an optional
integrated Pack MUST NOT equal a source, contain a source, or be nested inside
a source directory through lexical spelling, existing symlink alias, case
alias, or normalization alias. In particular, no directory or Pack stage may be
created inside an immutable source Vault. A standalone Pack publisher cannot
recover redacted original source locators from a Compiled Vault, so this
additional source boundary belongs to `compile_with_options`.

This check does not claim to close a malicious replacement of an ancestor
after validation. Descriptor-relative/no-follow source opening, staging, and
ancestor pinning remain a separate named release blocker.

### Staging disposition and post-commit state

The `TempDir` guard remains live through the exclusive commit. On every caught
pre-commit failure:

- with `retain_failed_staging = false`, the exact known stage is explicitly
  removed;
- with `retain_failed_staging = true`, a synchronized
  `.vaultc-INCOMPLETE` marker is written before the exact stage is retained;
- a marker or cleanup failure returns a distinct
  `StagingDispositionFailed` error containing the staging path, original
  compilation/publication error, disposition action, and disposition I/O
  failure. An unmarked tree is never intentionally retained.

A concurrent winner is never removed by cleanup. Process kill or power loss
may leave a hidden sibling stage because no in-process cleanup can run. A
future orphan-management command must identify compiler-owned stages safely;
this ADR does not authorize wildcard deletion.

After the no-replace commit, rollback is forbidden. If parent synchronization
fails, the verified complete directory remains visible and compilation returns
`PublishedButDurabilityUncertain`. Optional Pack publication MUST NOT begin in
that state. Windows success means process-visible no-replace publication and
the write-through behavior of the selected primitive; V1 does not claim a
safe-Rust directory-handle flush or physical power-loss guarantee on Windows.

The existing two-publication behavior remains: once directory publication and
its required durability action succeed, a later Pack error is
`PackPublicationAfterCompile` and keeps the valid Compiled Vault.

### Acceptance and fault seam

A private directory-publication hook MUST expose deterministic checkpoints for
stage creation, materialization/file synchronization, tree synchronization,
staged verification, before publish, published, and parent synchronized. It is
not a public fault-injection API.

Acceptance MUST include:

- existing file/directory/live-symlink/dangling-symlink preservation;
- a barrier immediately before the real no-replace call that installs an
  external file, empty directory, or symlink and proves the exact winner is
  preserved;
- concurrent SDK and CLI creators with exactly one complete verified winner;
- two distinct candidate builds that can never mix or replace one another;
- default cleanup, explicit marked retention, cleanup failure reporting,
  staged-verification failure, unsupported-primitive fail-closed behavior, and
  post-commit parent-sync state;
- source/publication equality and containment aliases, including an integrated
  Pack inside a source, rejected before any stage or final output;
- no optional Pack started by a race loser or a directory durability error;
  and
- Linux, macOS, and Windows local-filesystem CI. Symlink/reparse cases may be
  skipped only with an explicit runner capability reason.

Stress-only timing tests are supplemental. The publish-barrier regression is
the required proof because two normal compilers often race with non-empty
directories and can accidentally make the old replacing rename appear safe.

### Compatibility and support boundary

This is a pre-release safety completion. It does not alter plan, approval,
manifest, provenance, artifact, or VaultPack bytes. Staging names, host
destinations, fault checkpoints, and durability state remain runtime-only.
`VaultcError` is already non-exhaustive; `StagingDispositionFailed` adds honest
failure detail without changing serialized artifact schemas or CLI exit
families. Compilation and publication failures remain exit `6`, except a
nested internal invariant remains exit `70`.

Linux/macOS local filesystems and Windows NTFS are the V1 qualification target.
Network filesystems, FUSE implementations, unsupported kernel primitives,
ancestor replacement races, and physical power-loss behavior remain
unqualified until separately tested. This ADR closes final-leaf no-clobber; it
does not overstate complete handle-relative filesystem isolation.

## Consequences

- A requested Compiled Vault destination has exactly one namespace winner.
- A race loser cannot replace an empty directory or symlink installed after a
  preflight check.
- Source/output overlap is rejected before staging can mutate a source Vault.
- Operators receive an explicit path and both causes if failed staging cannot
  be disposed as requested.
- Supported-platform dependencies grow by target-specific safe wrappers, while
  the compiler itself remains free of unsafe code.
- Descriptor-relative ancestor pinning, crash-orphan management, and the full
  supported-filesystem CI matrix remain visible follow-up work.
