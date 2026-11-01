# Bounded native bridge validation

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
