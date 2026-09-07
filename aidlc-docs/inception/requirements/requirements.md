# Requirements (Inception, session-grounded)

> **Reconciliation, not reinvention.** okc-core already has a normative requirements matrix in
> [`docs/TRACEABILITY.md`](../../../docs/TRACEABILITY.md) with stable REQ-* IDs. This document does
> **not** mint competing IDs. It restates requirements **grounded in the user's own words from
> Codex sessions**, maps each to the existing REQ ID(s) and ADR(s), and flags mismatches for the
> user. Implementation status lives in `TRACEABILITY.md` / `docs/CURRENT_STATE.md`, not here.
> Session citation keys (S1–S15) are defined in
> [`../reverse-engineering/decision-intent-timeline.md`](../reverse-engineering/decision-intent-timeline.md).

## Intent Analysis

- **User request (this run, 2026-09-07)**: *"지금 만들어진 프로젝트에서 codex 세션 이력들을 참고해서
  ai-dlc 문서들 작성해줄 수 있어?"* — produce AI-DLC Inception documents for okc-core, using the
  Codex session history to reconstruct the "why".
- **Request type**: Reverse engineering + documentation (Enhancement of process artifacts). Not a
  code change.
- **Scope estimate**: System-wide (documents the whole product), but confined to `aidlc-docs/`.
- **Complexity estimate**: Moderate — low technical risk, but requires faithful attribution of
  intent to existing decisions without duplicating or contradicting authoritative docs.
- **Depth chosen**: Standard (functional + non-functional), executed in **batch mode** (no
  per-stage gates) per the user's explicit choice.
- **Underlying product being specified**: OKC — a Rust, documentation-first, AI-required compiler
  that merges immutable Obsidian Vault snapshots from arbitrary community MCPs into one
  deterministic, auditable Vault.

## Functional Requirements (grounded in session intent)

Each FR cites the originating session(s), the existing REQ ID(s) it corresponds to, and the ADR(s)
that decided it. **All REQ/ADR mappings below were cross-checked against
[`docs/TRACEABILITY.md`](../../../docs/TRACEABILITY.md) and `docs/adr/` by an adversarial
verification pass (2026-09-07); corrections were applied** (see the audit log entry
"Verification Pass Applied"). Notably, an initial `REQ-AI-*` rotation was fixed:
REQ-AI-001 = provider neutrality, REQ-AI-002 = hostile-proposals/independent critic,
REQ-AI-003 = recording/replay, REQ-AI-004 = mandatory integration.

| # | Requirement (from user intent) | Sessions | Existing REQ | ADR |
|---|---|---|---|---|
| FR-1 | Ingest **multiple** Obsidian Vault snapshots as inputs and merge them into one output Vault. | S1, S3 | REQ-SRC-001/002, REQ-SNP-001/002 | 0003, 0016 |
| FR-2 | Merge must be **MCP-origin-neutral**: sources produced by different community Obsidian MCPs must still integrate correctly. | S3 | REQ-SRC-001 (origin neutrality) | 0016 |
| FR-3 | Treat sources as **immutable**; never mutate inputs; emit a **new** compiled Vault that excludes raw sources. | S3, S4 | REQ-SNP-*, REQ-PRV-001 | 0003, 0006 |
| FR-4 | Detect duplicates/near-duplicates and **conflicts**; record every disposition with **typed provenance** and an audit envelope. | S3 | REQ-DED-001/002, REQ-PRV-001, REQ-CNF-001 | 0009, 0010, 0017 |
| FR-5 | **AI is required** for semantic classification/merge (not optional); the LLM operates on the defined logic. | S5, S6 | REQ-AI-004 (mandatory integration) | 0022 |
| FR-6 | **Provider-neutral** AI interface — any LLM can be attached through a common interface; provider profiles + disclosure + record/replay. | S1, S5 | REQ-AI-001 (provider neutrality), REQ-AI-003 (recording/replay) | 0004, 0023 |
| FR-7 | A **critic gate + human approval** must pass before a new Vault is produced (evidence-complete integration). | S6 | REQ-AI-002 (hostile proposals/critic), REQ-INT-004 (critic), REQ-INT-005 (immutable approvals) | 0024 |
| FR-8 | Single application named **`okc`** providing both **CLI and TUI** from one binary. | S3 | REQ-APP-001 | 0019 |
| FR-9 | **TUI resolves conflicts in-app** and runs the full flow: AI connect → Vault select → sensitive-info check/consent → taxonomy review → cluster review → compile → verify. | S3, S6, S7 | REQ-APP-001/002 | 0025 |
| FR-10 | **cwd-as-workspace**: running `okc` in a folder (Claude Code-style) uses that folder as the integration workspace; CLI and TUI share the same `okc-app` service code. | S6, S7 | REQ-APP-002 | 0025 |
| FR-11 | **Secret/token entry** like Claude Code (prompted token input) with a defined keychain/secret boundary; support env var too. | S6 | REQ-SEC-003 (credentials) | 0025 |
| FR-12 | **Claude Code-style install/update** — one-line install, usable immediately as `okc` without manual PATH setup. | S3, S11 | REQ-REL-001 | 0020, 0021 |
| FR-13 | Expose OKC as a **library** importable from other languages; ship **Python** (dist `okc-compiler`, import `okc`, CPython 3.11 abi3) and **Node.js** (npm `okc-compiler`) bindings with feature parity as a release condition. | S9, S10 | REQ-SDK-001/002 | 0026 |
| FR-14 | Keep **only the current schema (Schema 3)** in `main`; preserve V1/V2 via git history + remote **archive tags** (no versioned cruft in source). | S12, S13 | REQ-CMP-003 (current-only schema) | 0027 |
| FR-15 | Deterministic, verifiable **compiled output + pack** (stable golden SHA); `verify` step. | S4, S6 | REQ-CMP-*, REQ-MAT-001 | 0013, 0014, 0027 |
| FR-16 | Ship a **spring-quickstart-style guide** and keep README/docs/guide (incl. SDK) current. | S5, S10 | — (no docs REQ; DECISION_LOG maps operator guide to REQ-SDK-001/REQ-APP-001) | — |
| FR-17 | Provide **realistic demo vaults** (`VAULT_A/B/C`, interlinked) to exercise the flow. | S8 | — (dev tooling; no REQ — verified) | — |

