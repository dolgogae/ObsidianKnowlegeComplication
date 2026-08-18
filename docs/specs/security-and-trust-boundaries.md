---
title: Security and Trust Boundaries
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-08-18
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0009
  - ADR-0010
  - ADR-0012
  - ADR-0013
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-ONPREM-STACK
---

# Security and Trust Boundaries

## Threat model

Input Vaults, archives, filenames, Markdown, YAML/JSON, Canvas, Bases, attachments, links, AI output, MCP output, packs, and registry metadata are untrusted. Local operators, configured signing keys, and the compiler binary are not assumed infallible; verification and audit records constrain damage.

Primary threats include path traversal, symlink escapes, archive bombs, parser denial of service, malformed Unicode, secret/PII leakage, prompt injection, executable/plugin upload, output overwrite, provenance forgery, stale approval reuse, dependency compromise, malicious pack publishing, and corporate Vault exfiltration.

## Mandatory controls

### Files and archives

- Reject unsafe path syntax and every collision that cannot be resolved by the sealed deterministic layout rule. Detect exact, case-fold, and Unicode-normalization collisions before materialization; never use last-writer-wins.
- Do not follow source symlinks by default; reject links escaping a declared root.
- Enforce file count, size, expanded size, nesting, and compression-ratio limits before/during extraction.
- Decode no path lossily. V1 validates ZIP central-directory filename bytes as
  strict UTF-8 before library CP437/replacement decoding, parses archive names
  without host separator semantics, and rejects unsupported raw encodings.
- Exclude `.obsidian/plugins/**`, `.git/**`, executables, sockets, devices, and named secret classes.
- Use no-follow/open-relative primitives where platform APIs permit; recheck destination containment.

### Content and AI

- Treat retrieved text as DATA, never authority to invoke MCP, HTTP, shell, deletion, approval, or policy changes.
- Minimize provider disclosures and make local/remote processing explicit.
- Validate provider output against versioned schemas, bound IDs/hashes, path rules, evidence closure, and size limits.
- Require explicit approval and invalidate it when bound inputs change.
- Keep conflict decisions in a plan/content-hash-bound overlay; never mutate a sealed plan to make an approval appear current.
- Redact secrets and source text from default logs and traces.

### Materialization and distribution

- Write only into a validated sibling staging directory and publish atomically.
- Reject existing destinations by default.
- For VaultPack files, verify the source and staged pack, publish with an
  atomic no-replace primitive, never follow an existing leaf symlink, and
  report post-commit durability failure separately from pre-commit absence.

### Future installation clients

- Verify every pack member and checksum before installation.
- Display permissions, licenses, source attribution, and signature status.
- Never execute pack contents.

## Client and service scanning

A future upload client SHOULD scan for cloud/API keys, private keys, credentials, connection strings, email/phone PII, dangerous extensions, and unexpectedly large files before transfer. Server-side or local-worker scanning remains necessary because client declarations are untrusted. Candidate tools include Gitleaks and ClamAV, but policy and signatures must be versioned.

## Isolated processing profile

For a future on-premise service, each untrusted processing job should run non-root, with source mounted read-only, no host paths, no privilege escalation, default-deny outbound network, CPU/memory/time/PID limits, seccomp/AppArmor or equivalent, ephemeral workspace, and explicitly scoped result storage. Docker Compose deployment must preserve these properties; Kubernetes is not assumed.

## Governance

Publishers must attest ownership and distribution rights. Source author/license/provenance survives compilation. Takedown, deletion, pack revocation, and the status of already-derived artifacts require recorded policies; they are not solved by technical hashing alone.

## Incident posture

Verification failures are fail-closed. Logs identify diagnostic codes and content identities without leaking content. Signing keys, provider tokens, and service credentials never enter VaultPack contents or reproducibility manifests.

## Current `0.1.0` hardening boundary

The implemented scanner rejects/excludes unsafe logical paths, symlinks,
special files, named secret files, executable extensions, duplicate archive
members, strict raw ZIP/tar/PAX names, per-file/total/every-member limits,
source container bounds, and ZIP or complete-stream `tar.zst` expansion-ratio
breaches.
VaultPack extraction rejects non-regular outer files and members, bounds the
outer compressed size, prechecks declared expansion, and rechecks streamed
expanded bytes against the ratio and aggregate limits before publication. It
rechecks source type, size, and content hash before use. The CLI bounds
control files and provider I/O and validates serialized plans/approvals before
publication. Canvas parsing rejects duplicate JSON object keys, duplicate node
IDs, malformed known node fields, and references that would escape the
allocated Compiled Vault root. All source-derived output operations carry a
sealed output commitment. Markdown rewrites additionally bind the exact source
spans, replacement recipe, expected output hash, reverse-reconstructed source
hash, and semantic reparse; missing required plan fields fail closed.
Approved generated notes similarly carry a compiler-derived materialization
commitment for their exact body/output bytes, destination, operation ID, and
EvidenceId list. The verifier reconstructs the complete note and requires
exact agreement among the approval, frontmatter, provenance ledger, and
checksummed output even when surrounding artifact metadata is resealed.
The typed provenance decoder rejects unknown/duplicate/non-canonical fields,
invalid identities, graph closure violations, oversized records or ledgers,
and semantic graph substitutions even when all surrounding unsigned envelope
hashes are recomputed. Explanation pages bind their subject and cursor and
enforce their byte limit over the complete serialized response.
SDK and CLI now use one Pack publisher that validates portable path aliases,
rejects either containment direction, verifies the source and staged archive,
and commits a synchronized sibling file without replacement. Its deterministic
fault seam distinguishes an absent pre-commit failure from a complete retained
publication whose parent-directory durability is uncertain.

This is not yet the full release threat model:

- source files are not opened through a portable handle-relative/no-follow API,
  so filesystem race hardening remains incomplete;
- accepted source entries are still buffered before sealing, and nested
  archives are opaque assets, so extraction nesting depth is zero rather than
  recursive;
- a no-clobber destination check followed by a directory rename is not proven
  race-free on every supported platform;
- pack-file publication is locally verified on macOS, but Windows reparse
  points and the full supported-filesystem concurrency matrix are not yet;
- there is no fuzz/property campaign, malware scanner, PII/secret content
  scanner, dependency audit, SBOM, or signing/authenticity layer;
- current provider cancellation/process-tree E2E evidence is Unix-only.

These limitations MUST remain visible in release status until their controls
and platform tests exist. They do not authorize weakening any mandatory
control above.

The provenance graph retains recognized author/license declarations from
Markdown frontmatter without inferring their meaning. The manifest still lacks
license summaries, media types, creation metadata, and signature status. No
installation permission/license/signature display surface exists in `0.1.0`.
