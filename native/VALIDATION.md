# Bounded native bridge validation

## Native output seam checkpoint (wing/native-output)

Implemented renderer borrowed immediate control/observation and prepared-buffer
prefix rendering, plus Rust-only simulated callback adapter. No device streams
were built or started. CPAL fixture host and NAPI transport remain deferred.

- `cargo test --manifest-path native/Cargo.toml --locked`: 4 unit tests pass,
  including three callback adapter tests; doctests pass.
- Literal four-physical-channel f32 routing (two mapped, two silent), loop endpoint,
  chunk sizes 1/3/4 and uneven callback partitions produce identical exact samples.
- Atomic invalid-seek rejection, pause/seek readback, continuous output time,
  duplicate/invalid routes, unsupported formats/rates, malformed/empty spans,
  queue-full and four-batches-per-callback ceiling are tested.
- Thread-local global allocator guards count alloc/alloc_zeroed/realloc/dealloc:
  zero calls in callback success/loop/observation and rejection/control/queue-pressure
  paths; preparation and teardown excluded.
- Harness allocation suite: 5 tests pass; new immediate seam suite: 2 tests pass.
- Original `./harness/verify --output-root /tmp/audio-output-verify`: passed all
  checks; report `/tmp/audio-output-verify/audio-run-z6fiijqm/report.html`.
- `npm test --prefix native`: release addon rebuild and all 4 original Node tests
  passed. Device enumeration only; no playback.
- Initial integration-test arrangement hit unresolved NAPI symbols outside Node;
  moved callback tests into the library unit-test target (same adapter code), then
  the locked Rust suite passed. No production NAPI feature workaround was added.

No hardware, cross-thread observation publication, stream shutdown/reclamation or
real deadline/underrun acceptance is claimed. Exact API and limitations are in
[README](README.md).

## Prior bridge baseline

Local Linux run, 2026-11-01, Node **v26.13.0**, npm **11.20.0**, ALSA
pkg-config **1.2.16.1**. Implemented by Codex on `wing/native-bridge`.

- `npm ci --prefix native`: no npm dependencies; package lock pinned.
- `npm test --prefix native`: locked release addon build, **4 Node tests passed**.
  Tests load the real `.node`, assert exact three-channel f32-derived PCM, output
  names/order, gain/sum, starts on different frames, final player states,
  determinism, 0/1/2/5/10/4096-frame boundaries, and malformed/oversized requests.
- `npm run test:rust --prefix native`: **1 Rust test passed**, plus doctests.
  Literal channels/names, zero frames and input limits are asserted in Rust too.
- `cargo fmt --manifest-path native/Cargo.toml -- --check`: passed.
- `git diff --check`: passed.
- Original `./harness/verify --output-root <existing-external-directory>`:
  **passed** formatting, build, Rust tests, doctests and Python tests. No benchmark
  requested. A first attempt with a nonexistent output root failed before checks;
  creating the directory and rerunning succeeded.

Native enumeration used CPAL's ALSA host, returning 13 descriptors locally,
including jack, pipewire, pulse, default, four HDMI endpoints and five Generic_1
endpoints. Returned configuration counts ranged from 1 to 256. These include ALSA
virtual/plugin endpoints, not necessarily 13 physical devices. Names and counts
are environment-specific and are not test oracles. No returned descriptor had a
name/configuration error on this run. ALSA emitted discovery warnings for
unavailable dmix/OSS/channel maps; these were not suppressed. CPAL/ALSA can probe
PCM capabilities during enumeration, but this code never builds or starts an
audio stream and never submits samples to a device.

No native playback, callback timing, hardware routing or UI integration was
validated. Stage 2 remains incomplete. See README for the remaining host work.
