# Harness adapters

One locked Rust package in `harness/` contains a small reusable offline module and
its comparison tooling. It is deliberately not a production package layout. The
library's [offline module](../harness/src/offline.rs) imports no harness oracle,
transport, device or application policy; the runners depend on it, not vice versa.
Crate separation and the eventual Node/TypeScript API remain future decisions.

| source | implemented responsibility |
|---|---|
| [model](../harness/src/model.rs), [fixture](../harness/fixtures/first-copy.json) | Versioned dependency-light first-slice input, PCM/state/error boundary |
| [conformance](../harness/src/conformance.rs), [contracts](../harness/src/contracts.rs) | Independent literal checks and retained mismatch evidence |
| [reference](../harness/src/reference.rs), [external](../harness/src/external.rs) | Direct-copy and pinned FunDSP first-slice adapters |
| [offline](../harness/src/offline.rs) | Assets, pure Graph compilation, immutable Program, explicit mutable RenderState |
| [study](../harness/src/study.rs) | Deterministic workload model, flat direct/FunDSP adapters, correctness and measured runs |
| [results](../harness/src/results.rs), [report](../harness/report.py) | Versioned artifacts, completion checks and derived local report |

Inputs are in-memory mono f32 at 48 kHz. The tracked literal JSON and generated,
retained workload definitions use schema version 1. All candidates in a workload
receive the same samples, events, rates, channels, fixed gains and external blocks.
First-slice adapters return typed errors; study records distinguish failure from
an explicit capability gap. Neither is a passing speed competitor.

| candidate | evaluated scope and limitation |
|---|---|
| Offline MVP | Acyclic player/gain/ordered-sum graphs, named outputs and the [offline command contract](offline-contract.md) |
| Direct loop | Specialized flat player/gain/sum baseline with fixed loops and adapter-scheduled starts; no general graph or timed stop/seek adapter |
| FunDSP 0.20.0 | Native WavePlayer and fixed gain, adapter scheduling/position tracking/sum; scalar tick API, not optimized block/SIMD integration |
| Firewheel | Deferred; no capability or license review completed |
| Faust | Deferred processor-unit comparison; not an equivalent engine |

FunDSP's exact version, enabled features, license option and inspected native
boundaries are recorded in the [primary-source review](harness-reference-review.md).
It is an experimental dependency, not a production engine selection.

## Scheduling and attribution

The offline renderer handles batches natively at output-frame boundaries. The flat
references emulate scheduled starts in their per-frame adapter; FunDSP position is
adapter bookkeeping because the selected WavePlayer API does not expose it. Fixed
loops are prepared natively in WavePlayer. Timed stop/seek/atomic cancellation is a
gap of these reference adapters, not a claim that all of FunDSP lacks those features.

All dispatch, adaptation and ordered summation are included in end-to-end study
timing. External timed blocks are fixed at 256 frames, with a shorter final block;
conformance also uses 1, 64 and 257. No kernel-only timings are currently produced.
Future block splitting or narrower kernel measurements must be labeled explicitly.
The flat references intentionally cover less behavior than the graph runtime; their
costs are not a universal engine ranking.
