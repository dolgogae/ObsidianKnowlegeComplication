---
title: ADR-0008 — Markdown and Canvas Are First-Class in V1
status: normative
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0008
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0008: Markdown and Canvas Are First-Class in V1

## Status

Accepted on 2026-08-15.

## Context

Obsidian knowledge is not plain Markdown alone: frontmatter, wikilinks, embeds, aliases, block references, attachments, and Canvas carry structure. Bases are newer and their semantics are less stable.

## Decision

V1 parses Markdown/frontmatter/Obsidian links and JSON Canvas as first-class data with source-span preservation. It inventories/deduplicates attachments by hash. `.base` is copied opaquely with a warning; `.obsidian/**` is excluded.

## Consequences

Comrak must be supplemented with an Obsidian-aware span scanner and a typed/unknown-preserving Canvas model. Bases remain usable but their internal references cannot be guaranteed in V1.
