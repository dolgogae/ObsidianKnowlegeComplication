---
title: ALG-MEM-007 — Hopfield Associative Retrieval
status: research-only
owners:
  - algorithms-ai-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-MEM-007: Hopfield Associative Retrieval

## Objective and non-goals

Explore whether energy-based associative memory can recover stored knowledge patterns from partial/noisy cues. This is research-only: it is not a V1 storage model, graph truth mechanism, conflict resolver, or claim about biological memory.

## Inputs and outputs

Inputs are a frozen pattern encoder, partial/noisy cue, symmetric zero-diagonal weight matrix, thresholds, update-order policy, and iteration bound. Output is a candidate attractor with energy trace, convergence state, decoded canonical IDs/evidence, or an explicit nonconvergence result.

## Formula

```text
E(s) = -(1/2) s^T W s + theta^T s
```

Classical asynchronous update for component `i`:

```text
s_i <- sign(sum_j w_ij s_j - theta_i)
```

Use a declared tie rule for zero, e.g. retain the prior `s_i`. For the classical energy guarantee, `W` is symmetric with zero diagonal and updates are asynchronous.

## Symbols

| Symbol | Meaning | Type/range/unit | Research default |
|---|---|---|---|
| `s` | current binary pattern | `{-1,+1}^N` | required |
| `N` | representation dimension | positive integer | experiment-specific |
| `W` | association matrix | real symmetric `N×N`, diagonal zero | learned from patterns |
| `w_ij` | association between units | real | matrix element |
| `theta` | unit thresholds | `R^N` | zero baseline |
| `E(s)` | energy | real, arbitrary units | computed |
| `sign` | threshold function | negative→-1, positive→+1, zero→retain | defined |
| `T` | maximum async sweeps | positive integer | bounded, e.g. 50 |

## Pseudocode

```text
validate W symmetric and diagonal zero
s = encode(query cue)  // encoding is a separate frozen experiment
for sweep in 1..T:
    changed = false
    for i in deterministic or seeded-recorded asynchronous order:
        next = sign(dot(W[i], s) - theta[i]); zero retains s[i]
        assert energy(next_state) <= energy(current_state) + tolerance
        update s_i; changed |= next != previous
    if not changed: return decoded attractor with evidence links
return bounded-nonconvergence result
```

## Complexity

Dense updates cost `O(TN²)` time and `O(N²)` memory; sparse matrices cost `O(T|E|)`. Encoding/decoding and modern continuous Hopfield variants have separate contracts and are out of this formula's scope.

## Edge and security cases

Spurious attractors, limited capacity, correlated patterns, encoding bias, privacy memorization, adversarial cues, non-symmetric matrices, synchronous cycles, nondeterministic update order, and inability to cite source evidence are major risks. Retrieved patterns remain candidates and require canonical-source provenance.

## Worked example

Let `W=[[0,1],[1,0]]`, `theta=[0,0]`, and `s=[1,-1]`. Energy is:

```text
W s = [-1,1]
s^T W s = -2
E(s) = -0.5(-2) = 1
```

Asynchronously updating unit 1 yields `s=[-1,-1]`, whose energy is `-1`; the energy decreased. Synchronous updates would alternate `[1,-1]` and `[-1,1]`, illustrating why update semantics matter.

## Golden vectors

| `W`, `theta`, `s` | Expected |
|---|---|
| zero `W`, zero theta, any `s` | `E=0`, tie rule retains state |
| two-node matrix above, `[1,1]` | `E=-1` stable |
| two-node matrix above, `[1,-1]` | `E=1`; async update can reach energy `-1` |
| non-symmetric `W` | rejected for classical mode |

## Calibration and rollback

Compare cue-completion accuracy, false-attractor rate, source-evidence recovery, capacity versus pattern correlation, latency/memory, privacy attack rate, and simpler nearest-neighbor/vector baselines. Since status is research-only, failure means no product integration; remove feature flags and retain only reproducible research results.
