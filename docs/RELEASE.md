---
title: OKC Release Procedure
status: normative-v1
owners:
  - release-maintainer
  - qa-security-engineer
last_updated: 2026-09-05
decision_refs:
  - ADR-0007
  - ADR-0020
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
source_refs:
  - HIST-CURRENT-PLAN
---

# OKC Release Procedure

Stable release publication is fail-closed. A candidate tag MUST identify the
full expected commit SHA and MUST NOT be published until all of the following
evidence is attached to that SHA:

1. QG-001 through QG-008 are green, including two consecutive complete CI
   matrix successes on the same commit.
2. `dist 0.32.0 plan` validates [`../dist-workspace.toml`](../dist-workspace.toml).
3. cargo-dist produces the four configured native archives, shell and
   PowerShell installers, SHA-256 files, source archive, CycloneDX SBOM, and
   GitHub artifact attestations.
4. Both macOS binaries pass Developer ID verification and Apple notarization.
5. The Windows PE passes Authenticode verification and carries a valid RFC3161
   timestamp. cargo-dist's SSL.com production signer is configured, but its
   protected-environment credentials are never available to pull requests.
6. Release notes contain the source commit and pinned Rust/cargo-dist
   toolchains. Every archive contains the dual-license texts.

Signing and notarization credentials MUST exist only in a protected GitHub
`release` Environment with required human reviewers. A workflow lacking those
credentials, a runner that cannot verify the native signature, or an artifact
that fails attestation MUST stop before GitHub Release creation.

The embedded updater accepts only a matching cargo-dist receipt, refuses a
cargo/manual executable, defaults to the stable channel, treats `latest` as an
explicit prerelease opt-in, and requires confirmation before installation.
No update check or installation emits telemetry. Per ADR-0021, the pinned
blocking adapter owns a short-lived current-thread runtime during that command;
the TUI and compiler never run on it.

## Current publication state

The config and receipt-aware client are present, but the protected signing
environment, Apple certificate/notarization workflow, remote two-pass CI
evidence, and QG-006 benchmark evidence do not exist in this checkout. The V3
semantic scale, Pack/non-Markdown materialization, manual-section review,
schema-3 command adapter, provider conformance, and TUI PTY/platform gates in
the testing specification are also open.
Therefore publishing stable `0.3.0` is currently prohibited.
