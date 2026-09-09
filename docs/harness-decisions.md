# Harness decisions

Engine implementation is paused for evidence-first evaluation. The question is which
workloads should reuse a candidate, specialize it, or own a minimal subset—not which
library wins a universal speed ranking. No dependency selection is validated yet.

## Workflow and first exit

Specify behavior, write an independent literal oracle, then implement a small
vertical slice through the baseline and one adapter. Candidate agreement alone is
not correctness. Introduce targeted faults to prove assertions notice a frame offset
and missing end silence in the first slice. When seek and loop contracts arrive,
also check that tests catch a dropped seek and a wrong loop endpoint.

The first slice exits with the [literal oracle](harness-contract.md), a minimal
direct-loop baseline, one external adapter and an honest conformance/capability
outcome. A documented unsupported capability is useful evidence; exit does not
require invented emulation to hide it. Correctness outcomes determine which scopes
may enter [measurement](harness-measurement.md).

A future single local/CI verify command should run deterministic contract and adapter
checks. That command does not exist yet. Benchmark runs stay separate, with no flaky
speed-threshold gates. Runner APIs, crate details and adaptation choices remain
proposals to settle during implementation discussion.

## Per-workload decision record

| field | evidence to retain |
|---|---|
| Question and contract | Workload, required behavior, fixture/schema version and quality settings |
| Candidate and adapter scope | Candidate version, adapter revision, native/emulated behavior and capability gaps |
| Correctness | Literal oracle results, errors, rejected requests and targeted fault checks |
| Raw evidence | Run directory and manifest, conformance and trial records |
| Costs | Preparation, memory and performance with explicit timing boundaries and variability |
| Limitations | Unsupported behavior, host/platform coverage and unverified integration constraints |
| Reuse decision | Reuse, specialize, own a minimal subset or defer, with reasons specific to this workload |
| Revisit triggers | Contract changes, new candidate versions, integration findings or new measurements |

The broader [engine milestones](milestones.md) remain future intentions. Harness
success neither implements those stages nor proves realtime behavior or mix parity.
Eventual integration remains a thin native Node binding and `@openflow/audio` API
beside the Rust core, as described in [architecture](architecture.md).
