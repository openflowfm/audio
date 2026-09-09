# Harness contract

Proposed behavior for a comparison harness, not an implemented engine API. The
[adapters](harness-adapters.md) share these contracts; candidate agreement is not an
oracle. All frame indices are zero-based.

## First slice

Loading an immutable asset, compiling a graph and initializing its processors are
distinct preparation operations. Loading alone neither starts playback nor advances
render time. This slice uses one mono f32 asset, equal 48 kHz source/output rates,
unity gain and exact copying, with no implicit smoothing or resampling.

The proposed observable state is `(output_frame, source_frame, status)`. Observations
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
of this slice: stopped silence describes the initial state.

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

## Future contracts

Stop, seek, looping and cancellation are later slices. Before adapters implement
each, settle same-frame ordering, half-open ranges and endpoints, past-event
handling, and rejection side effects, including pending work. The first slice avoids
same-frame start conflicts by rejecting any second pending start.

For a future loop oracle, asset `[0.125, 0.25, 0.5, 0.75]`, region `[1,3)` and
initial source position 1 produce six frames `[0.25,0.5,0.25,0.5,0.25,0.5]`.
This example fixes the exclusive loop endpoint, not the remaining loop command
semantics. It is not a first-slice requirement. Future DSP comparisons must justify
their tolerances separately; exact-copy tests never silently gain smoothing.
