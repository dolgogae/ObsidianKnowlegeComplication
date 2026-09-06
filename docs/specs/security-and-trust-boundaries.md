---
title: Security and Trust Boundaries
status: normative
owners:
  - qa-security-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0012
  - ADR-0014
  - ADR-0019
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-ONPREM-STACK
---

# Security and Trust Boundaries

## Threat model

Vaults, archives, filenames, Markdown, YAML/JSON, Canvas/Base data,
attachments, links, provider output, manifests, project journals, and future
package/registry metadata are hostile. Primary threats are traversal, symlink
escape, archive bombs, parser denial of service, malformed Unicode, secret
leakage, prompt injection, executable content, output overwrite, provenance
forgery, stale authority, dependency compromise, and source exfiltration.

## Files and archives

- Reject absolute, drive-prefixed, NUL, parent-traversing, reserved,
  non-portable, overlong, duplicate, case-fold-unsafe, and NFC-colliding paths.
- Never follow source symlinks. Reject special files and unsafe output links.
- Decode paths strictly; lossy CP437/replacement decoding is forbidden.
- Exclude `.obsidian/plugins/**`, `.git/**`, executables, sockets/devices, and
  named secret classes.
- Bound compressed and expanded bytes, member count/size, path depth/length,
  and expansion ratio before and during archive processing.
- Recheck file type, containment, size, and hash when bytes are reopened.
- Prefer descriptor-relative no-follow traversal where portable APIs permit;
  the remaining cross-platform TOCTOU gap is a release blocker.
- On Linux/macOS, pin source roots by handle and use no-follow opens for each
  member component and archive leaf. Check root/file identity across opening
  and regular-file type on the acquired handle. Enumeration remains path-based;
  this is not a claim of complete filesystem isolation.
- Do not load ambient/global/parent ignore rules or execute source `.ignore`
  directives. The sealed policy alone determines source membership.

## Content and providers

- Treat source text as data, never authority to invoke tools, HTTP, shell,
  deletion, approval, or policy changes.
- Validate provider output against current schemas, bounds, IDs/hashes,
  evidence closure, and safe paths before storing it as a proposal.
- Require independent critic output and explicit hash-bound human approval.
- Escape ANSI, OSC, C0/C1, DEL, and bidi controls before terminal rendering;
  never emit OSC8, OSC52, or terminal-title controls from hostile data.
- Scan every Markdown block and frontmatter key/value before disclosure,
  including metadata-only documents, and store category, location,
  range, and content hash only—not matched secret text.
- Route embeddings/organizer locally when any effective sensitive finding
  exists and route synthesis/critic locally for affected clusters.
- Treat exact `localhost` and parsed loopback IP addresses as local; DNS names
  merely beginning with `127.` are not loopback. LAN or hosted endpoints are remote
  and require TLS verification, bounded I/O/deadline, no credential-bearing
  URL, and explicit consent for that call.
- No command-provider kind or implementation is exposed by the current product.

## Credentials and language runtimes

The CLI/TUI stores only an environment-variable name or opaque OS-keychain
account under the fixed service ID. Python and Node.js accept environment names
only. Raw secret fields and credential-like option keys are rejected.

Resolved values live only in redacted/zeroizing types and MUST NOT enter files,
SQLite, provider recordings, provenance, errors, debug output, screen output,
or progress events. Keychain errors never fall back to plaintext.

Language callers provide absolute project, source, output, and artifact paths.
Adapters do not infer cwd authority, prompt, print, install signals/tracing,
run updates, or open a native keychain. Remote cache misses require explicit
`allow_remote_provider` and `remote_disclosure_confirmed` on each call; neither
is persisted.

## Projects, artifacts, and publication

- Project manifests accept current Schema 3 only and journals use explicit
  private migrations. Paths and mutable operations remain single-writer locked.
- Before opening SQLite or project objects, reject symlinked/non-regular
  managed files, object/workspace directory aliases, and unsafe database
  sidecars. Recheck object directories and files on access. Build-workspace
  databases MUST be outside every source, including existing ancestor aliases;
  this check precedes directory creation and is repeated before persistence.
- On Unix, managed regular files and SQLite sidecars must have a link count
  of one. Reject aliases before SQLite, object access, or permission updates.
  Windows equivalent evidence and links introduced after this metadata check
  remain release blockers.
- Commit configuration/source invalidation before manifest replacement and
  update the in-memory manifest only on success. A partial update may require
  reintegration; it must not leave changed configuration with old live approval.
- Artifact detection precedes decoding and is bounded to a 1 MiB manifest
  header read.
- Root, `.okc` marker, and manifest symlinks are rejected. Mixed markers,
  malformed/oversized JSON, unknown families/schemas, corrupt inventories, and
  unsafe explanation paths fail closed.
- Verification derives its allowed file inventory from the approved plan, not
  the supplied manifest. Recomputed checksums cannot authorize an additional
  file. Approved-plan reads are bounded to 512 MiB; other file reads are bounded
  to their exact regenerated lengths. Unexpected files/directories are rejected
  before their content is opened.
- Recognizable Schema 1/2 markers receive only the structured unsupported
  result; their content is not interpreted.
- Compile writes only within a validated sibling stage and publishes an absent
  directory atomically without replacement. A race winner is preserved.
- Synchronize every staged directory bottom-up on Unix, retain the stage guard
  until the exclusive commit, and explicitly report failed cleanup with its
  exact stage path and original cause. Disarm the guard immediately after
  publication; never remove an actor's reuse of the former staging name.
- Source/output overlap is forbidden. Publication never executes output
  content and does not imply publisher authenticity.

## Resource and concurrency controls

Ingestion and provider limits are hard errors. The worker pool and language job
queues are bounded to 64 waiting jobs per client, in addition to its bounded
workers. Queue overflow returns retryable `RESOURCE_LIMIT`. At most 64 progress
events are retained per interop job;
terminal state/result is stored separately. Same-project in-process mutation
is excluded across client instances by canonical path, with reservations
released before the terminal result is visible and the on-disk lock remaining
inter-process authority. Cancellation is honored before the publication
barrier and reported as too late afterward.

Worker completion is retained independently of the progress queue. An
unconsumed full queue MUST NOT deadlock completion, destruction, or shutdown.

## Future installation clients

Any future package client must validate all members and checksums before
installation, display permissions/license/attribution/signature state, enforce
archive bounds again, and never execute package contents. User artifact
integrity is distinct from native application signing.

## Current limitations

- Accepted source entries are buffered under bounds; the 20 GB streaming gate
  is not met.
- Source content no-follow opens are implemented for Linux/macOS; directory
  enumeration, managed-state/output ancestor pinning, and Windows reparse-point
  evidence remain incomplete.
- Fuzz/property campaigns, malware scanning, broader sensitive-data corpora,
  process-crash injection, and supported-filesystem race matrices remain.
- Provider-backed PTY cancellation and all-host native signing/notarization
  evidence remain.
- Current materialization is Markdown-only; attachment/Canvas/Base and Pack
  paths are not exposed.

These gaps stay visible in `CURRENT_STATE.md` and cannot justify weakening any
mandatory control above.
