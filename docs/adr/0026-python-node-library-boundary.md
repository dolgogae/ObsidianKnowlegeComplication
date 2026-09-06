---
title: ADR-0026 — Python and Node.js Library Boundary
status: normative-v1
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0002
  - ADR-0019
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0026: Python and Node.js Library Boundary

## Status

Accepted on 2026-09-06 for the first non-Rust public library surfaces.

## Context

The schema-3 project workflow was available only through the Rust application
and executable. Automations in Python and Node.js need the same project,
provider, approval, compilation, verification, and explanation behavior
without spawning the CLI or duplicating policy. Directly binding CLI modules
would import cwd discovery, prompts, terminal signals, native credential
storage, updating, and presentation into processes that do not authorize those
side effects. A broad C ABI would also create a third ownership, error, and
memory-safety contract without a current consumer.

Long-running provider and compiler operations cannot block a language runtime
or lose cancellation and publication state. Multiple clients may also open the
same existing `.okc-project`, so in-process scheduling must complement rather
than replace the existing on-disk writer lock.

## Decision

Add the runtime-neutral `okc-interop` crate between `okc-app` and language
adapters. It owns versioned DTOs, stable structured errors, `OkcClient`,
`Project`, and `Job<T>`. It contains no Python, Node.js, CLI presentation, cwd
discovery, prompt, signal-handler, updater, native-keyring, or tracing-subscriber
type. V1/V2/V3 artifact detection and read-only verify/explain dispatch move to
an `okc-app` application service used by both CLI and interop.

The Python adapter is a PyO3 `cdylib` distributed as `okc-compiler` and imported
as `okc`. It targets CPython 3.11 or newer through `abi3-py311`. The Node.js
adapter is a napi-rs `cdylib` distributed as `okc-compiler`, targets Node-API 9,
and provides ESM, CommonJS, and TypeScript declarations for Node.js 22.13 or
newer. Both adapters are thin JSON/DTO and exception-conversion layers. No
generic `okc-ffi` or public C ABI is created.

All filesystem paths accepted by these libraries are explicit absolute paths.
The bindings never discover a project or source from the process cwd. They use
the existing project manifest, private schema-4 state, and on-disk writer lock;
they add no binding-specific migration or canonical state.

An `OkcClient` receives an immutable provider-profile collection and a bounded
worker count, four by default and at most 64. Provider profiles may name an
`api_key_env` but cannot carry a raw secret or `os_keychain` reference. Rust
resolves the environment variable at each provider job. Command providers are
rejected until the schema-3 supervisor is implemented. Credential-like keys in
provider options are rejected, and option values are excluded from `Debug`.

Every filesystem, provider, or compiler operation returns `Job<T>`. Its public
states are `queued`, `running`, `cancelling`, `publishing`, `completed`,
`failed`, and `cancelled`. At most 64 progress events are retained; intermediate
updates may coalesce while the terminal result is stored separately. Cancel is
`requested` before the publication barrier, `too_late` at or after it, and
`already_finished` after termination. Mutating work on an already reserved
project fails immediately with `PROJECT_BUSY`; distinct projects may run in
parallel, and the disk lock remains the inter-process authority.

Remote cache misses in integration or cluster regeneration require explicit
`allow_remote_provider` and `remote_disclosure_confirmed` booleans for that
call. Consent is never persisted or inferred. Human taxonomy, omission,
critic-waiver, and cluster approval remains mandatory and hash-bound.

Interop DTO schema starts at `interop_schema_version = 1`. Python exposes
snake_case and Node.js exposes camelCase. Errors are
`{code, category, message, retryable, details}` and callers do not parse the
message. Initial codes cover argument/path, project/busy, provider/credential,
consent, approval/staleness, output/no-clobber, verification, cancellation, and
internal failures.

`okc-app`'s updater and native-keyring dependencies are optional features. The
CLI enables both; language bindings enable neither. Libraries print nothing to
stdout/stderr and install no global tracing subscriber.

The Node adapter permits the `unsafe` tokens generated inside napi-rs export
macros because Rust's lint expands them in the adapter crate. Handwritten
adapter and interop code contains no unsafe block; `okc-interop` continues to
forbid unsafe code.

Release candidates include wheels, an sdist, the root npm tarball, four
platform-addon tarballs, checksums, and SBOMs. Build and clean-install smoke
tests cover Linux x86_64 GNU, Windows x86_64 MSVC, macOS x86_64, and macOS
arm64. Registry publication and credentials are separate, explicitly reviewed
work and are not part of this decision.

## Consequences

- Python and Node.js can execute the complete currently implemented schema-3
  Markdown directory workflow without a CLI subprocess.
- CLI, TUI, and bindings share project and artifact policy instead of copying
  dispatch or approval rules.
- Native package maintenance is duplicated per language and platform, but the
  business boundary remains one Rust facade with a versioned DTO contract.
- Native keychain access remains a CLI/TUI capability; library hosts supply an
  environment-variable name and own their process environment policy.
- V1/V2 writers, V3 Pack, Go, JVM, and .NET APIs remain outside this surface.
- Passing binding tests does not make V3 stable. All pre-existing V3 release
  blockers and two-success protected release gates remain in force.
