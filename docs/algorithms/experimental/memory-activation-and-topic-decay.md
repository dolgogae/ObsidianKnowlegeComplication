---
title: ALG-MEM-001 — Memory Activation and Topic Decay
status: experimental
owners:
  - algorithms-ai-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-MEM-001: Memory Activation and Topic Decay

## Objective and non-goals

Estimate how accessible a knowledge item should be from its historical uses, with topic-dependent decay. This ACT-R-inspired engineering heuristic is not a biological measurement, a deletion policy, or a truth/confidence score. It is isolated from V1 compilation.

## Inputs and outputs

Inputs are a stable item ID, evaluation time, deduplicated use-event times, topic assignment, decay policy, and time unit. Output is a dimensionless activation score or an explicit missing-activation state with policy/model version.

## Formula

```text
B_i(t) = ln(sum_{r=1..n_i} (t - t_ir + epsilon)^(-d_i))
d_i = d_topic(i)
```

Higher `B_i` means more recent/frequent access. Topic decay affects retrieval weight only; facts are not deleted.

## Symbols

| Symbol | Meaning | Type/range/unit | Experimental default |
|---|---|---|---|
| `i` | knowledge item | stable item ID | required |
| `t` | evaluation time | real, days since fixed epoch | required |
| `r` | use-event index | integer `1..n_i` | observed |
| `n_i` | number of valid use events | integer `≥0` | observed |
| `t_ir` | time of event `r` for item `i` | real days, `t_ir≤t` | observed |
| `epsilon` | minimum elapsed-time stabilizer | positive days | `0.01` day |
| `d_i` | item decay exponent | real `[0,2]` | from topic policy |
| `d_topic(i)` | decay assigned to item topic | real `[0,2]` | fast API `0.8`, general `0.5`, stable math `0.2`, personal history `0.0` |
| `ln` | natural logarithm | positive input → real | defined |
| `B_i(t)` | activation | dimensionless log score | computed |

The default topic values are hypotheses, not production constants. All times use one declared unit and fixed epoch.

## Pseudocode

```text
events = valid_use_events(item_id, at_or_before=t)
if events empty: return configured_floor or MissingActivation
d = topic_decay(resolve_topic(item_id))
terms = [(t - event.time + epsilon)^(-d) for event in events]
return logsumexp([-d * ln(t - event.time + epsilon) for event in events])
```

Use log-sum-exp for numerical stability. Deduplicate retries and synthetic events by event ID; do not count a retrieval as successful use without a declared feedback definition.

## Complexity

Time `O(n_i)` per item, memory `O(1)` streaming. Pre-aggregated approximations require separate error bounds because power-law decay is not exactly exponential.

## Edge and security cases

Future timestamps, mixed units, event spam, bot loops, missing topics, `d=0`, zero events, privacy-sensitive activity histories, and identity changes must be explicit. For `d=0`, each event contributes `1`, so `B_i=ln(n_i)` and never decays. Cap event influence/rate by authenticated actor/session to reduce gaming.

## Worked example

At `t=10` days, item `i` was used at days `9` and `6`. With `epsilon=0.01` and `d_i=0.5`:

```text
B_i = ln((1.01)^(-0.5) + (4.01)^(-0.5))
    ≈ ln(0.9950 + 0.4994)
    ≈ 0.4017
```

With `d_i=0`, the same two events give `ln(2)≈0.6931` regardless of age.

## Golden vectors

| Ages `(t-t_ir)` days | `epsilon` | `d` | Expected `B` |
|---|---:|---:|---:|
| `(1,4)` | 0.01 | 0.5 | approximately `0.4017` |
| `(1,4)` | 0.01 | 0 | `ln(2)≈0.693147` |
| `(0)` | 1 | 1 | `0` |
| no events | any | any | explicit missing/floor, never `ln(0)` |

## Calibration and rollback

Compare time-aware Recall@k, nDCG, revisit prediction, stale-answer rate, and topic slices against recency-only, frequency-only, and no-activation baselines. Tune only on past events and test on future windows. If it worsens retrieval or is gameable, set its retrieval coefficient to zero and retain events for offline analysis only.
