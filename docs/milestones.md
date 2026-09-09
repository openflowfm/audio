# Milestones

These are incremental proofs for the reusable engine. Stage 1 has a local offline
MVP and [comparison harness](harness-decisions.md); the remaining stages are future
work. The offline module provides the reusable rendering implementation, not a
separate DSP copy of a device engine. Native output and production integration have
not been implemented or authorized by this local evaluation.

| stage | scope | evidence required before proceeding |
|---|---|---|
| 1. Offline core | Two sample players, gains and summing; atomic timed starts, seeks and loops; RAM assets | Render known signals and check alignment, exact expected duration, loop boundaries and deterministic output under the same configuration; verify event offsets within blocks and stale-command cancellation |
| 2. Native output | Connect the same runtime to a native audio device | Playback continues during an intentional UI stall; measure callback work and underruns, and verify preparation/reclamation stay off the callback |
| 3. One deck | Original/stems switching, independent loops, Cue and smoothing through an application controller | Preserve source positions and participation, checkpoint restoration and cancellation; inspect/audition start, seek, loop and group-switch transitions for discontinuities |
| 4. One synced deck | Beat maps and stretch, following a supplied musical timeline | Measure source/output alignment, preparation and processing latency, tempo changes and native/stretched transitions; preserve independent source offsets across sample rates |
| 5. Current mix coverage | Four decks; channel/master processing, both effect returns and retained tails, master/Phones, local timing authority and Link | Exercise the current Play behavior specification, including loop-only Slip, queued-operation cancellation and replacement; measure worst-case callback and memory costs, and validate real hardware routing and Link peers |

Stage 1 is exercised by independent literal/fault tests, seeded partitions, explicit
resource/rejection checks and offline measurements. It has no realtime proof.
Stage 2 must establish that native rendering is
independent of the UI. Neither is a replacement for today's mix engine. Stage 5 is the
parity gate, governed by [current Play behavior](../../../better-session-view/mix/docs/play-view.md) and its
[behavior specification](../../../better-session-view/mix/docs/behavior-specification.md), not just by a count
of implemented nodes. Separate Phones output and external Link behavior require actual
device/peer validation; offline success alone cannot establish them.

Investigate the stretch interface early enough to expose integration constraints before
stage 4, without making it a prerequisite for the offline sample-player proof. Keep
native code generation, streaming and primitive sample-feedback experiments beyond
these initial gates. They should test or extend the reusable architecture when needed,
not delay basic rendering until a complete production system exists.
