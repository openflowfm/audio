# Architecture

This is the proposed contract, not an implemented API. Names below describe roles;
they do not prescribe a crate layout. Eventual integration is a thin native Node
binding and an `@openflow/audio` TypeScript API in the same repository as the Rust
core, not a separate server process. Engine implementation is paused for the
[comparison harness](harness-decisions.md); dependency choices remain unvalidated.

```text
editable Graph -> validate and prepare -> Program -> stateful rendering
                       off audio thread              offline or device callback
```

## Compiler, runtime and operations

The Graph describes nodes, connections, stable identities and output names. The
compiler validates port/channel compatibility, resource limits and dependencies,
then prepares a Program: processing order, reusable buffer allocation, processor
requirements and a state-compatibility plan. Rejected edits leave the active Program
running. A first implementation can execute block-processing Rust nodes through an
interpreter or execution plan. Native code generation or JIT can be investigated
later; neither is required to make this model useful.

| boundary | explicit contract to establish |
|---|---|
| Compiler to runtime | Validated Program, format/block limits, prepared processors and buffers, stable node/state identities, compatibility and activation revision |
| Operations/tooling to compiler | Editable graph and asset handles; cancellable preparation requests; validation diagnostics and readiness result |
| Controller to runtime | Bounded atomic timestamped command batches, target identities, cancellation/revision identity, acceptance or rejection |
| Runtime to controller/tools | Engine position and source positions tied to output-frame timestamps, applied revisions, bounded meter/diagnostic observations |
| Runtime to host | The same rendering entry point for offline buffers and native output; named output channels and supported format |

These boundaries allow compiler, runtime and operations/tooling work to proceed
independently once their contracts are agreed. Operations owns decoding, loading,
graph builds, processor preparation, publication and reclamation. It must not move
that work into the callback as a convenience.

Stable node/state identities let compatible processors retain playheads, filter
history and effect tails across Program changes. Incompatible replacements require
an explicit reset or transition. Prepare the new graph off the audio thread, activate
at a defined boundary, and reclaim old resources off the audio thread after they are
no longer used. Keeping state is not sufficient for click-free routing: gain steps
or changed signal paths may still require ramps, fades or overlapping Programs.
Budget that overlap and retain outgoing effects until the chosen tail policy ends.

## Nodes and state

The minimum vocabulary for eventual mix coverage is sample player, stretched player,
gain, summing mixer, EQ/filter, effects, meter and named output. These are reusable
processors, not deck-specific operations. Each original or stem is a separate player
with its own position, loop, participation and stretch state. Multiple players may
reference the same immutable asset without sharing mutable playback state.

Start with acyclic user graphs; effects may encapsulate feedback internally. A
longer-term architectural proof is primitive sample feedback with explicit
previous-state reads and next-state writes. Validation must reject algebraic cycles
with no delay. That proof needs defined sample ordering and delay semantics; merely
topologically sorting blocks does not establish it. Arbitrary user graph feedback
is not required for initial mix support.

## Three time domains

| domain | meaning |
|---|---|
| Engine output frames | Continuous rendering time at the engine rate; pausing or seeking a player does not rewind it |
| Musical beats | Musical position mapped onto output time through the tempo timeline; supplied timing authority can be local or Link |
| Source frames | Position in an asset at its source rate, with its own beat map |

Conversions must retain the source rate and beat-map rate, rather than assume they
equal the output rate. Beat maps provide source-frame/beat correspondence, including
pre-downbeat audio and tempo variation. Stretching and loops change the mapping from
output time to source position; a single global playback position cannot describe
independently moving stems. Define rounding at frame boundaries explicitly.

Atomic batches coordinate starts, seeks and loop changes across players at one
output-frame timestamp. Apply events at their offsets within an audio block, splitting
render spans when necessary, rather than quantizing every event to a callback boundary.
Validate all targets before accepting a batch so one invalid source cannot partially
move a group. Bound queue capacity and events per block; specify visible rejection
and late-command behavior before shipping instead of allowing unbounded callback work.

Commands and asynchronous preparation carry revision/cancellation identity. Stop,
replacement or a superseding operation invalidates older work so a late load or
prepared start cannot revive obsolete playback. Define ordering for commands at the
same timestamp. A controller converts musical intent into these batches and revises
pending work when timing authority changes. Observations report what the engine
actually rendered; UI prediction is only presentation.

## Resources and stretching

Begin with RAM-resident decoded assets and an explicit memory budget covering samples,
processor state, reusable buffers, loop caches and transition overlap. Reject or defer
preparation that cannot fit; disk streaming is later work. Loading, decoding and
processor preparation happen off the callback. The callback does bounded work without
blocking, allocation, file access, or surprise destruction, including last-reference
drops. Publication and retirement must preserve those constraints under cancellation.

Stretching is a major early risk. Investigate reuse of the current Signalsmith
algorithm behind a narrow processor interface before committing to a DSP rewrite or
another library. The existing Web Audio integration is not proof of a ready native
Rust integration. The interface must expose preparation requirements, input/output
latency, supported rates/channels, reposition/reset behavior and tail/drain behavior.
Measure alignment and audible transitions, including native-to-stretched changes;
schedule compensation from measured latency rather than treating preparation as free.
