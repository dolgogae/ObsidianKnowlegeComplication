---
title: Public SDK and CLI Contract
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0004
source_refs:
  - HIST-COMPILER-PLAN
---

# Public SDK and CLI Contract

## SDK workflow

The public API exposes explicit phases rather than one opaque merge call:

```rust,ignore
let session = VaultCompiler::builder()
    .workspace(".vaultc-work/build.sqlite")
    .policy(Policy::from_file("vaultc.toml")?)
    .build()?;

let inspection = session.inspect([
    Source::directory("personal", "./PersonalVault"),
    Source::archive("team", "./TeamVault.zip"),
])?;

let draft = session.plan(&inspection)?;
write_json("plan.json", &draft)?;

// Optional: another application invokes any provider and supplies proposals.
let validated = session.validate_proposals(&draft, proposals)?;
let approved = session.approve(validated, approval_log)?;

let artifact = session.compile(&approved, "./CompiledVault")?;
session.verify(&artifact)?;
let explanation = session.explain_provenance(&artifact, "knowledge/topic.md")?;
```

The concrete Rust API may evolve before `1.0`, but phase separation, immutable typed values, and error boundaries are normative.

## Public operations

| Operation | Input | Output | Mutates destination? |
|---|---|---|---|
| `inspect()` | sources + safety policy | sealed snapshots + diagnostics | no |
| `plan()` | snapshots + compilation policy | draft plan | no |
| `augment()` | plan projection + provider | transcript + proposals | no |
| `validate_proposal()` | plan + proposal | validation record | no |
| `approve()` | plan + decisions | approved plan | no |
| `compile()` | approved plan + new destination | compiled artifact | yes, atomically |
| `verify()` | artifact | verification report | no |
| `explain_provenance()` | artifact + output selector | evidence graph/explanation | no |

## CLI commands

```text
vaultc inspect SOURCE... --format human|json
vaultc plan SOURCE... --out plan.json
vaultc augment plan.json --provider-cmd PROGRAM --out proposals.jsonl
vaultc approve plan.json --decisions decisions.json --out approved-plan.json
vaultc compile approved-plan.json --output PATH [--pack FILE]
vaultc verify PATH_OR_PACK --format human|json
vaultc explain PATH_OR_PACK OUTPUT_PATH --format human|json
```

The final flag names may be refined, but every operation MUST have machine-readable output and stable exit-code families. Human output goes to standard output, diagnostics to standard error, and secrets or complete source content MUST NOT be logged by default.

## Exit behavior

- `0`: requested operation completed and policy permits all diagnostics.
- nonzero families: invalid invocation, unsafe/unreadable input, planning conflict requiring action, invalid/stale proposal, destination error, verification failure, internal error.

Exact numeric codes will be frozen before CLI beta and documented in generated reference material.

## Compatibility

Rust APIs follow SemVer. JSON schemas and NDJSON envelopes carry independent schema versions. CLI human text is not stable; JSON field meanings and diagnostic codes are. Deprecations require one minor-version migration window before `1.0` where practical and two after `1.0`.

## Language bindings

V1 offers the Rust crate and CLI/JSON/NDJSON interoperability. Python and Node bindings are later conveniences, not separate implementations. Other languages SHOULD invoke the CLI or implement the subprocess protocol until stable bindings exist.
