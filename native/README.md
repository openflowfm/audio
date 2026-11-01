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

## Remaining host work

No selected-device management, channel-map application, stream lifecycle, format
conversion/resampling, callback adapter, deadline/underrun instrumentation,
UI-stall proof, off-callback publication/reclamation, or hardware acceptance is
implemented. Streaming/decode, WING sessions, seek/loop application controls and
capture also remain future work. The existing [offline contract](../docs/offline-contract.md)
is unchanged. Do not infer realtime or host-playback support from these tests.
