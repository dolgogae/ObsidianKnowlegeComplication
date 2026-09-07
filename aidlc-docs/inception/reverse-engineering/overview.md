# Reverse Engineering — Overview

**Project**: okc-core (OKC — Obsidian Knowledge Compilation) · v0.3.0 · Schema 3
**Approach**: link + summarize existing normative docs; do **not** re-derive them.

> This repository is **documentation-first** by an explicit founding decision (2026-08-15): every
> plan, algorithm, and rationale is written to Markdown so that fresh coding sessions rely only on
> those docs. As a result, the authoritative reverse-engineering material already exists in-repo.
> These AI-DLC artifacts act as an **index and reconciliation layer** over that material, plus one
> genuinely new artifact — a citation-backed reconstruction of the user's intent from Codex
> sessions (`decision-intent-timeline.md`).

## What OKC is (business / system overview)

OKC compiles several **immutable** Obsidian Vault snapshots — each produced by a *different*
person using a *different* community Obsidian MCP — into **one new, deterministic, auditable
Vault**. Sources are treated as read-only and potentially hostile; nothing is mutated in place;
the compiled output is a fresh artifact whose every block is traceable to its origin and to the
decision that placed it. As of Schema 3 (v0.3.0), **AI is a required participant**: an LLM
performs the semantic classification/merge under deterministic guardrails, with a critic gate and
human approval before a new Vault is emitted.

```text
person A ── any MCP ──▶ Vault A ┐
person B ── any MCP ──▶ Vault B ├─▶  OKC (deterministic core + required AI)  ─▶  Compiled Vault + pack
person C ── any MCP ──▶ Vault C ┘        provenance · conflicts · audit envelope · critic gate
```

The problem OKC solves is **MCP-origin-neutral merge**: the sources may each come from a
different Obsidian MCP, and OKC must still integrate them correctly. See the intent timeline for
where this framing came from (user turns, 2026-09-01).

## Authoritative sources (read these first)

| Topic | Normative document | Kind |
|---|---|---|
| Product identity, durable principles, non-goals | [`PROJECT_CONTEXT.md`](../../../PROJECT_CONTEXT.md) | Charter |
| Current state, quality gates, release blockers, golden SHA | [`docs/CURRENT_STATE.md`](../../../docs/CURRENT_STATE.md) | Snapshot |
| Contemporaneous decision record (dated, source-linked) | [`docs/history/DECISION_LOG.md`](../../../docs/history/DECISION_LOG.md) | Log |
| Architecture decisions (0001–0031) | [`docs/adr/`](../../../docs/adr/) | ADRs |
| Formal specifications | [`docs/specs/`](../../../docs/specs/) | Specs |
| REQ → implementation → evidence matrix | [`docs/TRACEABILITY.md`](../../../docs/TRACEABILITY.md) | Traceability |
| Doc map / entry point | [`docs/INDEX.md`](../../../docs/INDEX.md) | Index |
| Open questions / known gaps | [`docs/history/OPEN_QUESTIONS.md`](../../../docs/history/OPEN_QUESTIONS.md) | Backlog |
| Agent operating rules (docs-first mandate) | [`AGENTS.md`](../../../AGENTS.md) | Rules |
| Algorithms (merge math, dedup, etc.) | [`docs/algorithms/`](../../../docs/algorithms/) | Specs |
| Glossary / roles | [`docs/GLOSSARY.md`](../../../docs/GLOSSARY.md) · [`docs/roles/`](../../../docs/roles/) | Reference |

## RE artifact set (this directory)

| File | Purpose |
|---|---|
| `overview.md` (this file) | System/business overview + pointer map to normative sources |
| [`architecture.md`](architecture.md) | Layering & data flow summary → links `docs/specs/framework-architecture.md` |
| [`component-inventory.md`](component-inventory.md) | Crate/binding inventory table → links source + specs |
| [`technology-stack.md`](technology-stack.md) | Languages, toolchain, key dependencies (from `Cargo.toml`) |
| [`decision-intent-timeline.md`](decision-intent-timeline.md) | **New**: user-intent reconstruction from Codex sessions, cited, mapped to ADRs/REQs |

## Scope & caveats

- **Not stale-checked line-by-line.** Facts here were summarized from the normative docs and
  `Cargo.toml` at commit `7f87f7c` (2026-09-06). Where an artifact and a normative doc disagree,
  the normative doc wins — record the conflict, do not silently resolve it.
- **No code was analyzed for correctness here.** This is an inventory/synthesis pass, not an audit.
- The intent timeline reflects what the **user asked for** in sessions; whether each item is
  *implemented* is tracked in `docs/TRACEABILITY.md` / `docs/CURRENT_STATE.md`, not here.
