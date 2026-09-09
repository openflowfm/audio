# Harness measurement

Correctness gates precede timings for each candidate/workload. The default CLI runs
only first-slice conformance. `--study` additionally runs literal offline gates,
seeded partition checks and real offline measurements; verification never invokes it.
From the repository root:

```sh
./harness/verify
cargo run --locked --release --manifest-path harness/Cargo.toml -- --output-root /existing/external/root
cargo run --locked --release --manifest-path harness/Cargo.toml -- --study --output-root /existing/external/root
python3 -B harness/report.py /existing/external/root/audio-harness-RUN
```

The last command prints a standalone `report.html` path. Open that local file in
Codex, or serve its run directory on loopback only for browser inspection. No remote
publication is part of these commands. `--save-failed-pcm` exports first-slice failure
PCM; study failures (including timed-output failures) retain lossless sample bits and
observed state in their evidence records.

## Retained evidence

Each run atomically reserves a unique directory beneath an existing external root.
Canonical paths, including symlinks, are checked against the standalone repository
root (the parent of `harness/`). No output may be written inside tracked source.
Existing files are never overwritten; partial runs remain inspectable. Historical
runs and their metadata are not rewritten when source changes or moves.

| artifact | schema-version-1 contents |
|---|---|
| `manifest.json` | Revision and dirty state at run; actual build compiler/profile/optimization/target/flags; exact dependency lock; available hardware/OS; rates, channels, blocks, warmups, repeats, order and timing/preparation boundaries |
| `workloads.json` (study) | Exact generated samples, resolved graph/events and versioned workload settings with deterministic seeds |
| `conformance.json` | Literal and partition outcomes, capabilities, positions/errors and failure evidence; study execution errors remain visible |
| `timings.jsonl` | Every retained trial: integer nanoseconds, frames and sample count, candidate/workload/order, timing scope, preparation duration and labeled logical byte counts |
| `summary.json` | Checked integer aggregates, lower median, observed min/max and speed eligibility; failed/gap workloads have no comparative metrics |
| `complete.json` | Written last after a valid summary; binds artifacts by byte count and FNV-1a checksum. Completion means fully written evidence, not passing conformance |
| `report.html` (derived) | Generated from a complete run, with summary arithmetic and correctness eligibility cross-checked against stored raw data |

A missing, invalid or mismatched completion marker means incomplete evidence, even
if `summary.json` exists. Checksums detect accidental changes, not hostile tampering.
The report refuses inconsistent summaries or synthetic performance claims. JSON
sample bits preserve f32 signs and NaN payloads on failure; optional `.f32le` files
include format/rate/channel/block metadata. Small fixtures stay tracked; run outputs
do not. Conformance-only runs have empty timings, not invented durations.

## Workloads and boundaries

The study uses one-shot playback, 1/8/32/64 looping voices, 8-voice bursts, denser
16-voice bursts and an 8-voice two-second render. Seeds 11–18 generate 257 exactly
representable samples. Workloads stay within 96,000 output frames, 64 voices and the
explicit graph/command limits in [offline contract](offline-contract.md). Burst
workloads exercise timed seek/start/stop at within-block offsets; simultaneous
commands and cancellation have separate literal and rejection tests.

Each supported pair gets three warmups and twelve retained trials, rotating
candidate order each round. The release binary, machine, samples, gains and external
blocks are shared. Rendering includes enqueue/dispatch/adaptation, writing prepared
buffers and an identical output-capture copy for every candidate. Input fixture,
command-list and oracle construction are excluded. Output is observably consumed;
full checksums/PCM comparison and state observation happen after timing. No per-sample
logging or allocation instrumentation runs inside speed measurements.

Preparation is timed separately: RAM asset load/copy, graph compilation or reference
setup, mutable state and output buffers. It is a combined duration, not a breakdown.
Logical asset and output-buffer bytes are reported; allocator totals, RSS and peak
memory remain unavailable. Separate allocation tests cover preallocated rendering
only, not enqueue, preparation, observation or teardown.

Raw variability, preparation costs and scope limitations are visible in the report.
Frames/s uses total frames / total seconds; the even-count median is the lower middle
sample. No confidence bound, universal fastest claim or realtime conclusion follows.
Callback percentiles, worst observed callback duration, deadline misses and underruns
require a native host, which does not exist here. Worst observed is never a
worst-case bound. Synthetic integer arithmetic tests are not performance evidence.
