# Intention

audio[flow] is planned as reusable audio rendering for open[flow]. The longer-term
direction is a programmable music production and performance system: reusable graph
devices, customizable interfaces, songs and parts, improvisation, and flexible
scheduling. The library should make those applications possible without encoding one
application's arrangement or interface into its rendering model. Recreating Ableton
is not its specification, and a mix-specific deck engine is not its boundary.

mix[flow] is the first consumer because it already supplies concrete playback and
timing requirements. Its current implementation remains in `mix/`; nothing in these
docs describes a completed migration. [Play](../../../better-session-view/mix/docs/play-view.md) governs
that implementation, and [playback](../../../better-session-view/mix/docs/playback.md) describes its shared
audio primitives. These are requirements to preserve as coverage grows:

| current mix behavior | implication for reusable rendering |
|---|---|
| Four decks, each with an original and up to six stems | Independent player instances; immutable sample assets may be shared |
| Full/Stems switching; heard groups continue beneath gain ramps | Participation and gain are distinct from playback position; switching requires coordinated alignment and smoothing |
| Independently moving stems, source loops, Cue checkpoints and loop-only Slip | Each player needs its own position, loop, participation and stretch state |
| Shared timing, beat maps, Sync and Link | Explicit clock mappings and coordinated scheduled operations |
| Channel/master EQ and filter, two shared effect returns with retained tails | Composable processing and processor lifetimes beyond send-input bypass or replacement |
| Master and Phones, with distinct monitoring taps | Named outputs and explicit routing; the host validates hardware capability |

The current engine uses Web Audio and Signalsmith worklets. Its UI reads the audio
clock; audio rendering is not driven by display refresh. Existing control policies
and their scheduling dependencies should be evaluated separately from that fact.

The reusable runtime owns rendering, player state, timed operations and observable
position. Applications own what Cue means, leader selection, song sections, Slip
decisions, source-group switching policy and Link authority. They translate those
decisions into engine commands. Eventually that controller should run headlessly,
without depending on React responsiveness to keep future musical actions scheduled.
The UI presents state and sends intent; it never clocks rendering.

The first useful core is smaller than current mix parity. Native code generation,
disk streaming and arbitrary user graph feedback are not entry requirements. The
[milestones](milestones.md) separate a working rendering foundation from full mix
coverage; they are validation gates, not estimates or claims of shipped behavior.
