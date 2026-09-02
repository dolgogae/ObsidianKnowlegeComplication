---
title: ADR-0020 — Install, Update, and Native Signing Policy
status: normative-v1
owners:
  - release-maintainer
  - qa-security-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0007
  - ADR-0020
  - ADR-0021
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0020: Install, Update, and Native Signing Policy

## Status

Accepted on 2026-09-02.

## Context

Internally verifiable user Packs do not authenticate the application binary.
Installation and update channels need explicit provenance and platform-native
trust without silently overwriting cargo or manual installations.

## Decision

The official repository is `dolgogae/okc`. cargo-dist 0.32.0 produces Linux
x86_64, Windows x86_64, macOS x86_64, and macOS arm64 archives plus shell and
PowerShell installers. Releases include SHA-256 sums, SBOM, licenses, source
commit, toolchain manifest, and GitHub artifact attestation.

Receipt-aware updates use axoupdater 0.10.0. Startup may check only; installation
requires user confirmation. `okc update` supports stable, opt-in latest, and an
exact semantic version. Cargo/manual installs are not overwritten. No telemetry
is collected.

macOS release binaries require Developer ID signing and Apple notarization.
Windows PE files require Authenticode with RFC3161 timestamping. Credentials
exist only in a protected GitHub Environment. A release runs only for an
approved tag at the full expected commit SHA. Any QG-001 through QG-008,
attestation, signing, or notarization failure blocks publication of stable
`0.2.0`.

User-created `.okcpack` files remain deterministically verifiable but are not
publisher-signed in this version; release-binary signing is a separate trust
boundary.

## Consequences

- Native signatures are mandatory release evidence, not local-development
  claims.
- The repository may contain release automation while stable publication stays
  blocked until protected CI proves every gate.
