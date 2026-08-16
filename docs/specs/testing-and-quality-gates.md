---
title: Testing and Quality Gates
status: normative-v1
owners:
  - qa-security-engineer
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
  - ADR-0007
  - ADR-0008
source_refs:
  - HIST-COMPILER-PLAN
---

# Testing and Quality Gates

## Release gates

- **QG-001 Functional:** all stable requirement tests pass on supported platforms.
- **QG-002 Determinism:** identical fixtures/configuration/transcript produce identical manifest, output hashes, and VaultPack bytes across two clean runs; cross-platform semantic hashes match.
- **QG-003 Provenance:** every output node has a complete derivation path; generated items have evidence and approval.
- **QG-004 Safety:** hostile-input corpus, traversal/symlink/archive and malicious-proposal tests pass fail-closed.
- **QG-005 Compatibility:** schema backward/forward behavior matches the version matrix.
- **QG-006 Performance:** reference workload meets ≤20 minutes and ≤2 GB peak RSS without AI on 8 cores, 16 GB RAM, NVMe.
- **QG-007 Documentation:** Markdown links resolve; traceability reflects code/tests; release changes and ADR impact are recorded.
- **QG-008 Supply chain:** licenses, lockfiles, vulnerability policy, provenance/SBOM, and reproducible release process pass.

## Test layers

### Unit and golden tests

- domain-separated identifiers and canonical serialization;
- Markdown/frontmatter source spans and rewrite output;
- wikilinks, embeds, aliases, heading/block refs, callouts, math, code fences;
- JSON Canvas typed fields and unknown-field preservation;
- exact and near-duplicate golden vectors;
- path/case/title/frontmatter conflict resolution;
- checksums, provenance records, approval invalidation, and pack archive headers.

### Property and fuzz tests

- parser/scanner never panics for arbitrary bytes;
- normalization is idempotent;
- path allocator returns safe unique paths or an explicit error;
- compilation never mutates source fixture hashes;
- provenance graph is closed and acyclic in derivation edges;
- serialization/parse round trips preserve semantic identity;
- concurrent and sequential planning yield the same result.

### Adversarial corpus

Include ZIP slip, tar traversal, symlink/hardlink escape, decompression bomb, huge frontmatter, deeply nested Markdown/JSON, duplicate archive members, malformed UTF-8/YAML/JSON, NFC/NFD/case collisions, Windows reserved names, control characters, malicious HTML, fake tool instructions, forged/stale proposal IDs, invalid evidence spans, extension spoofing, and interrupted writes.

### Integration and end-to-end

- Rust SDK and CLI create equivalent plans.
- Provider subprocess capability negotiation, timeout, crash, oversized/malformed response, cancellation, and transcript replay.
- Compile interruption never publishes partial output.
- Independent verifier catches each intentionally corrupted artifact class.
- Future MCP and Obsidian adapters pass contract and permission tests without bypassing framework invariants.

## Algorithm status gates

Stable algorithms require worked examples, golden vectors, boundary tests, and cross-platform determinism. Experimental algorithms require offline baselines, held-out evaluation, calibration/error bars, ablation, resource cost, safety/privacy review, and rollback behavior. Passing an experiment does not promote it; an ADR and normative spec update are also required.

## Benchmark hygiene

LLM-generated queries cannot be both the sole test generator and judge. Retrieval benchmarks use source notes as traceable ground truth, split generation/evaluation sources when possible, avoid train/test leakage, report topic/cohort sizes and confidence intervals, and retain failure cases. There is no single absolute quality score across unrelated topics.
