# Diagnostic profiling

Use the separate executable and launcher from the repository root:

```sh
python3 harness/profile.py --output-root /tmp --workload loop-32 --candidate offline --seconds 5 --sample
python3 harness/profile.py --output-root /tmp --workload bursts-16-dense --candidate offline --seconds 5 --sample
python3 harness/profile.py --output-root /tmp --workload loop-32 --candidate direct --seconds 5
python3 harness/profile.py --output-root /tmp --workload loop-32 --candidate fundsp --seconds 5
```

Python 3, Rust and the locked Cargo dependencies are required. The launcher supports
macOS and Linux process resource accounting; automatic stack sampling currently uses
macOS `/usr/bin/sample`. On other platforms `--sample` records unavailable. On macOS,
sampling can fail due to attach permissions: retain `sample-launch.txt` and inspect
`process.json` rather than treating an empty stack file as evidence. No sudo is used.
The custom Cargo `profiling` profile inherits release optimization and retains debug
symbols. Build products remain under Cargo's target directory. Set `CARGO_TARGET_DIR`
if needed. The source archive and executable SHA-256 accompany each result.

The unique `audio-profile-*` directory contains `report.md`, `profile.json` (raw
phase counters), `artifact-writes.json`, `gates.json`, `process.json`, stdout/stderr,
source snapshot/patch, a `profile-complete.json` SHA-256 inventory on success, and sampling logs plus `stacks.txt` when available. Sampling
starts after the correctness gates, and covers the repeated diagnostic trials;
handshake/exit and instrumentation frames may also appear. Read call-tree ancestry
to distinguish oracle generation, enqueue and render. `sample` includes blocked and
idle stacks: its sample counts are not process CPU percentages. Short trials can
finish before enough samples accumulate. Increase repeats or select a substantial
workload before interpreting absence of a function as low cost.

Each selection runs the existing independent literal gates plus workload checks at
blocks 1, 64 and 257 before profiling. Reference burst capability gaps remain gaps:
they exit unsuccessfully with retained gate evidence and no profile. Each profiled
trial verifies PCM and final state. There are no warmups; repeat until either the
requested repeat count (default 100,000) or wall budget (default 5 seconds, maximum
60) is reached, checking the budget between complete trials. Gate/build/sampler
startup time is outside that budget. Ordering is a single selected candidate and
workload, with block 256. Repeat across candidates for diagnostic comparison.

The canonical `--study` path is unchanged: three warmups, twelve retained repeats,
rotating candidate order, and the existing measurement boundary. The allocator is
linked only into `audio-profile`. Diagnostic artifacts never enter the throughput
report's timing format. Do not use instrumented phase ratios as replacement speed
rankings or realtime/callback evidence.

Phase scope:

| Phase | Included |
|---|---|
| fixture_events_oracle | analytic PCM oracle and command construction |
| preparation | asset copy, compile/adapter setup, mutable state, output/tail and validation capture buffers |
| enqueue | adapter acceptance of all prebuilt batches, including consumed batch drops |
| render | preallocated render entry point, one probe per external block |
| capture | copy each rendered block into validation PCM |
| validation | PCM comparison and observable-state construction/comparison |
| teardown | drop prepared state, buffers and oracle |
| artifact write | profile.json serialization and write only |

Gates, workload selection, metadata commands/serialization outside the measured
write, sidecar writes and launcher writes are not assigned to trial phases.
Process counters include the entire child, including gates and its artifact writes;
compiler, launcher and sampling-process resource consumption are excluded. No sum
of phase values claims to explain the complete process lifetime.

Wall time uses `Instant`; CPU uses `getrusage(RUSAGE_SELF)` process CPU deltas on macOS/Linux
(microsecond timeval resolution). Clock calls, atomics and per-block probes perturb results,
especially capture and small workloads. CPU is actual consumed process time, not
wall time or I/O counters. Totals aggregate all trials, with call counts; they do
not supply a distribution or allocation-site backtraces. Stack sampling is a
separate line of evidence for functions within phases.

Successful Rust global allocator calls report request counts/bytes, frees, signed
live-byte changes and maximum absolute live requested bytes observed in each phase.
A successful realloc counts as freeing the old request and allocating the full new
request, even if it stays in place. This observes transient allocations as well as
retained state. It excludes native allocations bypassing Rust's global allocator,
allocator metadata, fragmentation, stacks and mapped pages. Peak live bytes include
other still-live harness objects; they are not a phase's exclusive memory. RSS is
the OS whole-process lifetime high-water mark, not a per-phase current footprint.

`wait4` supplies child user/system CPU, peak RSS, faults and input/output block
operation counts. Counters vary by OS and filesystem caching; zero does not prove
no I/O. Byte counts, latency, queue depth, throughput demand and disk pressure are
unavailable. Workload assets are RAM resident; artifact writes and executable paging
are possible I/O, not a streaming workload. No native callback or streaming path
exists. Interpret profiles as local diagnostic evidence, not production readiness.

## Timestamped resources and phase counters

New profile runs retain `allocation-samples.json` and `resource-samples.json`, bound
by the existing completion hashes. No historical time series is reconstructed.
The binary takes real samples after completed trials roughly every 50 ms, plus
initial/final samples: elapsed `Instant` time, completed trials, cumulative user and
system CPU, faults/context-switch counts, Rust allocation requests/requested bytes,
and live requested bytes. A preallocated sample buffer avoids sampling allocations
inside phases; that buffer still contributes to live bytes/RSS. Sampling after
teardown misses transient intra-trial live peaks. Cumulative values include startup;
interval deltas and phase deltas exclude the earlier baseline.

The launcher independently samples whole-child current RSS and OS-accounted read/
write bytes at approximately 50 ms intervals using macOS `proc_pid_rusage` v2 or
Linux `/proc/PID/statm` and `/proc/PID/io`. Each observation records elapsed monotonic
time at the read midpoint and read duration. Its origin is just before releasing
the post-gate handshake; the internal allocator timeline starts at the trial loop.
These origins are separate, and candidates run sequentially. Missing reads are null
with error text. Lines connect actual observations only; gaps are not filled.

Interval CPU utilization is delta process CPU / delta elapsed time × 100 (one CPU
= 100%). Allocation and I/O rates likewise use actual intervals, not requested
sampling frequency. CPU is not wall time or a hardware-stall counter. The sampler
thread lives in the launcher, so its own allocations are not the child's; its OS
reads/system load still perturb diagnostic performance. Canonical studies use none
of this instrumentation. Artifact writes are outside the internal timeline but may
appear in the external process timeline.

Each diagnostic phase also retains user/system CPU, faults, block operations and
voluntary/involuntary context-switch deltas from paired `getrusage` reads. These
exclude faults that occurred before the phase, including startup/gates. They are
process-wide probes, include observer work, and do not identify why a thread waited
or establish cache/memory stalls. Inspect phase counters and real sampled stacks
together. Disk-byte accounting does not establish device pressure; queue depth and
latency remain unavailable.
