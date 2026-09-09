# Harness decisions

A local offline MVP and comparison harness are implemented. Evidence still guides
whether each workload should reuse a candidate, specialize it, or own a minimal
subset. The FunDSP reference is not a production dependency decision; native output,
application integration, streaming and stretching remain outside this pass.

## Verification and evidence

The first slice has a hand-written ten-frame oracle, a direct-copy baseline and a
pinned external adapter. Offline start/stop/seek/loop/cancellation slices have their
own literal PCM and state expectations. Seeded tests use an independent closed-form
source-position oracle rather than candidate agreement. The reference implementation
and oracle do not share a playback state machine.

[Faulty test adapters](../harness/tests/conformance.rs) must lose on shifted,
truncated, extra, stale-after-end, non-finite and state-corrupt output. Additional
[faults](../harness/tests/faults.rs) drop a seek, include a wrong loop endpoint,
partially accept an invalid atomic batch or revive a stale start. Serialization,
completion and report tests apply the same pattern to the harness itself, using
synthetic integers and deliberately malformed evidence.

`./harness/verify` is the local/CI entry point: Rust formatting, build, deterministic
Rust tests and Python report tests. Allocation checks are separate from timing.
The [study command](harness-measurement.md) runs benchmarks separately, with no flaky
speed-threshold gate. Correctness failure or a capability gap remains visible and
excludes the pair from performance comparison.

## Per-workload decisions

The generated report and raw artifacts are local evidence. Evaluate them with this
record before choosing a production implementation:

| field | evidence to retain |
|---|---|
| Question and contract | Workload, required behavior, fixture/schema version and quality settings |
| Candidate and adapter scope | Version, adapter revision, native/emulated behavior and capability gaps |
| Correctness | Literal oracle, positions/errors, rejected requests and targeted fault checks |
| Raw evidence | Run directory, manifest, conformance and trial records |
| Costs | Preparation, memory scope and performance boundaries with observed variability |
| Limitations | Unsupported behavior, selected API path, platform/host and integration coverage |
| Reuse decision | Reuse, specialize, own a minimal subset or defer, with workload-specific reasons |
| Revisit triggers | Contract changes, candidate updates, native integration or new measurements |

For flat playback/gain/sum, compare the specialized baseline with the selected
FunDSP scalar path and the broader offline graph. Defer a production choice until
block/SIMD integration and actual consumer preparation/memory needs are evaluated.
For timed control bursts, the current reference adapters have a gap: retain the
owned command implementation as the experimental baseline, without claiming a
competitive result for missing adapters.

The [milestones](milestones.md) distinguish the implemented offline proof from native
reliability and mix parity. Eventual integration remains a thin native Node binding
and `@openflow/audio` API beside the Rust core, not a separate server process.
