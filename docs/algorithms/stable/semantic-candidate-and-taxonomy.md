---
title: ALG-SEM-001 — Semantic Candidates and Taxonomy
status: normative
owners:
  - algorithms-ai-engineer
  - core-rust-engineer
last_updated: 2026-09-03
decision_refs:
  - ADR-0023
  - ADR-0024
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-SEM-001: Semantic Candidates and Taxonomy

## Objective and non-goals

Create a recorded, reproducible candidate graph and obtain a complete proposed
taxonomy for all Markdown documents. Similarity is retrieval evidence, not
merge authority or a probability of semantic equivalence.

## Inputs and outputs

Inputs are the sealed current corpus, deterministic Markdown chunks, one embedding
profile/model/options identity, exact-duplicate groups, retained MinHash candidates,
and title/alias/link candidates. Outputs are ordered embeddings, an ordered
candidate list, and a taxonomy proposal assigning every `DocumentId` to
exactly one cluster with one safe canonical path.

## Algorithm

1. Sort documents by raw `DocumentId`; split normalized block text at block
   boundaries under the configured byte/token ceiling. Oversize blocks split
   at deterministic UTF-8 scalar boundaries. Empty documents receive one
   explicit empty chunk.
2. Submit batches in this exact order to one embedding model identity. Require
   one vector per input in request order, one non-zero dimension for the run,
   and only finite values. Normalize vectors to unit length; a zero norm fails.
3. Build/query HNSW with fixed seed, insertion in `DocumentId/chunk-index`
   order, fixed construction/search parameters, and stable tie-breaking by
   candidate ID. Record the final bounded top-k neighbors; the index itself is
   disposable.
4. Union semantic pairs with exact/MinHash/title/alias/link pairs. Sort each
   unordered pair by ID, exact-deduplicate, attach all candidate reasons, and
   sort globally by IDs.
5. Give the organizer the sealed document summaries and recorded candidates.
   Locally validate that each known document occurs once, no unknown document
   occurs, cluster IDs are unique, canonical paths are safe/unique Markdown
   paths, and taxonomy labels are bounded data.
6. Seal the complete taxonomy hash. Human edits produce a new taxonomy revision
   and require a fresh whole-taxonomy approval.

## Identity

```text
TaskKey = H("okc:ai-task:v3\0" || canonical_json({
  stage, prompt_hash, schema_hash, source_hash,
  provider, model, adapter, options
}))

TaxonomyHash = H("okc:taxonomy:v3\0" || canonical_json(taxonomy))
```

No wall clock, request scheduling, host path, or provider key is an identity
input.

## Complexity

For `C` chunks, dimension `d`, bounded HNSW degree `M`, and `P` final pairs:
embedding validation is `O(Cd)`, index construction is expected
`O(C log C * M)`, and canonical pair reduction is `O(P log P)`. Implementations
must spill vectors/candidates to the current workspace at policy thresholds.

## Edge and security cases

Reject reordered/missing/extra vectors, dimension drift, NaN/infinity, zero
norms, duplicate or unknown document assignments, unsafe or colliding paths,
provider truncation, and prompt text that attempts to invoke tools or alter
policy. Sensitive-preflight routing applies before any chunk is disclosed.

## Golden vectors

- Equal vectors tie by raw candidate ID.
- Reversing source registration leaves chunks, pairs, and taxonomy validation
  order unchanged.
- One missing document assignment and one duplicate assignment both fail.
- Any non-finite coordinate or mixed vector dimension fails before indexing.

## Correctness and rollback

Primary metrics are 100% one-cluster coverage and byte-identical candidate
recording under replay. A failed organizer task leaves no taxonomy result.
Re-running from the last complete task is required; heuristic fallback is not.
