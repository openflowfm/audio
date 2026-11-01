# Experimental native Node bridge

This is a thin napi-rs binding around `audio-flow-harness::offline`, via an
in-repository Cargo path dependency. No DSP implementation is copied. CPAL queries
actual native output capabilities; **nothing opens a stream or emits audio**.
This is a stepping stone, not Stage 2 completion or a production package.

## Build / verify

From the repository root (Node 26, Rust/Cargo, C compiler; Linux also needs ALSA
headers and pkg-config, e.g. Debian `libasound2-dev pkg-config`):

```sh
npm ci --prefix native
npm run build --prefix native
npm test --prefix native
npm run test:rust --prefix native
cargo fmt --manifest-path native/Cargo.toml -- --check
./harness/verify
```

Cargo and npm lockfiles are committed. Build uses `cargo build --locked --release`
and copies the platform library to **`native/dist/audio-flow.node`**. The portable
Node build script needs no shell-specific copy command or npm build dependencies.
Linux/ALSA is the validated platform; other CPAL platforms are not validated here.
Do not commit `target/`, `dist/`, `node_modules/` or the compiled addon.

Consumers can `require('./native')` (or require this package directory by relative
path from their application). This loads the real local addon, not a fallback.
No outside-repository file dependency is needed. See `index.d.ts` for exact types.

## Exports

- **`engineInfo()`** reports explicit experimental/offline-only status, 48 kHz,
  mono finite f32 RAM input format, and all current Rust default limit ceilings.
  Native output is unavailable; those ceilings are not realtime guarantees.
- **`listOutputDevices()`** queries the CPAL default host (ALSA on Linux), returning
  actual device names and advertised configuration ranges: channel count,
  minimum/maximum sample rate and sample format. `id` is host plus enumeration
  index: **ephemeral, not a durable identity**. Names and order may change or be
  duplicated. Top-level enumeration failures throw; name/configuration failures
  remain explicit per-device strings with null names/empty configurations as
  applicable. An empty device list means none enumerated, not simulated devices.
  Advertised capability is not evidence of successful playback or channel wiring.
- **`renderFixture({ frames = 10 } = {})`** creates fresh Assets → Graph → Program
  → RenderState, schedules starts at frames 1 and 2, and returns planar JS number
  arrays converted from native f32, named outputs, rate, frame count and final
  observation. Frames must be an integer from 0 through 4096; malformed options
  and over-budget calls throw. No files are written. This synchronous allocating
  fixture is for integration tests/preparation, **not for a device callback**.

The fixture has two independent players. `fixture.left` plays
`[.25,-.5,.75,-1]` starting at frame 1. `fixture.right` plays
`[.125,.25,.5,.75]` through gain .5 starting at frame 2. `fixture.sum` is their
ordered graph sum. These are graph output names, not physical hardware channels.
Ten frames include trailing silence; zero frames leave starts pending. Every call
starts from scratch; this is not a transport/session API.

## Rust-only simulated callback seam

`output::Callback::prepare(program, rate, format, channels, chunk, mappings)`
prepares a render state, planar scratch and explicit `(output_name, physical_index)`
routes off callback. Only 48 kHz f32 is accepted, with 1–128 device channels and
chunks within the Program block limit. Unknown names, duplicate output names,
duplicate physical destinations, out-of-range indices and empty maps reject.
Unmapped channels are zero; there is no fallback, conversion or downmix.

It returns a single-producer `Controller` and callback-owned `Callback`.
`ControlBatch::new(revision, commands)` copies at most 16 commands into inline
storage. `Controller::submit` uses a preallocated rtrb SPSC queue of 32 batches,
returning the batch on queue-full and incrementing a producer-side counter.
`Callback::process(&mut [f32])` handles at most four batches (64 commands) at the
start of a nonempty callback, validates each atomically using the original
renderer, then renders arbitrary interleaved spans in bounded scratch chunks.
Start/Stop/Seek/SetLoop are generic renderer commands, not a second transport.
Command application is callback-boundary quantized; excess batches wait for a
later callback. Partition invariance applies without newly delivered controls,
not to different command-arrival schedules. Empty callbacks are no-ops; malformed
spans are silenced/rejected without consuming controls or advancing time.

`Callback::snapshot()` is a borrowed, nonallocating authoritative renderer view:
continuous next-output frame and actual next-source frames, statuses and loops.
It is **not** a cross-thread publication API. Callback diagnostics count callbacks,
applied/rejected batches, last rejection, maximum elapsed duration and duration
exceeding the supplied span's 48 kHz deadline. These are simulated CPU duration
measurements, **not measured underruns** or proof of device timing. Stream errors
are not instrumented because no stream exists. No logging occurs in process.

## Remaining host work / safety boundary

No CPAL stream or fixture-host NAPI API is implemented. `nativeOutputAvailable`
remains false. Device selection must eventually revalidate an ephemeral ID plus
expected descriptor/configuration at open, with no default/fallback. Cross-thread
observation publication, stream-error reporting, shutdown acknowledgement,
off-callback reclamation and human hardware routing/timing acceptance remain gates.
Prepared state, scratch, Program references and queue endpoints must be destroyed
**off callback after it stops**, never by dropping them during active rendering.
That lifecycle guarantee is not established by a live host here; do not call this
seam production realtime-safe. Processing uses no mutex, filesystem, JSON or Node
calls and allocator guards verify zero allocations **and frees** on simulated
success, rejection and queue-pressure paths. OS clock latency and hardware behavior
are not established. No test opens or starts a device stream.

Streaming/decode, resampling, WING sessions, UI integration and capture remain out
of scope. See the [offline contract](../docs/offline-contract.md).
