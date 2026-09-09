# External reference review

Bounded primary-source review for this local experiment, 2026-09-09. This selects a
reference scope, not a production dependency or a general engine winner.

FunDSP **0.20.0** is pinned exactly, with default features disabled and only `std`
enabled. It is an intentionally fixed older release with published API documentation;
The latest release observed during that review was 0.23.0, not evaluated here. Cargo.lock pins transitive versions.
The downloaded package's Cargo.toml and LICENSE-MIT identify `MIT OR Apache-2.0`;
this effort uses the MIT option. The repository's own code is MIT. This local
evaluation performs no new publication.

Primary sources: [versioned package](https://crates.io/crates/fundsp/0.20.0),
[versioned source](https://docs.rs/crate/fundsp/0.20.0/source/),
[upstream repository](https://github.com/SamiPerttu/fundsp).
The exact registry source and license were also inspected locally after download.

| inspected boundary | finding and experiment consequence |
|---|---|
| `src/wave.rs`, `WavePlayer::new/tick/reset` | RAM Wave player advances one sample per tick, emits zeros after its exclusive end, optionally wraps to a loop point; reset returns to constructor start |
| `Wave::from_samples` | Provides mono f32 in-memory input; file decoding is unnecessary and disabled |
| `AudioNode` / `hacker32::mul` | Scalar tick and fixed gain are useful processor references; the study deliberately does not evaluate block/SIMD optimization |
| WavePlayer fields | Position is private. Any observed position in the adapter is adapter bookkeeping, not a native observable-position claim |
| chosen boundary | Scheduled starts are adapter-emulated; common workloads use WavePlayer plus native gain, with ordered summation in the adapter |
| excluded from this adapter | Arbitrary graphs, timed stop/seek changes, atomic batches and revision cancellation are capability gaps of this integration, not claims that all of FunDSP lacks them |

The reference includes all adapter work in end-to-end render timings. Choosing a
small scalar API path may disadvantage FunDSP versus its block API; the report must
state this. Firewheel remains unreviewed and deferred. Faust remains a potential
processor-unit comparison, not an equivalent engine candidate. No device libraries,
Node bindings or file decoder are intentionally enabled by this integration.
