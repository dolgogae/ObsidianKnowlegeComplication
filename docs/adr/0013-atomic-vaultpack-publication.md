---
title: ADR-0013 — Atomic VaultPack Publication
status: normative-v1
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-08-18
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0007
  - ADR-0013
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0013: Atomic VaultPack Publication

## Status

Accepted on 2026-08-18.

## Context

The unpublished `0.1.0` SDK pack writer checks whether its destination exists
and then opens that final path with create/truncate semantics. A concurrent
creator can win between those operations, a dangling symlink is reported as
non-existent and may be followed, and an encoder or I/O failure can leave a
partial file at the requested `.vaultpack` path. The CLI partially avoids
those failures with a private temporary-directory and hard-link wrapper, so
SDK and CLI do not share one publication contract.

The public `CompileOptions` type already contains an optional pack path but is
not connected to the compiler facade. It is also important not to describe a
Compiled Vault directory and a pack at an unrelated path as one atomic
filesystem transaction. Two namespace entries, possibly on different
filesystems, cannot be portably committed as one operation.

## Decision

### One owner for pack publication

`vaultc::pack::create_pack` owns deterministic pack construction and
publication for SDK and CLI callers. It keeps its current public arguments but
MUST implement the following sequence:

1. validate the sealed zstd profile, independently verify the Compiled Vault, and validate the source/destination relationship;
2. create the destination parent when necessary;
3. create a unique regular temporary file in that same parent with restrictive permissions;
4. write the deterministic tar/zstd stream to the temporary file;
5. finish the archive and encoder, flush it, and synchronize the file;
6. independently verify the staged pack;
7. publish it with an atomic no-replace operation;
8. synchronize the parent directory where the platform policy supports it.

The destination extension MUST be ASCII-case-insensitively `.vaultpack`.
The destination MUST be disjoint from the Compiled Vault: equality, a pack
inside the Vault, or a Vault inside the path reserved for the pack is rejected.
Comparison resolves the existing ancestor prefix and then applies lexical
normalization plus the V1 portable NFC/full-case-fold component key so
existing symlink aliases, `..` spellings, or a case/normalization alias on a
supported filesystem do not bypass the check. This is a preflight guard, not a
claim that descriptor-relative ancestor races are closed.

Any existing leaf entry is `OutputExists`, including a regular file,
directory, live symlink, dangling symlink, or another supported reparse-point
entry. The compiler MUST NOT follow that leaf or create its referent. A race
loser receives `OutputExists`; the winner's entry is not modified.

The no-replace commit uses `tempfile::NamedTempFile::persist_noclobber` or a
semantically equivalent safe primitive. An implementation MUST NOT fall back
to `exists` followed by replacing `rename`. If the current platform or
filesystem cannot provide no-replace behavior, publication fails closed.

The deterministic archive writer is an internal operation separate from the
public publisher. Canonical-pack verification may rebuild a candidate in an
isolated temporary location without recursively invoking public verification
or publication policy. Public pack creation verifies the input directory and
the completed staged pack; it does not authenticate a publisher. Distribution
callers still verify the final path and apply the future signing policy.

### Failure and durability semantics

Before the no-replace commit, every caught validation, encoding, I/O,
file-sync, staged-verification, or publication failure leaves no file created
by vaultc at the requested destination. A concurrent actor's winning
destination is preserved. The in-process temporary file is removed on caught
failure. Process kill or power loss may leave a hidden sibling staging file;
orphan discovery and cleanup are a later maintenance policy and MUST never
infer that an arbitrary user file is safe to delete.

After the no-replace commit, rollback is unsafe and is not attempted. If a
required parent-directory synchronization then fails, the complete pack stays
published and the SDK returns a distinct
`PublishedButDurabilityUncertain { path, source }` error. Callers can therefore
distinguish an absent pre-commit failure from a complete, process-visible
publication whose survival across immediate power loss is uncertain. Retrying
the same destination observes `OutputExists`. The existing Compiled Vault
directory publisher adopts the same post-commit error state; this clarifies
durability without claiming that its separate no-replace race is closed.

On Unix local filesystems the implementation synchronizes the parent
directory. The safe Rust V1 implementation does not claim a Windows directory
flush or physical power-loss guarantee; Windows success means the completed
file was synchronized and atomically published without replacement. Network,
FUSE, and other filesystems are not release-qualified until their primitive
and durability behavior is tested explicitly.

### Compile-plus-pack orchestration

`VaultCompiler::compile_with_options` connects the existing
`CompileOptions { create_pack: Option<PathBuf> }`. `VaultCompiler::compile`
delegates with default options. When a pack is requested, the compiler
performs all detectable pack preflight checks before publishing the Compiled
Vault, publishes the verified Compiled Vault directory, then calls the same
public atomic pack publisher.

These remain two ordered publications, not a combined transaction. A runtime
pack failure after Compiled Vault publication returns
`PackPublicationAfterCompile { compiled_vault, pack, source }`, leaves the
valid Compiled Vault intact, and leaves no vaultc-created partial pack at the
requested destination. A nested post-commit durability error may leave both
complete outputs visible. Automatic deletion of the Compiled Vault would be a
false rollback and is forbidden.

A future all-or-nothing release needs a single containing release directory,
commit marker, or other versioned bundle format and a separate ADR. It cannot
be inferred from `CompileOptions`.

The CLI delegates semantic path validation, staging, and publication to this
core API. It may retain argument-level `.vaultpack` validation so an invalid
extension remains usage exit `2`; all destination, compilation, and runtime
pack publication failures remain exit `6`, except internal invariants at
exit `70`.

### Acceptance and compatibility

The acceptance suite MUST cover deterministic bytes, existing file/directory
and live/dangling symlink leaves, both containment directions, a deterministic
write failure, staged verification, a publish race, concurrent creators with
exactly one winner, retry after failure, and compile-plus-pack residual state.
Fault injection MUST prove the order `write -> finish -> flush -> file sync ->
staged verify -> no-replace publish -> parent sync` without relying on
disk-full timing.

Linux, macOS, and Windows local-filesystem CI MUST exercise concurrent
no-clobber publication. Symlink/reparse tests are required only where the CI
runner has permission to create them, and unsupported coverage must be
reported rather than assumed. Physical power-loss durability is not proven by
unit tests.

This is a pre-release completion of the `0.1.0` SDK behavior. It changes no
Compiled Vault, manifest, provenance, tar, or zstd bytes and adds no serialized
schema field. The new facade method and explicit publication-state errors are
public Rust API additions before the first compatibility-qualified release.
`VaultcError` is marked non-exhaustive so later error detail can be added
without encouraging exhaustive downstream matches.

## Consequences

- SDK and CLI use one race-safe pack publication path.
- A requested pack path is never used as the encoder's partial output.
- Existing entries and symlink referents are not overwritten or followed.
- Pack failure state is honest without pretending that two destinations form
  one transaction.
- Portable atomic no-clobber publication of the Compiled Vault directory and
  descriptor-relative ancestor hardening remain separate release blockers.
