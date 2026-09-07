# Reverse Engineering — Architecture (Summary)

> **Authoritative source**: [`docs/specs/framework-architecture.md`](../../../docs/specs/framework-architecture.md).
> This file is a thin summary/index for AI-DLC continuity — read the spec for the normative version.

## Shape

OKC is **framework-first with thin adapters** (ADR-0002): a deterministic Rust core does the
compilation work, and CLI/TUI/SDK/MCP are thin surfaces over it. AI is provider-neutral and
isolated behind an interface (ADR-0004), so the core stays deterministic while the LLM does the
semantic work under guardrails.

```text
 surfaces │ okc (CLI + TUI)   bindings/python   bindings/node    (future: MCP adapter)
──────────┼──────────────────────────────────────────────────────────────────────────
   app    │ okc-app          orchestration / shared services (CLI & TUI call the same code)
──────────┼──────────────────────────────────────────────────────────────────────────
   core   │ okc-core         deterministic compilation: snapshot, parse, dedup, conflicts,
          │                  provenance, materialization, pack  (unsafe_code = forbid)
    ai    │ okc-ai           provider-neutral AI interface (record/replay, profiles, critic)
 interop  │ okc-interop      shared types across the binding boundary
```

## Load-bearing invariants (from ADRs / specs)

- **Immutable inputs, new output** (ADR-0003): sources are never mutated; output is a fresh Vault.
- **Compiled output excludes raw sources** (ADR-0006).
- **Deterministic core, provider-neutral AI** (ADR-0004): same inputs → same golden output SHA;
  AI variability is bounded and recorded (record/replay, ADR-0011/0023).
- **Typed provenance & audit envelope** (ADR-0010) and **immutable conflict decision overlays**
  (ADR-0009, ADR-0017): every block's disposition is traceable.
- **cwd-workspace worker + keychain boundary** (ADR-0025): `okc` run in a folder treats that
  folder as the workspace (Claude Code-style); secrets handled at a defined boundary.
- **V3 AI-required format and legacy boundary** (ADR-0022) with **evidence-complete integration
  and critic gate** (ADR-0024).
- **Current-schema single source** (ADR-0027): main holds only Schema 3; V1/V2 preserved via git
  history and archive tags.

## Determinism anchor

`docs/CURRENT_STATE.md` records the deterministic golden output SHA
`452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5` (verify against the current
snapshot before relying on it).

## Related specs

- Compilation pipeline: [`docs/specs/vault-compilation-pipeline.md`](../../../docs/specs/vault-compilation-pipeline.md)
- Canonical knowledge IR: [`docs/specs/canonical-knowledge-ir.md`](../../../docs/specs/canonical-knowledge-ir.md)
- Provenance & conflicts: [`docs/specs/provenance-and-conflicts.md`](../../../docs/specs/provenance-and-conflicts.md)
- AI provider & augmentation: [`docs/specs/ai-provider-and-augmentation.md`](../../../docs/specs/ai-provider-and-augmentation.md)
- Security & trust boundaries: [`docs/specs/security-and-trust-boundaries.md`](../../../docs/specs/security-and-trust-boundaries.md)
- Compiled vault & pack: [`docs/specs/compiled-vault-and-vaultpack.md`](../../../docs/specs/compiled-vault-and-vaultpack.md)
