# Flat-dispatch experiment

The fused renderer is the default for supported graphs. `RenderState::new(program)`
selects the fused kernel when
compilation recognizes one output sum of distinct `gain(player)` branches. Each
player and gain must occur once and every graph node must be used. Sum input order
is preserved even when it differs from player/node order. Shared branches, gain
chains, unused players, extra outputs and all other shapes fall back to Generic.
`active_strategy()` exposes the actual path for checks.

The experiment removes per-sample generic opcode dispatch and intermediate node
scratch reads/writes. It retains separate f32 multiply/add operations, player
advancement, per-sample command application, half-open boundaries and the existing
scan enqueue strategy by default. It introduces no FMA/reassociation, explicit SIMD,
event-span batching or parallel rendering. Generic code remains the
control; scratch allocation is retained even for the fused path. Shape recognition
and a compact voice list add preparation work/memory. Both paths use the same
prepared Program shape in the controlled comparison.

Run the integrated comparison and diagnostic report after stopping the watcher:

```sh
./harness/verify --full --dispatch-experiment --profile-renderer flat-fused --profile-seconds 3 --sample --output-root /tmp
```

`--dispatch-experiment` is opt-in; ordinary `--full` runs do not repeat it. It runs
checks first, then an uninstrumented same-binary Generic/FlatFused/FlatFusedCounted comparison for all
eight existing workloads. Each of two replicates has three warmups and twelve
retained trials, rotating all three strategies by round/replicate. Workloads,
commands, 256-frame blocks, compiler/target/options and timing boundaries match.
Every trial validates PCM and final state. Timed scope stays enqueue + preallocated
render + capture, with fixture/events/validation outside; these are not kernel-only
speed figures. No speed threshold controls success.

The separate `dispatch-experiment` binary links no profiling allocator. A completion
marker binds `experiment.json`, including raw integer trials, gates, errors,
ordering, exact workloads and build metadata. The consolidated report validates it
and groups neutral per-replicate medians/ranges/preparation costs by workload. A
failed gate/execution prevents comparisons and returns failure with retained data.
Canonical reference comparisons now use FlatFused with Scan enqueue; unsupported
graphs fall back to Generic. The existing coloring rule is unchanged; experimental
rows are not performance-colored. The study manifest records both default choices.

`--profile-renderer flat-fused` selects only the Offline diagnostic profile path;
reference adapters remain unchanged. Reports display the selection. For a separate
Generic diagnostic control use `python3 harness/profile.py --renderer generic`
with matching workload, seconds and output-root settings. Profile CPU per output
frame and allocation requests per complete workload use actual validated trial/frame
denominators. Fixture/oracle, capture, validation and preparation remain separate
from engine/adapter enqueue/render attribution. Hardware instructions per frame
remain unavailable until event identities and per-core deltas are validated.

Tests exercise both strategies across seeded workloads/partitions, literal control
oracles on an eligible flat graph, event boundaries, revisions/rejection atomicity,
source end/loop/seek, fallback topologies and unused-player advancement. Order-sensitive
f32 sums, signed zero and an FMA-sensitive rounding case check exact math. Allocation
instrumentation runs in separate tests for loop and dense-command render paths.

## Independent enqueue control

`RenderState::new_with_strategies(program, renderer, EnqueueStrategy::Counted)`
selects the enqueue experiment independently. Scan remains the default control.
Both modes maintain a pending command count, decrement it when a queued batch is
consumed, and clear it on revision advance. Counted replaces the two full pending
queue scans with the cached total and binary partition searches for the same-frame
range. Only that range is summed; sorted insertion and stable same-frame ordering
remain unchanged. Validation happens before any mutation or queue reclamation.
Both modes retain preparation-side queue reclamation and no render allocations.

The three experiment labels mean Generic+Scan, FlatFused+Scan, and FlatFused+Counted.
Compare the first two to isolate rendering and the latter two to isolate enqueue.
The same binary rotates 576 retained trials, with correctness gates for all modes.
For separate diagnostics use `python3 harness/profile.py --renderer flat-fused
--enqueue counted` (plus workload, seconds, and output root). The profiling binary
accepts the renderer followed by enqueue mode as optional final positional values.
`profile.json` records `render_strategy` and `enqueue_strategy`; diagnostic timings
remain ineligible for comparative speed claims.
