---
title: OKC Release Procedure
status: normative-v1
owners:
  - release-maintainer
  - qa-security-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0007
  - ADR-0020
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
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
4. The SDK binding workflow produces CPython `cp311-abi3` wheels for all four
   native targets, one `Cargo.lock`-bearing sdist, one root npm tarball, and the
   four platform-addon tarballs. Every package set has SHA-256 and CycloneDX
   SBOM evidence.
5. Clean environments install the built wheel and sdist and both root/platform
   npm tarballs. Tests cover CPython 3.11 through every supported stable minor,
   Node.js 22.13.0 and the current supported line, Python type stubs,
   ESM/CommonJS imports, and TypeScript declarations.
6. CLI, Python, and Node.js run the shared fixtures with identical artifact
   bytes, identities, provenance, verification, and explanation results. The
   complete V3 human-approval workflow and frozen V1/V2 read-only fixtures pass
   in both language packages.
7. Both macOS binaries pass Developer ID verification and Apple notarization.
8. The Windows PE passes Authenticode verification and carries a valid RFC3161
   timestamp. cargo-dist's SSL.com production signer is configured, but its
   protected-environment credentials are never available to pull requests.
9. Release notes contain the source commit and pinned Rust, cargo-dist,
   Maturin, napi-rs, Python, and Node.js toolchains. Every distributable
   declares `MIT OR Apache-2.0`, and every native release archive contains both
   license texts.

The `okc-compiler` name on PyPI and npm MUST be revalidated immediately before
any candidate publication. Registry credentials and publication steps belong
in a separate reviewed, protected workflow; `.github/workflows/sdk-bindings.yml`
is build-and-verification only and MUST remain unable to publish.
The official registry JSON endpoints returned HTTP 404 for both names on
2026-09-06; this observation is recorded in the
[`time-sensitive facts register`](references/TIME_SENSITIVE_FACTS.md) and does
not reserve either name.

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
evidence, remote language-version/native-package matrix evidence, and QG-006
benchmark evidence do not exist in this checkout. Local macOS arm64 wheel,
sdist, root npm tarball, platform-addon tarball, clean-install, type, checksum,
and SBOM checks pass, but neither language package is published. The V3
semantic scale, Pack/non-Markdown materialization, manual-section review,
schema-3 command adapter, provider conformance, and TUI PTY/platform gates in
the testing specification are also open.
Therefore publishing stable `0.3.0` is currently prohibited.
