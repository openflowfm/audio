# Offline MVP contract

This is the implemented local offline contract. The production
integration, native output, stretching and mix migration remain deferred. The
experimental Rust library lives beside the harness until packaging is settled.

## Preparation and graph

`Assets::load` accepts finite nonempty mono f32 RAM samples at 48 kHz. Immutable
assets may be shared; players never share mutable position. Loading, pure graph
compilation and mutable render-state initialization are distinct operations. Graphs
contain player, fixed gain and ordered sum nodes, with unique nonempty named mono
outputs. Compilation validates references, acyclicity, finite gains and explicit
limits before creating an immutable Program; a failed compile changes no active
state. Graph node IDs and player IDs are distinct: player IDs follow player-node
order. Each render state belongs to one Program. Summation follows declared input
order with ordinary f32 operations, with no clipping, smoothing or fused multiply-add.

Default limits: 128 assets, 1,048,576 total asset frames, 1,024 graph nodes, 128 players,
16 outputs, 4,096 total graph edges, 4,096 pending commands, 256 commands per batch
and per timestamp, and 4,096 frames per render block. Limits are explicit inputs;
zero or values above these hard ceilings are rejected. These are safe offline
budgets, not realtime guarantees. The render entry point writes caller-preallocated
output buffers; a convenience
allocating wrapper is for tests. Zero-frame rendering is a no-op. Allocation checks
run separately from speed runs; device-callback reliability remains unverified.

## Events, endpoints and observations

Frames are zero-based. Rendering `[t,t+n)` applies commands at frame `t` before its
sample; commands at `t+n` remain pending. Batches at the same frame apply in accepted
submission order; commands within a batch apply in list order. Start/stop therefore
resolve by the last command in that order. All commands are checked before enqueue:
unknown player, past timestamp, invalid seek or loop, wrong revision, empty/oversized
batch or exhausted queue rejects the whole batch with no queue or player mutation.
No accepted batch can become partly invalid at render time.

| command | exact effect at its output timestamp |
|---|---|
| Start | Play from retained source position; at source end with no loop, remain ended and silent |
| Stop | Stop before this sample, retaining source position and loop; silence until a later Start |
| Seek(source) | Accept `[0,asset_length]`; set next source position. Ended becomes stopped; playing remains playing until source-end handling. Does not implicitly start |
| SetLoop([start,end)) | Require `0 <= start < end <= asset_length`; retain position until rendering. Before any playing sample, a position outside the region normalizes to start |
| SetLoop(None) | Disable looping, retaining position |

After a playing sample, increment position, wrap at the exclusive loop endpoint, or
mark ended at asset length. Stopped positions do not advance or normalize. An
observation between blocks gives next output frame, each player's next source frame,
status and loop, active revision and pending batch count. Rendering silence still
advances output time. No implicit retrigger occurs at source end.

`advance_revision(new)` requires a strictly greater revision, clears all pending
batches and leaves current players/output time untouched. Enqueue must match the
active revision (initially 0). Cancellation is immediate between renders; it is not
a timestamped command. Rejected revision changes have no effect. A batch at the next
output frame is valid; a batch in the past is rejected rather than clamped.

## Independent literal gates

Keep the original ten-frame oracle and position observations from
[harness contract](harness-contract.md). Additional small oracles precede code:

- Asset `[1,2,3,4]`, start at 1, stop at 3, seek 0 and start at 5:
  eight frames `[0,1,2,0,0,1,2,3]`; final next-source 3, playing.
- Asset `[0.125,0.25,0.5,0.75]`, seek 1, loop `[1,3)` and start at 0:
  six frames `[0.25,0.5,0.25,0.5,0.25,0.5]`; final next-source 1.
- Independent players on `[1,2]`, first gain 0.5 starting at 0, second gain
  0.25 starting at 1, ordered sum: four frames `[0.5,1.25,0.5,0]`.
- Valid start plus invalid seek in one batch: reject both and render silence.
- Queue a start in revision 0, advance to revision 1: old start never plays;
  a newly submitted revision-0 command rejects without side effects.

Each runs under single-frame and uneven blocks. Targeted faulty adapters must lose
on dropped seek, wrong exclusive loop endpoint, partial atomic acceptance and stale
start revival. Seeded stress augments these literal oracles; candidate agreement
alone never replaces them. No audio device, Node binding, UI or mix change is in scope.

The generic renderer remains the default. An explicit [flat-dispatch experiment](harness-dispatch-experiment.md) recognizes one narrow topology and retains the generic fallback; it does not change this observable contract.
