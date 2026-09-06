---
title: Public SDK and CLI Contract
status: normative
owners:
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0001
  - ADR-0004
  - ADR-0019
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-COMPILER-PLAN
---

# Public SDK and CLI Contract

## Rust surface

`okc-core` exposes one current-schema path:

```rust,ignore
use okc_core::{CorpusBuilder, SourceSpec};

let prepared = CorpusBuilder::new()
    .workspace(".okc-work/build.sqlite3")
    .build([
        SourceSpec::directory("personal", "./PersonalVault")?,
        SourceSpec::archive("team", "./TeamVault.zip")?,
    ])?;
```

`PreparedCorpus` contains the sealed `IntegrationCorpus`, a stable block-text
projection, and source count. Intermediate source inspection and planning are
private. `okc_core::integration` exposes generation-neutral DTOs and
`compile`, `verify`, and `explain`. Existing Schema 3 serialized field names,
identity domains, paths, and bytes remain unchanged.

No retired compiler API, alias, reader, migration, generic artifact family,
Pack writer, augmentation protocol, or command-provider API is public.

## CLI surface

The product version is `0.3.0`; `--project PATH` is the only global option.

```text
okc [--project PATH]
okc tui [--project PATH]
okc project create PATH --name NAME --curator ID
    [--policy-version VERSION] [--language BCP-47]
okc --project PATH project source add SOURCE_ID SOURCE_PATH [--owner NAME]
okc --project PATH project ai-route set [ROLE] PROFILE
okc provider add|list|show|test|remove
okc --project PATH integrate [--allow-remote-provider --yes]
okc --project PATH integration status [--format human|json]
okc --project PATH review taxonomy show|export|approve
okc --project PATH review cluster list|show|export|approve|regenerate
okc [--project PATH] compile [--integration-plan FILE] --output DIRECTORY
    [--format human|json]
okc verify DIRECTORY [--format human|json]
okc explain DIRECTORY OUTPUT_PATH [--format human|json]
okc doctor
okc update [stable|latest|VERSION] [--yes]
```

Without a subcommand, a TTY starts the TUI. A non-TTY prints help and exits 2.
Commands that need a project use the explicit global path or deterministic cwd
discovery. Ambiguous or missing non-interactive discovery fails rather than
selecting silently.

Compile reads the selected project's latest approved plan unless
`--integration-plan` is supplied. It makes no provider call, requires an absent
output directory, and publishes only a current Schema 3 directory. Verify and
explain accept only such directories; explain requires exactly one output path.

The following names and shapes MUST remain absent: `inspect`, `plan`,
`augment`, `replay`, `validate`, the retired top-level `approve`, project
upgrade, `--policy`, `--workspace`, a positional approved plan, `--pack`, Pack
input, and explanation package/pagination/cursor options. Clap parse/help tests
are the executable contract for this absence.

## Python and Node.js API v1

The runtime-neutral `okc-interop` facade backs thin PyO3 and napi-rs adapters.
Python exposes snake_case; Node.js exposes camelCase. Every result/progress DTO
uses `interop_schema_version = 2`. Scalar job state/cancel values and the
structured error object are not DTO envelopes.

| Concept | Python | Node.js |
|---|---|---|
| package/module | `okc-compiler` / `okc` | `okc-compiler` |
| client | `okc.OkcClient` | `OkcClient` |
| project | `okc.Project` | `Project` |
| asynchronous work | `okc.Job[T]` | `Job<T>` |
| structured failure | `okc.OkcError` | `OkcError` |

Verification returns:

```text
Python: {interop_schema_version, valid, artifact_path, manifest}
Node:   {interopSchemaVersion, valid, artifactPath, manifest}
```

Explanation returns:

```text
Python: {interop_schema_version, artifact_path, record}
Node:   {interopSchemaVersion, artifactPath, record}
```

Python requires `explain_artifact(path, *, output_path=...)`. Node.js requires
`explainArtifact(path, {outputPath})`. Neither API exposes a generic JSON
artifact-family result. This schema-2 boundary is intentionally breaking and
has no interop-schema-1 alias.

All project, source, output, and artifact paths passed by a binding MUST be
explicit absolute lexical paths. Bindings do not search the cwd, prompt, print,
install tracing or signal handlers, invoke updates, or access native keychains.
They use the same project manifest, journal, immutable objects, and on-disk
lock as CLI/TUI.

Project and compile results can contain canonical absolute paths, including
Windows extended-length (`\\?\`) paths. Their spelling need not match the
caller-supplied path. Callers comparing locations SHOULD use filesystem path
identity; returned compile paths MUST be usable directly for verify/explain.

`OkcClient` accepts an immutable provider-profile set and 1–64 workers (four by
default), plus at most 64 queued jobs per client. Further submissions fail with
retryable `RESOURCE_LIMIT` without blocking the caller. Language profiles may name `api_key_env` but cannot carry raw secrets
or keychain references. Credential-like options and unknown provider kinds are
rejected. Environment secrets resolve at job start and are never stored.

Project operations cover create/open, manifest/status, source add/rebind/
replace, language and AI routes, preflight, integration, taxonomy review,
cluster review/regeneration, and compile. Remote integration or regeneration
requires both consent booleans for that call; consent is never persisted.

Every filesystem/provider/compiler operation returns `Job<T>`. States are
`queued`, `running`, `cancelling`, `publishing`, `completed`, `failed`, and
`cancelled`. At most 64 progress events are retained. Cancellation is effective
before publication, too late at/after the barrier, and already finished after a
terminal state. A second in-process mutation of the same canonical project
returns `PROJECT_BUSY`; the disk lock remains inter-process authority.
This exclusion spans distinct client instances and filesystem aliases, and the
reservation is released before job completion is observable. Full progress or
work queues do not make client finalization wait for free queue capacity.

Node.js converts contract field names to camelCase, but preserves arbitrary
provider option maps and source metadata value keys exactly, including distinct
`release_date` and `releaseDate` keys in one source value.

`OkcError` fields are `code`, `category`, `message`, `retryable`, and `details`.
Clients MUST branch on code/category, not parse text. Recognizable retired
artifacts return `ARTIFACT_SCHEMA_UNSUPPORTED` and details for supported schema
3, detected schema, and detected format family. All other verification hazards
use fail-closed verification errors.
Failed staging cleanup maps to `PATH_UNSAFE` in the `output` category, with
`staging_path`, `original_code`, and `io_kind` details; callers must not infer
that the failed stage was removed. No new interop code or schema is introduced.

Python targets CPython 3.11+ through `abi3-py311`; Node.js targets 22.13+
through Node-API 9. Release candidates require typing checks, tests, wheel and
sdist smoke, npm declaration and Pack smoke, checksums/SBOMs, clean installs,
and identical fixture inventory bytes across Rust, Python, and Node.js.

## CLI exit classes

| Exit | Meaning |
|---:|---|
| 0 | success |
| 2 | usage or invalid option |
| 3 | input/project/path failure |
| 4 | missing or stale decision/approval |
| 5 | provider or disclosure failure |
| 6 | output/publication failure |
| 7 | verification/explanation failure, including retired schema |
| 70 | unexpected internal failure |
