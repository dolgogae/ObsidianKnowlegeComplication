---
title: ADR-0031 — Cancellation, Recovery and Filesystem Capabilities
status: proposed
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0014
  - ADR-0025
  - ADR-0026
  - ADR-0027
  - ADR-0031
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0031: Cancellation, Recovery and Filesystem Capabilities

## Status

Proposed on 2026-09-06; not accepted or implemented. It does not assert that
all filesystem races, power-loss recovery or native-platform gates are closed.
Current output bytes and the source/private-state trust boundary remain in
force. Review entry point: [`ADR proposals`](README.md).

## Context and affected contracts

The application announces `Publishing` before entering the whole core compile
call. This closes cancellation while staging is still in progress. The core
now has private failure checkpoints and correct final-leaf no-replace behavior,
but public operation control does not share the actual barrier. Current
journal-before-manifest ordering conservatively invalidates authority; it is
not a recoverable transaction across SQLite and the filesystem.

Linux/macOS source-content reads use pinned handles. Enumeration, mutable
SQLite access, output ancestors and Windows reparse defenses remain partially
path-based. Testing race winners at the final leaf is not proof that every
ancestor or temporary tree is safe.

Affected: REQ-SNP-001, REQ-SEC-001, REQ-CMP-001, REQ-INT-005,
REQ-APP-001/002, REQ-SDK-002, REQ-REL-001; ALG-SNP-001, ALG-CNF-001;
QG-001/002/004/005/008. This is runtime/storage policy, not a semantic algorithm
promotion or an automatic artifact-schema change.

## Proposed decision

### 1. One real publication barrier

Add a minimal runtime-only core control interface with bounded progress,
cancellation checkpoints and an atomic publication transition. Plain `compile`
delegates to the same implementation with default non-cancelled control; it
does not gain provider/runtime/UI dependencies. The application and interop
adapt the same state rather than infer it from a progress event.

```text
queued -> running / staging / staged verification
                |                       |
                v                       v
       cancellation requested      try_begin_publish
                |                       |
                v                       v
         cleanup -> cancelled       publishing -> completed
                                        |
                                        +-> failed with actual publication state
```

Cancellation and `try_begin_publish` atomically compete on one operation state.
If cancellation wins, no publication syscall runs. If the core wins immediately
before the exclusive namespace operation, cancellation returns `too_late`.
Progress-queue delivery is not that linearization point. There is no window in
which a reported successful cancellation can still publish.

Check cancellation between bounded reads/writes, parser units, hash batches,
provider attempts, vector/candidate chunks and verification records. Propose
a <=250-ms checkpoint interval for CPU work under the reference load. Blocking
filesystem calls may not be interruptible; report that limitation explicitly
and do not advertise a guaranteed kill deadline. Transport deadlines remain
bounded across retries; cancellation cannot create a reusable partial response.

The non-cancellable state includes no-replace publication and required
postcommit durability/verification recording. A precommit error after acquiring
that state still cleans the exact stage and returns failed, not completed or
cancelled. A successful publish followed by sync/journal failure keeps the
verified output, reports its actual path/state, and never tries to roll it back.
Current error codes/DTO meanings must be reviewed before adding detail fields.

### 2. Recoverable configuration intent

Propose a versioned private journal upgrade, candidate schema 5, with explicit
configuration-intent and publication-observation records. Do not write new
schema-5 structures into a database still labelled schema 4. Projects retain
their public Schema 3 manifest shape; the private migration requires tests,
backup/recovery instructions and explicit compatibility review.

For a source/language/route change, under the writer lock:

1. Validate the entire candidate, source separation and expected current
   configuration hash before mutation.
2. Persist the candidate and previous canonical configuration as immutable
   project objects, then append a prepared intent and downstream invalidation
   in one SQLite transaction.
3. Atomically replace and synchronize the manifest from the candidate object.
4. Append an applied intent after confirming the persisted manifest hash.

On startup, inspect unfinished intents before admitting new work. If the
manifest equals the candidate, finish applied recording. If it equals the
previous hash, retry the prepared replacement using the immutable candidate;
if the exact safe write cannot complete, stop with recoverable pending state.
If it matches neither, stop for explicit conflict resolution—never guess which
external edit to overwrite. Any deliberate abandonment is a new recorded
decision retaining conservative invalidation, not restored old approval.

