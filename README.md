# Vault Compiler Framework

`vaultc` compiles immutable Obsidian Vault snapshots into a new deterministic,
auditable Compiled Vault. The Rust SDK owns inspection, planning, approval,
compilation, verification, and provenance. AI providers, MCP servers, and
Obsidian plugins are optional adapters.

The implementation contract starts at [`AGENTS.md`](AGENTS.md) and
[`docs/INDEX.md`](docs/INDEX.md).

## Workspace

- `vaultc`: compiler library.
- `vaultc-protocol`: provider-neutral serializable protocol.
- `vaultc-cli`: the `vaultc` command-line application.

This repository currently targets Rust 1.97.1 and uses the Rust 2024 Edition.
