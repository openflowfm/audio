# Harness adapters

This layout is proposed pending discussion. No Rust scaffold or engine implementation
exists. A Rust workspace could separate a dependency-light contract/fixture model,
a correctness runner, a benchmark runner and individual candidate adapter crates.
Public APIs and crate boundaries remain unresolved.

Use deterministic in-memory f32 fixtures and versioned JSON workload definitions.
Adapters return typed PCM plus observable state and typed error results. Every
comparison fixes inputs, events, rates, channels and quality settings. Unsupported
behavior is a capability gap, not permission to quietly change the workload.

| candidate | proposed comparison scope |
|---|---|
| Firewheel | Graph and playback workloads |
| FunDSP | Graph and DSP workloads |
| Faust | Generated-processor unit comparisons; not an equivalent engine |
| Minimal direct loop | Literal-copy baseline with explicitly limited scope |

Candidate fit, versions, licenses and integration constraints are **unverified**
pending primary-source review. These names are evaluation candidates, not selected
dependencies. Processor-only results cannot establish scheduler or graph capability.

## Scheduling and cost attribution

Record whether scheduling is **native** or **adapter-emulated**. An adapter that
splits blocks at event boundaries can establish candidate-plus-adapter correctness;
it does not establish a native scheduler capability. Do not invent emulation just
to conceal missing behavior. Report both the gap and any deliberate adaptation.

Keep external block partitions fixed across candidates in timed workloads. Record
internal splitting and include dispatch, splitting and adaptation in end-to-end
timing. Additional internal kernel figures must be labeled with their narrower
scope. Kernel-only tests are distinct workloads from full event scheduling.

Implement only the [first-slice contract](harness-contract.md) initially. A harness
adapter is an experimental boundary, not a commitment to the eventual thin native
Node binding and `@openflow/audio` API described in [architecture](architecture.md).