The migration first validates the old manifest/journal relation and records a
baseline; it cannot infer or fabricate missing approval. Source, config,
taxonomy and output identities remain unchanged for unchanged data. No source
is reopened merely to recover a manifest. A migration failure before the SQLite
commit leaves the old state intact. After that commit, recovery must report the
actual new schema and any durability uncertainty, not pretend the upgrade
never occurred or downgrade it automatically.

### 3. Publication observations and orphan handling

Before staging, append an operation intent binding plan hash, validated
destination-parent identity and operation ID. Record the exact created stage
identity once available. A process can die between those events; an unrecorded
or otherwise unprovable stage is reported for manual inspection, not deleted
by glob, age, prefix or guessed ownership.

After publication, append verified path/plan/durability observations. If the
process dies between rename and the journal update, restart may independently
verify the expected destination and bind it to the exact planned operation.
An absent or conflicting destination remains incomplete; it does not acquire
success from an intent record alone. Re-verification is provider-free.

No automatic orphan deletion is proposed. An explicit cleanup operation must
hold the writer lock and revalidate recorded parent/stage identity, private
ownership and containment through handles. A replaced identity, unknown child,
source overlap, active operation or ambiguous ownership stops cleanup. Preserve
the concurrent winner and unrelated user files. Failed cleanup reports the
exact retained path and both original/cleanup causes.

### 4. Filesystem capabilities, not optimistic path checks

Introduce narrow safe-Rust filesystem capabilities for validated source roots,
project storage roots, output parents and owned stages. Creation, opening,
verification and final commit resolve relative to those capabilities rather
than re-resolving an untrusted absolute path. Check opened-file type, link
count/identity and bounds at the point of use; reject symlinks/reparse points,
special files and containment aliases. Enumeration must not confer authority
to reopen a file through a different path.

SQLite needs special review: pinning its main file alone does not pin WAL,
SHM/journal creation or a library's later reopen. Qualify a restricted storage
directory plus a suitable SQLite VFS/platform mechanism before claiming this
boundary closed. A metadata check followed by path-based SQLite is not the
target design. No unsafe-code exemption or new native wrapper dependency is
approved here; safe wrappers require license, API and platform review.

Define the allowed concurrent-mutation threat model explicitly. A pinned
handle does not prove that a privileged external actor cannot relocate an
entire tree into a source or change filesystem semantics. Refuse unqualified
shared/writable-ancestor setups for strong guarantees; document actual platform
capabilities rather than declaring every path race solved.

Initially qualify local Linux, macOS and Windows NTFS with exact filesystem,
kernel and privilege evidence. Network/FUSE/cloud-synchronized locations do not
inherit that qualification. Unsupported no-replace/no-follow/durability
capabilities fail closed; no replacing rename or plaintext-state workaround.

## Alternatives and consequences

- Earlier `Publishing` notification is simple but makes cancellation semantics
  depend on adapter timing; rejected.
- Killing a worker thread/process as normal cancellation can lose cleanup and
  journal state; process-kill remains a failure test, not the API contract.
- Pretending manifest plus SQLite is one atomic transaction hides an actual
  failure window. Explicit intent recovery makes that state inspectable.
- Automatically sweeping `.okc-stage-*` risks deleting user data; rejected.
- Capability/VFS work may require platform-specific dependencies and a private
  journal migration. These costs remain explicit acceptance decisions.

## Acceptance, rollout and rollback

Core/QA/architect owners must review the control interface and atomic race,
intent schema/recovery rules, native filesystem mechanism and supported threat
model. Release owners separately approve qualification evidence; acceptance is
not permission to access signing credentials or publish packages.

Planned deterministic tests cover cancellation at every precommit checkpoint,
cancel-versus-barrier winners, a full progress queue, postcommit cancel refusal,
sync/journal failures after publish, and process kill at every intent/manifest/
rename/journal boundary. Recovery must never reuse a partial provider exchange,
resurrect stale approval, remove an external winner or mutate source metadata.

Exercise source/managed/output ancestor replacement, hardlinks, Windows reparse
points, SQLite sidecars, unsupported capabilities, disk exhaustion and corrupted
ownership records on native supported hosts. PTY/ConPTY tests verify responsive
progress, cancellation state, terminal restoration and restart. Use fixed
barriers plus coverage-guided fuzz campaigns; a skipped privileged case is a
reported gap, not a passing assertion.

Update pipeline/security/SDK/testing specs and TRACEABILITY in implementation
changes. Keep artifact goldens unchanged. Rollback can disable new operation
entry points but cannot downgrade a migrated journal in place: restore a
verified pre-migration backup to a separate destination using the matching
binary, preserving later history and published outputs for inspection.
