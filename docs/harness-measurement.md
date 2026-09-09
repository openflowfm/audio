# Harness measurement

Correctness gates precede benchmarking for each workload. Failures and unsupported
capabilities remain evidence; they are not speed competitors. These Rust CLI runners
and artifact formats are proposed, not existing commands.

Each run creates a unique directory outside the standalone audio repository, beneath
an existing temporary or user-chosen output root. The repository root is the parent
of the `harness` Cargo directory. Historical external artifacts and their recorded
paths and metadata remain unchanged when the repository moves.

| artifact | proposed contents |
|---|---|
| `manifest.json` | Schema version; source revision and dirty state; compiler and dependency versions; hardware/OS; release, optimization and target options; sample rate, channels and external block settings; seed, warmup, repeats, trial ordering and timing boundary |
| `conformance.json` | Workload and fixture versions, candidate/adapter identity, native or emulated scheduling, pass/fail/capability gaps, expected and observed state/errors, mismatch details |
| `timings.jsonl` | Raw per-trial integer nanoseconds, frame count, measured scope, candidate/workload identity and trial order |
| `summary.json` | Derived statistics, variability/uncertainty and links to contributing trials |

A human report is generated from stored data, not an independent source of truth.
Track small fixtures; keep large artifacts untracked. Save PCM only for failures or
on request, with sample format, rate and channel metadata. Public result publication
is not authorized.

## Timing boundaries

Fixture construction, loading, graph compilation and processor initialization occur
outside render timing. Report their preparation durations and memory costs separately.
End-to-end rendering includes event dispatch and all adapter work. Consume output
observably so optimization cannot discard the workload. Expensive validation or
checksums belong outside the timer, or must be identically included and reported for
every candidate. Do not log per sample. Run allocation instrumentation separately
from speed measurements.

Retain raw repeats, seeds and trial ordering; report variability and uncertainty
rather than only a best run. Use the same fixed external partitions and quality
settings across candidates; label narrower kernel-only results separately. Record
enough configuration to reproduce both conformance and measured scope.

Offline throughput does not establish realtime reliability. Callback percentiles,
worst observed duration, deadline misses and underruns wait for a native host.
Worst observed duration is not a worst-case bound. Memory and preparation costs
remain separate evidence even when offline throughput looks favorable.
