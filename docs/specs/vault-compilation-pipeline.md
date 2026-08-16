---
title: Vault Compilation Pipeline
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0005
  - ADR-0008
source_refs:
  - HIST-COMPILER-PLAN
---

# Vault Compilation Pipeline

## Ordered stages

1. **Open safely:** resolve input kind, establish resource limits, reject unsafe archives and paths.
2. **Enumerate:** traverse with deterministic logical-path ordering and exclusions.
3. **Snapshot:** stream hashes, build manifest, and seal snapshot identity.
4. **Parse:** decode Markdown/frontmatter/Canvas and inventory opaque assets and Bases.
5. **Normalize:** compute comparison forms without replacing source bytes.
6. **Resolve:** build source-local and cross-source link candidates.
7. **Analyze:** group exact duplicates, generate near-duplicate candidates, detect conflicts.
8. **Plan:** allocate output paths, rewrites, copies, generated metadata, diagnostics, and required decisions.
9. **Augment optionally:** ask providers for evidence-bound proposals and capture a transcript.
10. **Validate and approve:** reject invalid/stale proposals; record explicit decisions.
11. **Compile:** reverify source hashes, stage output, materialize approved operations, write audit metadata and checksums.
12. **Publish:** atomically rename the complete sibling staging directory.
13. **Verify:** independently validate paths, hashes, manifests, provenance closure, and resolvable rewrites.

Stages are resumable only when their input identity and semantic configuration hash match.

## Source policies

V1 excludes `.obsidian/**`, `.git/**`, known executable file classes, named secret files, and any external symlink traversal. Symlinks are not followed by default. A logical path containing an absolute root, drive prefix, NUL, `..` traversal, or normalization collision is rejected.

Archive extraction is virtual/streamed when possible. Limits MUST cover compressed bytes, expanded bytes, file count, per-file size, path length, nesting, and compression ratio. The planner does not need to execute or import uploaded plugin JavaScript.

## Analysis policy

- Exact duplicates follow ALG-DED-001.
- Near duplicates follow ALG-DED-002 and only create review candidates.
- Link, path, title, alias, and frontmatter collisions become typed conflicts.
- Attachments are content-addressed by SHA-256 and deduplicated independently of note identity.
- `.base` files are copied to `views/` and receive an `OPAQUE_BASE_UNVALIDATED` diagnostic.

## Plan structure

A plan contains:

- plan/schema/compiler versions;
- complete ordered snapshot IDs and configuration hash;
- output operations with stable operation IDs;
- input-to-output path map and link rewrite map;
- exact duplicate groups and canonical member selection;
- near-duplicate candidates;
- conflicts, required decisions, and their states;
- provider proposals, validation states, and approval references;
- expected output hashes where computable;
- diagnostics and resource estimates.

Plans are immutable values. Any change yields a new `plan_id`; approvals bind to a specific plan/proposal content hash.

## Failure semantics

No partial output may become the requested destination. On failure, the staging directory may be retained only under an explicit debug option and MUST be clearly marked incomplete. The default behavior removes the exact known staging directory after safe validation; it never recursively deletes an unresolved path.

Warnings may permit compilation if policy allows. Errors prevent approval or publication. Resource-limit violations are errors, not best-effort truncation.

## Determinism

All traversal, candidate, diagnostic, manifest, JSON line, and archive member orderings are explicit. Locale, wall clock, random process seed, hostname, absolute input path, filesystem inode, and thread scheduling MUST NOT affect semantic results or bytes. Parallel processing may be used only with deterministic collection and reduction.
