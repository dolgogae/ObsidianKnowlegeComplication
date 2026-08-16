---
title: Security and Trust Boundaries
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0009
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
members, per-file/total/count limits, and source ZIP expansion-ratio breaches.
It rechecks source type, size, and content hash before use. The CLI bounds
control files and provider I/O and validates serialized plans/approvals before
publication.

This is not yet the full release threat model:

- source files are not opened through a portable handle-relative/no-follow API,
  so filesystem race hardening remains incomplete;
- source archive compressed-byte size and total visited-member count, including
  excluded and non-file entries, are not separately bounded; nested archives
  are opaque assets, so extraction nesting depth is zero rather than recursive;
- VaultPack extraction enforces regular files and per-file/count/total expanded
  size, but not a compressed-to-expanded ratio for the outer zstd stream;
- a no-clobber destination check followed by a directory rename is not proven
  race-free on every supported platform;
- the public SDK pack writer writes directly to its destination; only the CLI
  wrapper currently stages and no-clobber-publishes a pack file;
- there is no fuzz/property campaign, malware scanner, PII/secret content
  scanner, dependency audit, SBOM, or signing/authenticity layer;
- current provider cancellation/process-tree E2E evidence is Unix-only.

These limitations MUST remain visible in release status until their controls
and platform tests exist. They do not authorize weakening any mandatory
control above.

The current manifest and provenance schemas also lack author/license
attribution, media types, creation metadata, and signature status. No
installation permission/license/signature display surface exists in `0.1.0`.
