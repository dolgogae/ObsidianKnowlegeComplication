# Reverse Engineering — Component Inventory

> Snapshot at commit `7f87f7c` (2026-09-06), workspace version `0.3.0`. Source of truth for the
> member list is [`Cargo.toml`](../../../Cargo.toml); responsibilities summarized from
> [`docs/specs/`](../../../docs/specs/) and [`docs/INDEX.md`](../../../docs/INDEX.md).

## Cargo workspace members

| Component | Path | Layer | Responsibility (summary) |
|---|---|---|---|
| `okc-core` | `crates/okc-core` | Core | Deterministic compilation engine: snapshot ingest, Markdown parse, dedup, conflict detection, provenance, materialization, pack. `unsafe_code = forbid`. |
| `okc-ai` | `crates/okc-ai` | AI | Provider-neutral AI interface: provider profiles, disclosure, record/replay, critic gate. Isolates LLM variability from the deterministic core. |
| `okc-app` | `crates/okc-app` | App | Shared orchestration/services used by **both** CLI and TUI, so the two surfaces run identical logic (introduced with the cwd-workspace work, ADR-0025). |
| `okc-interop` | `crates/okc-interop` | Interop | Shared types across the language-binding boundary (Rust ↔ Python/Node), ADR-0026. |
| `okc` | `crates/okc` | Surface | Single application binary: CLI **and** TUI (ADR-0019). Installable Claude Code-style (ADR-0020). |
| `bindings/python` | `bindings/python` | Binding | PyO3 `abi3-py311` extension. Dist name `okc-compiler`; import name `okc`. |
| `bindings/node` | `bindings/node` | Binding | napi-rs (`napi9`) native addon. npm package `okc-compiler`. |

## Adjacent (non-crate) assets

| Asset | Path | Purpose |
|---|---|---|
| Demo vaults | `demo/VAULT_A`, `VAULT_B`, `VAULT_C` | Realistic interlinked Obsidian dummy data for exercising the compile flow (created 2026-09-05). |
| Guide site | `guide/` | Spring-quickstart-style usage guide: `cli.md`, `tui.md`, `integration.md`, `ai-provider.md`, `python-node.md`, `conflicts.md`, `troubleshooting.md`, `current-state.md`. |
| Binding build manifest | `bindings/build_artifact_manifest.py` | Cross-language build artifact bookkeeping. |

## Notes

- There is **no MCP adapter crate** in the workspace today. `docs/specs/mcp-adapter.md` and
  ADR-0016 describe MCP-origin *neutrality* of the merge (sources may come from any MCP); the
  spec `mcp-adapter.md` is design surface, not a shipped crate. Confirm against `TRACEABILITY.md`
  (REQ-MCP-001) before treating MCP as implemented.
- CLI and TUI are the **same** binary (`crates/okc`); do not model them as separate deliverables.
- For per-requirement implementation status and evidence, see
  [`docs/TRACEABILITY.md`](../../../docs/TRACEABILITY.md) rather than duplicating status here.
