# Test and report runner

From the repository root, run:

```sh
./harness/verify --output-root /tmp
```

This one command runs formatting, locked Cargo build, all Rust targets through
Cargo/libtest, Rust doctests, and all Python `*_test.py` suites through unittest.
It automatically writes and prints a standalone `report.html`, including the actual
runner logs and exit statuses, under a fresh external `audio-run-*` directory.
No benchmark or allocation instrumentation is enabled for normal correctness runs.
The profiling allocator has its own binary/tests; it does not instrument the study.

For the complete checks → canonical study → diagnostic profiles → report workflow:

```sh
./harness/verify --full --profile-seconds 2 --sample --output-root /tmp
```

Equivalent entry point: `python3 harness/run.py` with the same flags. `--study` and
`--profile` explicitly enable either measurement stage on its own. `--full` enables
both. `--sample` requests macOS stack sampling; attach failure or unsupported
platform metrics are shown as unavailable, not fabricated or treated as a failed
correctness test. The profile launcher currently supports macOS and Linux.

The full-run profile matrix selects `loop-32` and `bursts-16-dense`, each with Offline,
Direct and FunDSP. Each supported pair runs for up to the configured 1–60 seconds,
checked between complete trials, with a maximum of 100,000 trials. Defaults mean
four eligible two-second profiles plus two retained reference burst capability gaps.
Builds, gates, sampler startup and artifact writing add time. Each command has a
300-second timeout; timeout/launch failure is retained as failure evidence. On POSIX,
the runner kills the timed-out stage's process group to avoid orphaned work.

Independent correctness commands continue after a failure so their evidence is
retained, then all measurement stages are skipped. A study correctness or evidence
failure also prevents profiling. A profile correctness/execution/evidence failure
makes the overall run fail; other selected profiles still run to retain their results.
A proven reference capability gap requires passing literal gates and no failed
workload checks; it is shown separately and is not an overall failure. No elapsed
time, allocation count or speed comparison is a pass/fail threshold.

`run.json` records the stage commands, exit statuses, wall durations, selected mode,
platform, actual study/profile evidence and artifact locations. Each stage also has
its own JSON and complete stdout/stderr log. `report.html` is automatically rendered
from that persisted run data, including on failure; `completion.json` records SHA-256
hashes and the overall status. Completion does not mean success: inspect `status`.
The process returns nonzero for correctness, command, timeout or invalid evidence
failures. If the OS kills the entire runner or storage itself fails, report completion
cannot be guaranteed; stage checkpoints/logs already written remain.

Study and profile outputs stay in distinct child directories of that same run. The
runner verifies the existing study completion/checksums and summary consistency,
and verifies successful profile completion hashes before including measurements.
It never searches historical runs or carries results from another invocation.
Historical artifacts are never rewritten. The report includes all study capability
gaps, per-workload timing medians/min/max, literal/execution gate evidence, profile
phase counters and available whole-child resources. Raw repeats and stack traces
remain linked. Full libtest/unittest logs show individual test outcomes and summaries,
including ignored tests; stage wall time is not individual test execution time.

Canonical timing retains three warmups and twelve measured repeats with rotating
candidate order. Diagnostic CPU/allocation/RSS/I/O figures come from separate
instrumented runs. Preparation, enqueue, preallocated render, capture, validation,
fixture/oracle work and artifact-write scopes are labeled in the report; see
[profiling](harness-profiling.md) for precise boundaries and platform limitations.
Normal correctness CPU/allocation/RSS/I/O are unavailable. No disk pressure,
streaming performance, production engine choice or realtime guarantee is inferred.

Runner behavior is tested through unittest, including real failing-test subprocesses,
nonzero exit propagation, continued independent checks, skipped measurements,
retained failure reports, launch errors/timeouts, HTML escaping, completion hashes,
and capability-gap separation. Test fixtures are not performance evidence.

