---
title: Security and Trust Boundaries
status: normative-v1
owners:
  - qa-security-engineer
last_updated: 2026-09-05
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0009
  - ADR-0010
  - ADR-0012
  - ADR-0013
  - ADR-0014
  - ADR-0017
  - ADR-0019
  - ADR-0020
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
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
- Decode no path lossily. V2 validates ZIP central-directory filename bytes as
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
- Permit output-changing manual actions only for link ambiguity and only when
  the typed target is an exact sealed candidate; never accept replacement text.
- Escape ANSI, OSC, C0/C1, DEL, and bidi controls before TUI rendering. Do not
  emit OSC8, OSC52, or terminal-title controls.
- Redact secrets and source text from default logs and traces.
- Before schema-3 disclosure, scan every Markdown block with a versioned
  sensitive-data scanner. Store category, location, range, and content hash,
  never the matched secret text.
- If any effective finding exists, require local embedding and organizer
  routes. Require local synthesis and critic routes for each affected cluster.
- Treat only loopback endpoints and direct command adapters as local. LAN hosts
  are remote; remote HTTP requires TLS, OS certificate validation, no redirect,
  bounded response/deadline, and one-run consent.
- Store only API-key environment variable names or OS-keychain account
  references under the fixed OKC service ID. Never serialize resolved key
  values or lengths, or include authorization header values in `Debug`, screen,
  recording, or provider errors. A locked or unavailable keychain MUST NOT
  trigger plaintext file fallback.

### Materialization and distribution

- Write only into a validated sibling staging directory and publish atomically.
- Reject existing destinations by default.
- Reject a Compiled Vault or integrated Pack equal to, containing, or nested
  within an immutable source before creating its parent or staging entry.
- Publish Compiled Vault directories with the supported platform's atomic
  no-replace primitive. Preserve every file, directory, symlink/reparse point,
  and race winner; never fall back to a replacing rename.
- For OKCPack files, verify the source and staged pack, publish with an
  atomic no-replace primitive, never follow an existing leaf symlink, and
  report post-commit durability failure separately from pre-commit absence.

### Future installation clients

- Verify every pack member and checksum before installation.
- Display permissions, licenses, source attribution, and signature status.
- Never execute pack contents.

## Client and service scanning

The schema-3 CLI implements deterministic detection for private keys, common
API/cloud credentials, connection strings, email addresses, and phone numbers.
Persisted hash-bound false-positive exceptions, malware scanning, and broader
signature coverage are still required. A future upload client SHOULD repeat
these checks before transfer; server-side or local-worker scanning remains
necessary because client declarations are untrusted.

## Isolated processing profile

For a future on-premise service, each untrusted processing job should run non-root, with source mounted read-only, no host paths, no privilege escalation, default-deny outbound network, CPU/memory/time/PID limits, seccomp/AppArmor or equivalent, ephemeral workspace, and explicitly scoped result storage. Docker Compose deployment must preserve these properties; Kubernetes is not assumed.

## Governance

Publishers must attest ownership and distribution rights. Source author/license/provenance survives compilation. Takedown, deletion, pack revocation, and the status of already-derived artifacts require recorded policies; they are not solved by technical hashing alone.

## Incident posture

Verification failures are fail-closed. Logs identify diagnostic codes and content identities without leaking content. Signing keys, provider tokens, and service credentials never enter OKCPack contents or reproducibility manifests.

## Current `0.2.0` hardening boundary

The implemented scanner rejects/excludes unsafe logical paths, symlinks,
special files, named secret files, executable extensions, duplicate archive
members, strict raw ZIP/tar/PAX names, per-file/total/every-member limits,
source container bounds, and ZIP or complete-stream `tar.zst` expansion-ratio
breaches.
OKCPack extraction rejects non-regular outer files and members, bounds the
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
publication whose parent-directory durability is uncertain. ADR-0014 applies
the same exclusive-commit principle to Compiled Vault directories, requires
source/output disjointness before staging, and requires explicit staging
disposition errors rather than ignored cleanup failures.

This is not yet the full release threat model:

- source files are not opened through a portable handle-relative/no-follow API,
  so filesystem race hardening remains incomplete;
- accepted source entries are still buffered before sealing, and nested
  archives are opaque assets, so extraction nesting depth is zero rather than
  recursive;
- final-leaf no-replace directory publication is specified for Linux, macOS,
  and Windows, but its full supported-platform/filesystem CI evidence and
  descriptor-relative ancestor-race hardening remain incomplete;
- pack-file publication is locally verified on macOS, but Windows reparse
  points and the full supported-filesystem concurrency matrix are not yet;
- there is no completed fuzz/property campaign, malware scanner, PII/secret
  content scanner, protected dependency audit, or executed native
  signing/notarization evidence; cargo-dist is configured to generate SBOM and
  attestations, but configuration is not release evidence;
- current provider cancellation/process-tree E2E evidence is Unix-only.

These limitations MUST remain visible in release status until their controls
and platform tests exist. They do not authorize weakening any mandatory
control above.

The provenance graph retains recognized author/license declarations from
Markdown frontmatter without inferring their meaning. The manifest carries
media types, byte lengths, raw/content hashes, attribution summary, creation
policy, and distribution metadata. It deliberately records user Packs as
unsigned and is not a native application-signature verifier.
