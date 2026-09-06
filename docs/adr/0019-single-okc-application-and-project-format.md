---
title: ADR-0019 — Single OKC Application and Project Format
status: normative
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0001
  - ADR-0005
  - ADR-0019
  - ADR-0021
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0019: Single OKC Application and Project Format

## Status

Accepted on 2026-09-02.

## Context

Long-running inspection, review, and compilation require resumable application
state, while separate CLI and TUI implementations would drift in policy and
security behavior.

## Decision

`okc` is the only executable. It contains Clap CLI commands and a Ratatui
0.30.2/Crossterm TUI. Both call services in `okc-app`; neither invokes the
other as a subprocess. With no subcommand a TTY starts the TUI, while a
non-TTY receives help and exit code 2. Tokio is not used for the TUI,
application services, or worker architecture. ADR-0021 defines the sole
library-owned runtime exception for the pinned updater.

A project is a `Name.okc-project/` directory containing `manifest.json`,
`state.sqlite3`, `objects/`, `workspace/build.sqlite3`, and a transient
single-writer `project.lock`. Immutable objects use content-addressed,
no-clobber writes and synchronization before SQLite references commit. State
uses WAL, foreign keys, `FULL` synchronous mode, and explicit schema 2
migration. Source rebinding or snapshot change invalidates downstream plans,
decisions, and approvals.

Projects are not encrypted. Source paths, plans, decisions, and AI records are
plaintext; private local permissions and shared/network-location warnings are
required. Curator identity is fixed as project `curator_id + policy_version`.

The TUI uses a pure `Model + Event -> Model + Effects` reducer and worker
threads for core/provider work. It restores terminal state on normal exit,
panic, signal, cancellation, and provider failure; escapes hostile terminal
controls and emits no OSC title, OSC8, or OSC52 sequence.

## Consequences

- CLI and TUI share one compilation policy boundary.
- Project moves require explicit source rebinding.
- Publication-barrier cancellation and platform process-tree cleanup remain
  application-service responsibilities and release gates.
