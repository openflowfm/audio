# Harness contract

Implemented first-slice adapter contract, not the production engine API. The
[adapters](harness-adapters.md) share these contracts; candidate agreement is not an
oracle. All frame indices are zero-based.

## First slice

Loading an immutable asset, compiling a graph and initializing its processors are
distinct preparation operations. Loading alone neither starts playback nor advances
render time. This slice uses one mono f32 asset, equal 48 kHz source/output rates,
unity gain and exact copying, with no implicit smoothing or resampling.

The observable state is `(output_frame, source_frame, status)`. Observations
occur between rendered spans: `output_frame` is the next frame to render and
`source_frame` is the next source frame to copy. Initially these are `(0, 0, stopped)`.
Rendering while stopped advances only output time and emits zeros. An accepted
scheduled start leaves status stopped until its frame is rendered; pending events
are separately observable. The start applies before producing that output frame.
After the last source sample is copied, source position stays at asset length and
status becomes ended. Later output is silent, with no implicit retrigger.

The first slice accepts one start for a prepared, stopped player, at or after the
next output frame. A second pending start, an ended-player restart, a past timestamp,
an unknown asset/player, an invalid frame value, or an unprepared target is rejected
with a stable typed error. Invalid assets (empty, non-finite samples or unsupported
format) are rejected during loading. Rejections leave assets, active graph, positions
and pending events unchanged; validation precedes mutation. No stop command is part
of this slice: stopped silence describes the initial state. Zero-length rendering
is a no-op, including with a pending start; it is distinct from a malformed
zero-sized benchmark partition. Load, compile and initialize readiness are observed
separately. Repeated preparation is rejected without changing the active asset.

### Literal oracle

Load `[0.25, -0.5, 0.75, -1]`; prepare the player and schedule its start at output
frame 3 before rendering. Render exactly ten frames:

```text
[0, 0, 0, 0.25, -0.5, 0.75, -1, 0, 0, 0]
```

Compare external partitions `[10]`, `[1,1,1,1,1,1,1,1,1,1]` and `[3,2,5]`.
Concatenated samples must match exactly, as must final state `(10, 4, ended)`, an
empty pending-event list and no errors. At output boundary 3, the player is still
stopped at source 0 with its start pending; at boundary 5 it is playing at source 2;
at boundary 7 it is ended at source 4. Observe common boundaries in separate probes
when a partition does not expose them. Also test stopped-only rendering and each
rejection independently, including unchanged state after rejection.

## Offline extension

Stop, seek, looping and revision cancellation are implemented separately under the
[offline contract](offline-contract.md), which settles ordering, half-open endpoints,
past events and rejection side effects. The narrow first-slice adapter still rejects
a second pending start; the richer offline command API accepts ordered batches.
Future DSP or asynchronous preparation contracts must settle these questions before
new adapters implement them.

The offline loop oracle uses asset `[0.125, 0.25, 0.5, 0.75]`, region `[1,3)` and
initial source position 1 to produce six frames `[0.25,0.5,0.25,0.5,0.25,0.5]`.
Its full command semantics are in the offline contract. It is not a first-slice
adapter requirement. Future DSP comparisons must justify their tolerances separately; exact-copy tests never silently gain smoothing.
