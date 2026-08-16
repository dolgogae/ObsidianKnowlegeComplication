---
title: ALG-DED-001/002 — Exact and Near Deduplication
status: normative-v1
owners:
  - core-rust-engineer
  - algorithms-ai-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-DED-001/002: Exact and Near Deduplication

## Objective and non-goals

Unify genuinely identical notes and cheaply surface similar-note review candidates. Near similarity never proves identity, factual agreement, authorship, or safe merge.

## Inputs and outputs

Inputs are normalized IR Documents, pinned Unicode/hash configuration, and resource bounds. Outputs are exact-duplicate groups with canonical members and provenance, plus ordered scored near-duplicate review candidates and truncation diagnostics.

## Exact duplicate formula — ALG-DED-001

```text
BodyHash(d) = H("vaultc:body:v1\0" || canonical_encode(N_body(d)))
MetaHash(d) = H("vaultc:frontmatter:v1\0" || canonical_encode(N_fm(d)))
ExactKey(d) = (BodyHash(d), MetaHash(d))
```

Documents are exact duplicates iff both tuple members match. Empty frontmatter has a specified canonical representation. Attachments are exact duplicates iff their byte SHA-256 matches.

## Near duplicate formula — ALG-DED-002

Normalize text for candidate comparison, segment it into Unicode grapheme clusters, form contiguous 5-shingles, hash each shingle with a pinned seeded hash family, and compute a MinHash signature:

```text
S_5(d) = set of all contiguous 5-grapheme shingles in comparison text
m_j(d) = min { h_j(s) : s in S_5(d) }, j = 1..k
sim_hat(a,b) = (1/k) sum_{j=1}^k 1[m_j(a) = m_j(b)]
```

LSH bands generate candidates; the estimated similarity is checked against `tau_near = 0.85`. Recommended V1 candidate defaults are `k=128`, `b=32` bands, `r=4` rows (`k=b*r`). Exact configuration is recorded in the plan.

## Symbols

| Symbol | Meaning | Type/range/unit | Default |
|---|---|---|---|
| `d,a,b` | documents | normalized IR documents | required |
| `H` | SHA-256 | bytes → 32 bytes | SHA-256 |
| `N_body` | structural body normalization | ALG-NRM-001 bytes | v1 |
| `N_fm` | typed frontmatter normalization | ALG-NRM-001 bytes | v1 |
| `S_5` | set of 5-grapheme shingles | set of byte strings | 5 graphemes |
| `h_j` | seeded stable shingle hash | 64-bit unsigned | seed table v1 |
| `k` | MinHash permutations/components | positive integer | 128 |
| `b` | LSH band count | positive integer | 32 |
| `r` | rows per band | positive integer | 4 |
| `sim_hat` | estimated Jaccard similarity | `[0,1]` | computed |
| `tau_near` | review candidate threshold | `[0,1]` | 0.85 |
| `1[condition]` | indicator | 0 or 1 | computed |

## Pseudocode

```text
for document in deterministic ID order:
    exact_key = hash(normalized body), hash(normalized frontmatter)
    append document to exact_group[exact_key]
for each exact_group with size > 1:
    choose canonical member by ALG-CNF-001; retain all provenance

for each non-exact representative:
    text = near_comparison_projection(document)
    if fewer than 5 graphemes: use explicit short-document comparison bucket
    signature = minhash(unique 5-shingles, pinned seeds)
    insert each band key into LSH index
for each unique candidate pair from shared bands:
    score = equal_signature_components / k
    if score >= 0.85: emit review candidate
sort candidates by (-score, min(DocumentId), max(DocumentId))
```

## Complexity

Exact grouping is `O(B)` hashing plus ordered grouping. MinHash is `O(U*k)` naively for `U` unique shingles; one-permutation/optimized implementations require equivalence tests. LSH candidate comparison is data-dependent and can degrade to `O(D²)` for repetitive corpora, so bucket and pair limits are mandatory.

## Edge and security cases

Do not collapse same body/different frontmatter. Handle documents shorter than five graphemes, empty/generated boilerplate, repeated characters, extremely large notes, Unicode segmentation versions, hash collisions, and adversarial common-template buckets. Apply maximum candidates per document and mark truncation explicitly. Secret filtering occurs before any remote embedding/AI, although this algorithm is local.

## Worked examples

Exact: two notes normalize to body `# Topic\nText` and frontmatter `{tags:[rust]}`; they share an ExactKey and become one output with two provenance sources. If one has `{tags:[rust,api]}`, MetaHash differs, creating a metadata conflict instead.

Near: with a pedagogical signature length `k=8`, signatures `[1,8,3,4,9,2,7,6]` and `[1,8,3,4,0,2,7,5]` match in 6 positions, so `sim_hat=6/8=0.75`; they are below `0.85` and are not candidates. Production uses `k=128`.

## Golden vectors

| Case | Expected |
|---|---|
| identical body + identical normalized frontmatter | exact group |
| identical body + distinct frontmatter value | not exact; frontmatter conflict |
| CRLF vs LF with otherwise equivalent parsed structure | exact body hash |
| only path/title differs but body and metadata equal | exact group; canonical path tie-break |
| estimated score `108/128 = 0.84375` | no near candidate |
| estimated score `109/128 = 0.8515625` | near review candidate |

Complete shingle/hash signatures MUST be frozen in repository fixtures with the pinned Unicode and seed-table versions.

## Metrics and rollback

Exact grouping requires zero known false positives and deterministic equality. Near candidates are measured by recall/precision against human-labeled same-topic pairs, review burden, bucket explosion, and language/topic slices. If resource bounds are exceeded, disable/truncate near candidate generation with a diagnostic; exact deduplication remains available. Never lower the threshold silently.
