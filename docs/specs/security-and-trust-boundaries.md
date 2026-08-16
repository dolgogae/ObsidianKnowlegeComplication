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

- Reject absolute, parent-traversal, drive, UNC, NUL, normalization-colliding, overlong, and reserved output paths.
- Do not follow source symlinks by default; reject links escaping a declared root.
- Enforce file count, size, expanded size, nesting, and compression-ratio limits before/during extraction.
- Exclude `.obsidian/plugins/**`, `.git/**`, executables, sockets, devices, and named secret classes.
- Use no-follow/open-relative primitives where platform APIs permit; recheck destination containment.

### Content and AI

- Treat retrieved text as DATA, never authority to invoke MCP, HTTP, shell, deletion, approval, or policy changes.
- Minimize provider disclosures and make local/remote processing explicit.
- Validate provider output against versioned schemas, bound IDs/hashes, path rules, evidence closure, and size limits.
- Require explicit approval and invalidate it when bound inputs change.
- Redact secrets and source text from default logs and traces.

### Materialization and distribution

- Write only into a validated sibling staging directory and publish atomically.
- Reject existing destinations by default.
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
