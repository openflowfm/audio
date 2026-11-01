# audio[flow]

An experimental reusable, headless Rust audio library for open[flow], with
mix[flow] as its intended first consumer. This independent repository contains
an offline Graph → Program → mutable render-state implementation and a comparison
harness with source, tests,
a locked Cargo package, fixtures and a Python report generator. It is not yet a
production audio engine or an application.

The local offline MVP covers RAM players, fixed gain/sum, named outputs and timed
atomic command batches. An [experimental native Node binding](native/README.md) reuses that renderer and
queries output-device capabilities without opening streams. Native output,
stretching and application integration remain deferred. Comparative results do not select a production engine.

This is an index. Read only the topic that matches the work.

| touching | read | source governed |
|---|---|---|
| purpose, scope, or the boundary with applications | [Intention](docs/intention.md) | future library and consumer boundaries |
| graph compilation, rendering, timing, assets, or processor contracts | [Architecture](docs/architecture.md) | future compiler, runtime and operations |
| implementation sequence, validation gates, or mix feature coverage | [Milestones](docs/milestones.md) | future implementation sequence |
| first-slice behavior and literal fixtures | [Harness contract](docs/harness-contract.md) | [conformance](harness/src/conformance.rs) and [fixtures](harness/fixtures/first-copy.json) |
| candidate boundaries and scheduling adaptation | [Harness adapters](docs/harness-adapters.md) | [reference](harness/src/reference.rs) and [external](harness/src/external.rs) adapters |
| opt-in single-thread flat-dispatch control experiment | [Dispatch experiment](docs/harness-dispatch-experiment.md) | [offline runtime](harness/src/offline.rs) and paired experiment |
| unified checks, measurements and generated reports | [Test and report runner](docs/harness-runner.md) | [runner](harness/run.py) and [verify](harness/verify) |
| opt-in CPU, allocator and process I/O diagnostics | [Harness profiling](docs/harness-profiling.md) | [profiling launcher](harness/profile.py) and diagnostic executable |
| timing boundaries and interpreting artifacts | [Harness measurement](docs/harness-measurement.md) | [results](harness/src/results.rs) and [report generator](harness/report.py) |
| workflow and reuse decisions | [Harness decisions](docs/harness-decisions.md) | evaluation and decision records |
| experimental native Node binding and device enumeration | [Native bridge](native/README.md) | [addon](native/src/lib.rs) |
| offline rendering behavior | [Offline contract](docs/offline-contract.md) | [offline runtime](harness/src/offline.rs) |
| external candidate reference review | [Reference review](docs/harness-reference-review.md) | candidate evaluation |

Run the existing checks from this repository root:

```sh
./harness/verify
```

The command checks formatting, builds and tests all Rust targets and doctests, runs
the Python suites, and automatically produces a consolidated HTML report with raw
logs. It does not run benchmarks by default. Add `--full --profile-seconds 2 --sample`
to run the canonical study and bounded diagnostic profiles in the same invocation.
See the [runner guide](docs/harness-runner.md). Rust and Python 3 are required.

Run artifacts belong in an existing directory outside this repository, selected
with `--output-root`. Historical results remain at their original external paths;
relocation does not rewrite their metadata. Use the [measurement commands](docs/harness-measurement.md) to rebuild, run the
study and generate a standalone local report.
