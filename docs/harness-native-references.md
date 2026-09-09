# Native block references

The canonical candidate IDs are `Offline`, `Direct`, `Fundsp`, `FundspBlock`, and
`Oddio`. Existing `Fundsp` evidence remains the pinned 0.20.0 scalar WavePlayer plus
boxed gain adapter. `FundspBlock` is a separate 0.20.0 adapter, not a relabeling.
`Oddio` pins version 0.7.4. Ours remains fused plus scanning enqueue by default;
the counted enqueue experiment is independent.

## FunDSP block path

Each voice statically composes `An(WavePlayer) >> mul(gain)`. One boxed AudioUnit
call processes a block of at most 64 samples; prepared buffers and node allocation
are outside the timed render boundary. The adapter divides at scheduled starts and
one-shot end boundaries and sums each voice into output in declared order. Gain and
player rendering are native FunDSP work. Position reporting and starts are adapter
bookkeeping. The pinned WavePlayer has no specialized `process`: AudioNode's default
calls native `tick` internally. Thus block processing removes the scalar adapter's
boxed per-sample gain call but does not establish that sample playback uses SIMD.

[FunDSP processing documentation](https://github.com/SamiPerttu/fundsp#processing)
recommends blocks to reduce call/setup overhead; the pinned source, not current
README version examples, defines the measured implementation.

## Oddio native path

Loop frames are prepared from the exact half-open source region `[1,256)`, then
played by native `Cycle`. One-shot data uses native `FramesSignal`. Both use native
`Gain` initialized through `set_amplitude_ratio`, avoiding FixedGain's decibel
conversion and changing gain smoothing. No gain updates are submitted. Native
`Mixer` mixes the signals through `oddio::run` at 48000 Hz. A scheduling wrapper
writes leading silence then forwards the remaining slice to the native signal;
it neither generates playback samples nor performs gain/mix math.

The pinned Mixer visits reverse insertion order. Voices are inserted in reverse to
preserve declared sum order in these fixed-loop workloads. The one-shot workload
contains one voice. General multi-voice removal/reinsertion ordering is outside
this adapter's claimed scope. Position/status observations are adapter bookkeeping,
not an assertion that Oddio exposes the exact contract's native state API.

[Oddio APIs](https://docs.rs/oddio/0.7.4/oddio/) use interpolation. Its FramesSignal
stores elapsed time as f64 but increments it using an f32 interval. At 48000 Hz,
partitioned one-shot rendering differs bitwise from exact-copy output; a literal
one-frame partition leaves a small nonzero value where zero is required. This is
retained as a failed gate, not rounded away. Oddio Once is explicitly refused at
admission as a native capability limitation; unrestricted diagnostic checks rerun
the native path and retain their failed PCM under `native_diagnostics`. The
manifest names the limitation and evidence. The seeded one-shot likewise fails
required partitions and receives no timings. Native Cycle uses sample-position
increments and passes the existing seeded loop workloads and tested partitions.
This is a bounded exact-equivalence result, not a claim about every Oddio signal.

## Gates and timing

Independent native literal gates appear under `fundsp_block` and `oddio` in study
conformance and profiler gates. IDs `native-Once` and `native-Loop` scope the gates to
workload mode. The study manifest explicitly lists `native_gate_requirements`
records (`candidate`, `workload`, `gate_ids`); profile gates record
`required_native_gate_ids`. Every matching literal partition and every required workload check
must pass before that candidate/workload can be timed. Failed PCM/state checks
retain mismatch location and sample bits. Burst stop/seek workloads remain explicit
capability gaps for both new adapters. No approximate tolerance is introduced.

Native tests include literal PCM/state, the seeded workloads at block sizes
1, 7, 64, 257, and 4096, submission rejection atomicity, zero-frame no-op, and no
allocation/free during prepared 64-voice loop rendering. The Oddio one-shot test
asserts the known native mismatch rather than weakening the oracle.

All five candidates rotate in the same binary per workload/round, with three
warmups and twelve retained trials per eligible candidate. The timed scope remains
enqueue + prepared rendering + identical capture; fixture generation, preparation,
and exact validation remain separate. Native mixer setup is preparation; scheduling
submission is enqueue. Direct remains unchanged. Profiles use CLI candidates
`fundsp-block` and `oddio` and remain separate diagnostic evidence.
