---
title: Agent Operating Contract
status: normative-v1
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
source_refs:
  - HIST-CURRENT-PLAN
---

# Agent Operating Contract

This repository is documentation-first. The Markdown knowledge base is the implementation contract for the Vault Compiler Framework. A fresh coding-agent session must be able to work from this repository without relying on chat memory.

## Authority and precedence

When documents disagree, use this order:

1. Current files in [`docs/specs/`](docs/specs/)
2. Stable algorithms in [`docs/algorithms/stable/`](docs/algorithms/stable/)
3. Accepted ADRs in [`docs/adr/`](docs/adr/)
4. Experimental and research-only algorithms
5. Historical summaries
6. Historical transcripts

Do not silently choose between conflicting normative documents. Stop implementation, record the conflict in [`docs/history/OPEN_QUESTIONS.md`](docs/history/OPEN_QUESTIONS.md), and report the documentation defect.

## Before coding

1. Read this file, [`PROJECT_CONTEXT.md`](PROJECT_CONTEXT.md), and [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md).
2. Select the closest role through [`docs/INDEX.md`](docs/INDEX.md), then read that role's mandatory reading list.
3. Identify the requirement IDs, algorithm IDs, ADRs, and invariants affected by the task.
4. Confirm whether the algorithm is `normative-v1`, `normative-future`, `experimental`, or `research-only`. Experimental logic must not enter the default compiler path.
5. Inspect the working tree and preserve unrelated user changes.

## While coding

- Treat source Vaults as immutable and hostile inputs.
- Preserve deterministic behavior when AI augmentation is absent.
- AI providers may return proposals only; proposals require schema validation and explicit approval before compilation.
- Preserve provenance from every generated artifact back to source snapshot, document, block, and content hash.
- Keep MCP servers and plugins as adapters. They do not own canonical state or compilation policy.
- Add or update tests named in [`docs/specs/testing-and-quality-gates.md`](docs/specs/testing-and-quality-gates.md).
- Do not weaken a normative invariant without an accepted ADR.

## After coding

1. Update [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) with what is implemented, verified, and still missing.
2. Update [`docs/TRACEABILITY.md`](docs/TRACEABILITY.md) for changed requirement-to-code and requirement-to-test mappings.
3. Add an ADR if an architectural decision changed.
4. Append a dated entry to [`docs/history/DECISION_LOG.md`](docs/history/DECISION_LOG.md); never rewrite historical entries.
5. Update normative specifications in the same change as behavior changes.
6. Run the relevant checks and report exact commands and outcomes.

## Historical integrity

Files under [`docs/history/transcripts/`](docs/history/transcripts/) are immutable source records. Append corrections as clearly marked errata; do not rewrite prior statements. Summaries may be refined, but must retain source links and must not invent redacted or inaccessible content.

## Requirement language

`MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative. Requirement IDs use `REQ-*`; algorithm IDs use `ALG-*`; quality gates use `QG-*`; decisions use `ADR-*`.