## Non-Functional Requirements

| # | NFR (from intent + repo posture) | Sessions / source | Existing REQ | ADR |
|---|---|---|---|---|
| NFR-1 | **Determinism**: identical inputs → identical output (golden SHA in `CURRENT_STATE.md`). | S4, S6; specs | REQ-CMP-* | 0004 |
| NFR-2 | **Auditability/traceability**: every output block traceable to origin + decision. | S3 | REQ-PRV-001 | 0010 |
| NFR-3 | **Security/trust boundaries**: sources treated as hostile/immutable; narrow network surface (AI + updater only); secrets zeroized. | repo posture; S3, S6 | REQ-SEC-001/002/003 | 0025 |
| NFR-4 | **Portability**: cross-platform incl. Windows (SDK smoke tests must pass on win_amd64). | S15 | REQ-SDK-* | 0007 |
| NFR-5 | **Performance / bounded semantic execution** (batching, cancellation/recovery) — captured as proposed design. | S14 | REQ-PERF-001 (scale) | 0028, 0031 (proposed) |
| NFR-6 | **Maintainability**: documentation-first; `unsafe_code = forbid`; clippy pedantic; decisions recorded as ADRs. | S1, S14; `Cargo.toml` | — | 0001, 0002 |
| NFR-7 | **Open source**, cross-platform, `MIT OR Apache-2.0`. | S1 | — | 0007 |

## Confirmations Needed (in lieu of the blocking verification-questions gate)

Batch mode was chosen, so the mandatory `requirement-verification-questions.md` gate is not run.
The REQ/ADR attribution questions have since been **resolved by the adversarial verification pass**
(corrections applied above). The items below are the **product/scope** decisions that remain
genuinely open for you — they are choices, not attribution errors:

1. **Verified corrections — please confirm acceptable** — the pass corrected these mappings against
   `TRACEABILITY.md`: FR-5 (→ REQ-AI-004), FR-6 (→ REQ-AI-001/003), FR-7 (→ REQ-AI-002/REQ-INT-004/005),
   FR-2 (→ REQ-SRC-001; dropped REQ-MCP-001), FR-11 (→ REQ-SEC-003 only), FR-14 (→ REQ-CMP-003 only),
   FR-16 (→ no docs REQ), NFR-5 (dropped REQ-MEM-001, which is *experimental algorithms*, not RSS).
   Confirm these match your understanding.
2. **MCP adapter status** — the pass clarified that merge *neutrality* is **REQ-SRC-001**, while
   **REQ-MCP-001 is a separate, future-only MCP *adapter*** (`specs/mcp-adapter.md`, no shipped
   crate). FR-2 now cites REQ-SRC-001. Open product question: is a real MCP adapter still on the
   roadmap, or is origin-neutral ingest sufficient?
3. **Scope of this AI-DLC run** — Confirm the batch scope is complete as delivered (state + audit +
   reverse-engineering + requirements). Should I also draft User Stories and Workflow Planning, or
   stop Inception here?
4. **Neuroscience "merge math" (S1)** — the genesis session referenced brain-merging / neuroscience
   math as inspiration. The pass confirmed this maps to **REQ-MEM-001 = experimental algorithms**,
   explicitly *excluded from the default path* (deferred to future experimental work per
   DECISION_LOG 2026-08-15). Confirm it stays framing/experimental, not a live FR.
5. **Demo vaults (FR-17)** — attribution verified as "no REQ" (dev tooling). Open product question:
   keep as informal tooling, or promote to a formal requirement?

## Key Requirements Summary

- OKC's *reason to exist* (FR-1/FR-2): merge immutable, multi-origin Obsidian Vaults into one
  deterministic, auditable Vault — MCP-origin-neutral.
- The defining evolution (FR-5/FR-7): **AI moved from optional to required**, gated by a critic +
  human approval, while the core stays deterministic and the AI stays provider-neutral (FR-6).
- The UX north star (FR-8/FR-10/FR-12): **Claude Code-like** — one binary `okc`, one-line install,
  cwd-as-workspace, prompted token entry.
- The maturity arc (FR-13/FR-14): **library-ized** to Python/Node, then **single-sourced** to
  Schema 3 with history/tags preserving older formats.
- Everything above was **documentation-first** from the first session (NFR-6) — the premise that
  makes this reconstruction possible at all.