The generated report groups canonical statistics and resource charts by workload.
Candidate identity is explicit: audio[flow] MVP (ours), Direct reference, FunDSP
scalar reference, FunDSP native block reference and Oddio reference. Historical
three-candidate runs retain scalar FunDSP identity without invented new series. CPU/allocation and coarse I/O totals are divided by validated
profile trials; the denominator is displayed for each candidate. RSS and Rust live
requested-byte peaks remain high-water values. Primary wall-time charts show every retained trial and the lower-middle median,
with a zero origin and a shared scale within each workload comparison. Diagnostic
CPU breakdowns show phase means in CPU ns/output frame, separately from wall time. Uncollected and unsupported metrics remain labeled. The all-candidate inventory keeps actual canonical durations visible and neutral
except the blue Direct reference. Independent pairwise tables and charts compare
ours with each named eligible reference; no global competitive rank is implied. The faster eligible
median is green. The slower is yellow through 10% slower and red above 10%, with
percent slower calculated against the faster median and displayed. The threshold
is `SLOWER_YELLOW_PERCENT` in `harness/impact_charts.py`. Exact ties say tied and stay
neutral. Missing or sole eligible competitors stay neutral. A zero faster median
has no defined percentage; a positive slower duration is red.
Direct reference ("Purpose-built implementation of this workload.") is always blue
and excluded from competition, even when a metric is unavailable; no missing bar
is fabricated. Our canonical row remains bold. Controlled strategy comparisons
remain separately ranked within each replicate, and diagnostic resources stay neutral.
Logs and raw counters remain
expandable.

New timestamped diagnostic plots show interval CPU/allocation/I/O rates and observed
RSS/live bytes from real samples. Raw times, cumulative counters and phase-scoped
fault/context-switch deltas remain expandable. See the profiling guide for origins,
normalization and sampling overhead. Older runs explicitly lack these series.

`--dispatch-experiment` adds a same-binary strategy comparison after checks.
Historical two-path evidence remains supported. Three-path evidence presents
Generic→FlatFused and FlatFused→FlatFusedCounted as separate changes.
`--profile-renderer flat-fused` is the default Offline diagnostic selection;
canonical strategy selection belongs to the study binary and is retained in its manifest. See [dispatch experiment](harness-dispatch-experiment.md).

Impact views keep canonical reference comparisons, controlled optimization replicas,
and diagnostic profiles inside each workload section. The optimization view plots
all retained trials per strategy/replicate on one axis and reports the difference
of the two medians in milliseconds and percent of the control median. Negative
savings are displayed as such; a zero control has no percentage. Dots retain trial
identity via tooltips; their vertical offsets only separate observations visually.
No statistical significance, cross-run pairing, or causal phase attribution is inferred.

The diagnostic breakdown stacks only measured enqueue, render and capture process
CPU, divided by completed trials × output frames. Missing phases are explicitly
partial/unavailable; fixture/oracle and other harness phases remain separate.
Allocation/memory/process charts and raw resource time-series are secondary
expandable details. Trial counts, units and timing scope accompany the main charts.
Historical artifacts are not regenerated in place when previewing a design.

Each ours-versus-reference pair shows the absolute median time difference in
automatic ns/µs/ms/s units with less/more direction, percent less/more time using
that named reference as denominator, and the ours/reference time ratio. These
are elapsed-time comparisons, not throughput speedup percentages. The reference
row shows em dashes in ours-only columns. Unsupported comparisons say unavailable;
a zero reference median makes percentages and ratios undefined. The separate
color-threshold column uses the faster competitor denominator within that pair.

The canonical manifest declares candidate identities, requiring a complete
workload × candidate aggregate inventory, including capability gaps. New native
reference timings require their own literal gates and exact workload/partition
checks, not scalar FunDSP's gates. `native_gate_requirements` explicitly maps each
native candidate/workload to required gate IDs; all matching retained partitions
must pass. Unrelated-mode literal mismatches remain retained evidence and do not
exclude an independently conforming mode. Profiles carry `required_native_gate_ids`.
Empty requirements permit a profile gap only with explicit capability-gap checks;
an unexpected failure is never relabeled as unsupported. Pairwise timing scopes must match. The profile
matrix covers all five candidates for loop-32 and dense bursts; unsupported
profiles remain gap evidence. Native block FunDSP names its block API separately
from the historical scalar adapter; this is not a claim that every internal node
uses SIMD. Oddio is independently identified and never assigned fabricated values.
