//! Diagnostic executable only: the canonical study never links this allocator.
use audio_flow_harness::{
    conformance, contracts, external, model, offline, reference, results, study::*,
};
use serde::Serialize;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::{Duration, Instant},
};
struct Meter;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
fn allocated(n: usize) {
    ALLOCS.fetch_add(1, Relaxed);
    BYTES.fetch_add(n as u64, Relaxed);
    let live = LIVE.fetch_add(n as u64, Relaxed) + n as u64;
    PEAK.fetch_max(live, Relaxed);
}
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            allocated(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(l);
        if !p.is_null() {
            allocated(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        FREES.fetch_add(1, Relaxed);
        LIVE.fetch_sub(l.size() as u64, Relaxed);
        System.dealloc(p, l);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = System.realloc(p, l, n);
        if !q.is_null() {
            FREES.fetch_add(1, Relaxed);
            LIVE.fetch_sub(l.size() as u64, Relaxed);
            allocated(n);
        }
        q
    }
}
#[global_allocator]
static ALLOCATOR: Meter = Meter;
#[derive(Clone, Copy, Default, Serialize)]
struct ResourceSnapshot {
    user_cpu_ns: u64,
    system_cpu_ns: u64,
    minor_faults: u64,
    major_faults: u64,
    input_block_operations: u64,
    output_block_operations: u64,
    voluntary_context_switches: u64,
    involuntary_context_switches: u64,
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn resource_snapshot() -> Option<ResourceSnapshot> {
    // macOS/Linux rusage ABI: two timeval (sec/usec) followed by 14 longs.
    #[repr(C)]
    #[derive(Default)]
    struct Usage {
        user: [std::os::raw::c_long; 2],
        system: [std::os::raw::c_long; 2],
        counts: [std::os::raw::c_long; 14],
    }
    unsafe extern "C" {
        fn getrusage(who: std::os::raw::c_int, usage: *mut Usage) -> std::os::raw::c_int;
    }
    let mut u = Usage::default();
    if unsafe { getrusage(0, &mut u) } != 0 {
        return None;
    }
    let ns = |t: [std::os::raw::c_long; 2]| t[0] as u64 * 1_000_000_000 + t[1] as u64 * 1000;
    Some(ResourceSnapshot {
        user_cpu_ns: ns(u.user),
        system_cpu_ns: ns(u.system),
        minor_faults: u.counts[4] as u64,
        major_faults: u.counts[5] as u64,
        input_block_operations: u.counts[7] as u64,
        output_block_operations: u.counts[8] as u64,
        voluntary_context_switches: u.counts[12] as u64,
        involuntary_context_switches: u.counts[13] as u64,
    })
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn resource_snapshot() -> Option<ResourceSnapshot> {
    None
}
fn cpu_ns() -> Option<u64> {
    resource_snapshot().map(|r| r.user_cpu_ns + r.system_cpu_ns)
}
#[derive(Default, Serialize)]
struct Phase {
    calls: u64,
    wall_ns: u64,
    process_cpu_ns: Option<u64>,
    resource_deltas: Option<ResourceSnapshot>,
    allocations: u64,
    requested_bytes: u64,
    frees: u64,
    live_delta_bytes: i64,
    peak_live_bytes: u64,
}
impl Phase {
    #[inline(never)]
    fn measure<T>(&mut self, f: impl FnOnce() -> T) -> T {
        let a = ALLOCS.load(Relaxed);
        let b = BYTES.load(Relaxed);
        let d = FREES.load(Relaxed);
        let l = LIVE.load(Relaxed);
        PEAK.store(l, Relaxed);
        let cpu = resource_snapshot();
        let start = Instant::now();
        let value = f();
        self.wall_ns += start.elapsed().as_nanos() as u64;
        if let (Some(a), Some(b)) = (cpu, resource_snapshot()) {
            *self.process_cpu_ns.get_or_insert(0) +=
                (b.user_cpu_ns + b.system_cpu_ns).saturating_sub(a.user_cpu_ns + a.system_cpu_ns);
            let d = self
                .resource_deltas
                .get_or_insert_with(ResourceSnapshot::default);
            d.user_cpu_ns += b.user_cpu_ns.saturating_sub(a.user_cpu_ns);
            d.system_cpu_ns += b.system_cpu_ns.saturating_sub(a.system_cpu_ns);
            d.minor_faults += b.minor_faults.saturating_sub(a.minor_faults);
            d.major_faults += b.major_faults.saturating_sub(a.major_faults);
            d.input_block_operations += b
                .input_block_operations
                .saturating_sub(a.input_block_operations);
            d.output_block_operations += b
                .output_block_operations
                .saturating_sub(a.output_block_operations);
            d.voluntary_context_switches += b
                .voluntary_context_switches
                .saturating_sub(a.voluntary_context_switches);
            d.involuntary_context_switches += b
                .involuntary_context_switches
                .saturating_sub(a.involuntary_context_switches);
        }
        self.calls += 1;
        self.allocations += ALLOCS.load(Relaxed) - a;
        self.requested_bytes += BYTES.load(Relaxed) - b;
        self.frees += FREES.load(Relaxed) - d;
        self.live_delta_bytes += LIVE.load(Relaxed) as i64 - l as i64;
        self.peak_live_bytes = self.peak_live_bytes.max(PEAK.load(Relaxed));
        value
    }
}
#[derive(Default, Serialize)]
struct Phases {
    fixture_events_oracle: Phase,
    preparation: Phase,
    enqueue: Phase,
    render: Phase,
    capture: Phase,
    validation: Phase,
    teardown: Phase,
}
fn write_new(path: impl AsRef<std::path::Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes.as_ref())
}
#[derive(Serialize)]
struct AllocationSample {
    elapsed_ns: u64,
    process_cpu_ns: Option<u64>,
    resources: Option<ResourceSnapshot>,
    completed_trials: usize,
    allocations: u64,
    requested_bytes: u64,
    live_requested_bytes: u64,
}
fn allocation_sample(start: Instant, completed_trials: usize) -> AllocationSample {
    AllocationSample {
        elapsed_ns: start.elapsed().as_nanos() as u64,
        process_cpu_ns: cpu_ns(),
        resources: resource_snapshot(),
        completed_trials,
        allocations: ALLOCS.load(Relaxed),
        requested_bytes: BYTES.load(Relaxed),
        live_requested_bytes: LIVE.load(Relaxed),
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(6..=8).contains(&args.len()) {
        return Err(
            "usage: audio-profile RUN_DIRECTORY WORKLOAD offline|direct|fundsp REPEATS SECONDS [generic|flat-fused] [scan|counted]"
                .into(),
        );
    }
    let strategy = match args.get(6).map(String::as_str).unwrap_or("flat-fused") {
        "generic" => offline::RenderStrategy::Generic,
        "flat-fused" => offline::RenderStrategy::FlatFused,
        _ => return Err("unknown render strategy".into()),
    };
    let enqueue_strategy = match args.get(7).map(String::as_str).unwrap_or("scan") {
        "scan" => offline::EnqueueStrategy::Scan,
        "counted" => offline::EnqueueStrategy::Counted,
        _ => return Err("unknown enqueue strategy".into()),
    };
    let dir = PathBuf::from(&args[1]).canonicalize()?;
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .canonicalize()?;
    if dir.starts_with(repo) {
        return Err("profile artifacts must be outside the repository".into());
    }
    write_new(dir.join("profile-started"), b"diagnostic run")?;
    let workload = workloads()
        .into_iter()
        .find(|w| w.id == args[2])
        .ok_or("unknown workload")?;
    let candidate = match args[3].as_str() {
        "offline" => Candidate::Offline,
        "direct" => Candidate::Direct,
        "fundsp" => Candidate::Fundsp,
        _ => return Err("unknown candidate".into()),
    };
    let repeats: usize = args[4].parse()?;
    let seconds: u64 = args[5].parse()?;
    if !(1..=100000).contains(&repeats) || !(1..=60).contains(&seconds) {
        return Err("budget requires repeats 1..100000 and seconds 1..60".into());
    }
    let fixture = model::Fixture::first()?;
    let literal = contracts::verify(|p| {
        offline::RenderState::new_with_strategies(p, strategy, enqueue_strategy)
    });
    let direct = conformance::run_suite(&fixture, reference::DirectLoop::default);
    let fundsp = conformance::run_suite(&fixture, external::FundspCopy::default);
    let checks: Vec<_> = [1, 64, 257]
        .into_iter()
        .map(|b| check_with_strategies(&workload, candidate, b, strategy, enqueue_strategy))
        .collect();
    let passed = literal.iter().all(|g| g.passed)
        && direct.outcome() == conformance::Outcome::Pass
        && fundsp.outcome() == conformance::Outcome::Pass
        && checks.iter().all(|c| c.outcome == "pass");
    write_new(
        dir.join("gates.json"),
        results::json_bytes(
            &serde_json::json!({"literal":literal,"direct":direct,"fundsp":fundsp,"checks":checks,"passed":passed}),
        )?,
    )?;
    if !passed {
        return Err("correctness/capability gate excludes this profile; see gates.json".into());
    }
    write_new(dir.join("ready"), b"ready")?;
    let wait = Instant::now();
    while !dir.join("go").exists() {
        if wait.elapsed() > Duration::from_secs(30) {
            return Err("launcher handshake timeout".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut phases = Phases::default();
    let mut allocation_samples = Vec::with_capacity((seconds * 20 + 3) as usize);
    let start = Instant::now();
    let mut completed = 0;
    allocation_samples.push(allocation_sample(start, 0));
    let mut last_sample = start;
    for _ in 0..repeats {
        let (expected, batches) = phases
            .fixture_events_oracle
            .measure(|| (workload.expected(), workload.batches()));
        let (mut adapter, mut output, mut tail, mut rendered) =
            phases.preparation.measure(|| -> Result<_, String> {
                Ok((
                    Prepared::new_with_strategies(
                        &workload,
                        candidate,
                        strategy,
                        enqueue_strategy,
                    )?,
                    vec![vec![0.; 256]],
                    vec![vec![0.; workload.frames % 256]],
                    vec![0.; workload.frames],
                ))
            })?;
        phases.enqueue.measure(|| adapter.enqueue(batches))?;
        let mut offset = 0;
        while offset < workload.frames {
            let buffer = if workload.frames - offset >= 256 {
                &mut output
            } else {
                &mut tail
            };
            phases.render.measure(|| adapter.render_into(buffer))?;
            phases
                .capture
                .measure(|| rendered[offset..offset + buffer[0].len()].copy_from_slice(&buffer[0]));
            offset += buffer[0].len();
        }
        let valid = phases.validation.measure(|| {
            first_mismatch(&expected, &rendered).is_none()
                && adapter.observe() == workload.expected_state()
        });
        if !valid {
            return Err("profile output validation failed".into());
        }
        phases
            .teardown
            .measure(|| drop((adapter, output, tail, rendered, expected)));
        completed += 1;
        if last_sample.elapsed() >= Duration::from_millis(50) {
            allocation_samples.push(allocation_sample(start, completed));
            last_sample = Instant::now();
        }
        if start.elapsed() >= Duration::from_secs(seconds) {
            break;
        }
    }
    if allocation_samples.last().unwrap().completed_trials != completed {
        allocation_samples.push(allocation_sample(start, completed));
    }
    write_new(
        dir.join("allocation-samples.json"),
        results::json_bytes(&serde_json::json!({
            "schema_version":1,"requested_interval_ns":50000000,
            "scope":"diagnostic process CPU and Rust global allocator counters; real samples at completed-trial boundaries after teardown; live bytes include preallocated sampling buffer; initial and final samples retained",
            "timestamp_scope":"Instant elapsed from trial-loop start, separate from launcher sampler origin",
            "overhead":"one elapsed clock check per trial, preallocated sample storage, periodic CPU clock and atomic reads; outside individual phase timers; no transient intra-trial live-memory reconstruction",
            "samples":allocation_samples
        }))?,
    )?;
    let manifest = serde_json::json!({"schema_version":1,"kind":"instrumented-profile","render_strategy":strategy,"enqueue_strategy":enqueue_strategy,"comparative_speed_eligible":false,"workload":workload,"candidate":candidate,"completed":completed,"requested_repeats":repeats,"seconds_budget":seconds,"block":256,"warmups":0,"revision":results::command_text("git", &["rev-parse","HEAD"]),"dirty":results::command_text("git", &["status","--porcelain"]),"compiler":env!("HARNESS_RUSTC"),"build_profile":env!("HARNESS_PROFILE"),"target":env!("HARNESS_TARGET"),"opt_level":env!("HARNESS_OPT_LEVEL"),"rustflags":env!("HARNESS_CARGO_ENCODED_RUSTFLAGS"),"lock":include_str!("../../Cargo.lock"),"phases":phases});
    let mut writes = Phase::default();
    writes.measure(|| {
        write_new(
            dir.join("profile.json"),
            results::json_bytes(&manifest).unwrap(),
        )
    })?;
    write_new(
        dir.join("artifact-writes.json"),
        results::json_bytes(&writes)?,
    )?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracks_owned_allocation_lifetime() {
        let mut p = Phase::default();
        let v = p.measure(|| vec![1u8; 8192]);
        assert!(p.requested_bytes >= 8192);
        assert!(p.live_delta_bytes >= 8192);
        let mut d = Phase::default();
        d.measure(|| drop(v));
        assert!(d.frees >= 1);
        assert!(d.live_delta_bytes <= -8192);
    }
}
