# Reverse Engineering — Technology Stack

> Extracted from [`Cargo.toml`](../../../Cargo.toml) and [`rust-toolchain.toml`](../../../rust-toolchain.toml)
> at commit `7f87f7c` (2026-09-06). Versions are pinned in the workspace manifest; consult it for
> exact constraints (several are `=`-pinned).

## Languages & toolchain

| Item | Value |
|---|---|
| Primary language | Rust, edition **2024**, `rust-version` 1.97.1 |
| Pinned toolchain | channel `1.97.1`, profile `minimal`, components `clippy` + `rustfmt` |
| Binding targets | Python (CPython 3.11 `abi3`), Node.js (napi9) |
| Workspace version | `0.3.0` (Schema 3) |
| License | `MIT OR Apache-2.0` |
| Lints | `unsafe_code = "forbid"`; clippy `all` + `pedantic` at warn |
| Release profile | `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"` |

## Key dependencies (by role)

| Role | Crates |
|---|---|
| CLI / TUI | `clap` 4 (derive), `ratatui` =0.30.2, `crossterm` =0.29.0 |
| Markdown / text | `comrak` 0.48, `unicode-normalization` =0.1.25, `unicode-segmentation`, `caseless` =0.2.2 |
| Serialization | `serde`, `serde_json`, `serde_yaml_ng` 0.10, `toml` 0.9 |
| Storage / workspace | `rusqlite` 0.37 (bundled), `atomicwrites` =0.4.4, `tempfile`, `rustix` =1.1.4 |
| Hashing / encoding | `sha2`, `hex`, `data-encoding` |
| Archive / compression | `zip` 4 (deflate+zstd), `zstd` 0.13, `tar` 0.4 |
| Parallelism | `rayon` 1 |
| HTTP (AI providers) | `ureq` =3.4.0 (rustls, platform-verifier, json) |
| Secret handling | `zeroize` 1 (derive) |
| Install / update | `axoupdater` =0.10.0 (blocking) — supports Claude Code-style install/update (ADR-0020/0021) |
| Errors / logging | `anyhow`, `thiserror` 2, `tracing`, `tracing-subscriber` 0.3 |
| File walking | `ignore` 0.4 |
| Python binding | `pyo3` =0.29.2 (`abi3-py311`, `extension-module`), `pyo3-build-config` |
| Node binding | `napi` =3.12.2 (`napi9`), `napi-derive` =3.6.3, `napi-build` =2.4.1 |

## Observations

- **Offline-capable determinism**: bundled SQLite, atomic writes, and pinned versions support the
  deterministic-output guarantee (golden SHA in `docs/CURRENT_STATE.md`).
- **Network surface is narrow**: only `ureq` (for AI provider calls) and `axoupdater` reach the
  network; the compilation core does not.
- **Security posture**: `unsafe_code = forbid` workspace-wide; `zeroize` for secret material,
  consistent with the keychain/secret boundary in ADR-0025 and `docs/specs/security-and-trust-boundaries.md`.
