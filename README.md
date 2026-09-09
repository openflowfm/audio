# audio[flow]

A planned reusable, headless Rust audio engine library for open[flow], with
mix[flow] as its intended first consumer. This independent repository contains
engine design documentation and a Rust comparison harness with source, tests,
a locked Cargo package, fixtures and a Python report generator. It is not yet a
production audio engine or an application.

Engine implementation remains paused. Existing harness code and local evidence
are retained for evaluation; relocation does not resume implementation or select
a production dependency.

This is an index. Read only the topic that matches the work. Some topic documents
describe the original proposal and have not yet been reconciled with the harness.

| touching | read | source governed |
|---|---|---|
| purpose, scope, or the boundary with applications | [Intention](docs/intention.md) | future library and consumer boundaries |
| graph compilation, rendering, timing, assets, or processor contracts | [Architecture](docs/architecture.md) | future compiler, runtime and operations |
| implementation sequence, validation gates, or mix feature coverage | [Milestones](docs/milestones.md) | future implementation sequence |
| first-slice behavior and literal fixtures | [Harness contract](docs/harness-contract.md) | [contracts](harness/src/contracts.rs) and [fixtures](harness/fixtures/first-copy.json) |
| candidate boundaries and scheduling adaptation | [Harness adapters](docs/harness-adapters.md) | [reference](harness/src/reference.rs) and [external](harness/src/external.rs) adapters |
| timing boundaries and interpreting artifacts | [Harness measurement](docs/harness-measurement.md) | [results](harness/src/results.rs) and [report generator](harness/report.py) |
| workflow and reuse decisions | [Harness decisions](docs/harness-decisions.md) | evaluation and decision records |
| offline rendering behavior | [Offline contract](docs/offline-contract.md) | [offline runtime](harness/src/offline.rs) |
| external candidate reference review | [Reference review](docs/harness-reference-review.md) | candidate evaluation |

Run the existing checks from this repository root:

```sh
./harness/verify
```

The command checks formatting, builds and tests the Rust harness, and runs the
Python report tests. It does not run benchmarks. Cargo requires Rust and the
locked dependencies; report tests require Python 3.

Run artifacts belong in an existing directory outside this repository, selected
with `--output-root`. Historical results remain at their original external paths;
relocation does not rewrite their metadata. Copied Cargo build artifacts may embed
the old source path, so rebuild before running the harness from its new location.
