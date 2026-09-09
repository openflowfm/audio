use crate::{
    conformance::{Outcome, Report},
    model::SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingSource {
    Synthetic,
    Monotonic,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trial {
    pub schema_version: u32,
    pub trial: u64,
    pub frames: u64,
    pub elapsed_ns: u64,
    pub source: TimingSource,
    pub scope: String,
    pub workload: String,
    pub adapter: String,
    pub external_partitions: Vec<u64>,
}
impl Trial {
    pub fn synthetic(trial: u64, frames: u64, elapsed_ns: u64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            trial,
            frames,
            elapsed_ns,
            source: TimingSource::Synthetic,
            scope: "synthetic-aggregation-test".into(),
            workload: "synthetic".into(),
            adapter: "none".into(),
            external_partitions: vec![frames],
        }
    }
    pub fn monotonic(trial: u64, elapsed_ns: u64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            trial,
            frames: 10,
            elapsed_ns,
            source: TimingSource::Monotonic,
            scope: "start-dispatch-and-render-including-output-allocation".into(),
            workload: "first-copy-v1".into(),
            adapter: "direct-loop".into(),
            external_partitions: vec![10],
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Summary {
    pub schema_version: u32,
    pub conformance: Outcome,
    pub comparative_speed_eligible: bool,
    pub trials: u64,
    pub excluded_trials: u64,
    pub total_ns: u64,
    pub total_frames: u64,
    pub min_ns: Option<u64>,
    pub max_ns: Option<u64>,
    pub source: Option<TimingSource>,
    pub raw_timings: String,
    pub limitation: String,
}
pub fn summarize(outcome: Outcome, trials: &[Trial]) -> Result<Summary, String> {
    if outcome != Outcome::Pass && !trials.is_empty() {
        return Err("nonconforming trials cannot enter speed aggregation".into());
    }
    let mut summary = Summary { schema_version: SCHEMA_VERSION, conformance: outcome,
        comparative_speed_eligible: false, trials: 0, excluded_trials: 0, total_ns: 0, total_frames: 0,
        min_ns: None, max_ns: None, source: trials.first().map(|t| t.source), raw_timings: "timings.jsonl".into(),
        limitation: "Timing smoke only: no candidate comparison, confidence interval, memory/preparation or realtime evidence. Min/max describe observed variability, not bounds.".into() };
    for (index, trial) in trials.iter().enumerate() {
        if trial.schema_version != SCHEMA_VERSION
            || trial.trial != index as u64
            || trial.frames == 0
            || trial.external_partitions.is_empty()
            || trial.external_partitions.contains(&0)
            || trial
                .external_partitions
                .iter()
                .try_fold(0u64, |sum, n| sum.checked_add(*n))
                != Some(trial.frames)
            || trials.first().is_some_and(|first| {
                first.source != trial.source
                    || first.scope != trial.scope
                    || first.workload != trial.workload
                    || first.adapter != trial.adapter
                    || first.external_partitions != trial.external_partitions
            })
        {
            return Err("invalid, unordered or incomparable trial set".into());
        }
        summary.total_ns = summary
            .total_ns
            .checked_add(trial.elapsed_ns)
            .ok_or("timing total overflow")?;
        summary.total_frames = summary
            .total_frames
            .checked_add(trial.frames)
            .ok_or("frame total overflow")?;
        summary.min_ns = Some(
            summary
                .min_ns
                .map_or(trial.elapsed_ns, |n| n.min(trial.elapsed_ns)),
        );
        summary.max_ns = Some(
            summary
                .max_ns
                .map_or(trial.elapsed_ns, |n| n.max(trial.elapsed_ns)),
        );
        summary.trials += 1;
    }
    Ok(summary)
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn checked_root(root: &Path) -> io::Result<PathBuf> {
    let root = root.canonicalize()?; // Existing root only: resolves symlinks before any mutation.
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()?;
    if root.starts_with(source) {
        return Err(invalid(
            "run output must be outside the repository source tree",
        ));
    }
    Ok(root)
}
pub fn create_run_dir(root: &Path) -> io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = checked_root(root)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| invalid(e.to_string()))?
        .as_nanos();
    for _ in 0..1000 {
        let dir = root.join(format!(
            "audio-harness-{stamp}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a unique run directory",
    ))
}
pub fn command_text(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
}
pub fn json_bytes(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}
/// Complete runs have a valid complete.json marker binding every artifact by size and checksum.
/// Failed writes leave evidence but never a completion marker. Files never overwrite.
pub fn write_run(
    dir: &Path,
    report: &Report,
    trials: &[Trial],
    save_failed_pcm: bool,
) -> io::Result<()> {
    checked_root(dir)?;
    let mut summary = if report.outcome() == Outcome::Pass {
        summarize(report.outcome(), trials).map_err(invalid)?
    } else {
        // Preserve raw failure evidence but exclude it from every derived timing metric.
        summarize(Outcome::Pass, trials).map_err(invalid)?;
        let mut summary = summarize(report.outcome(), &[]).map_err(invalid)?;
        summary.excluded_trials = trials.len() as u64;
        summary
    };
    summary.comparative_speed_eligible = false; // First pass has no external comparison.
    let git_status = command_text("git", &["status", "--porcelain"]);
    let source = trials.first().map(|t| t.source);
    let manifest = serde_json::json!({
        "schema_version": SCHEMA_VERSION, "harness_version": env!("CARGO_PKG_VERSION"),
        "source_revision_at_run": command_text("git", &["rev-parse","HEAD"]),
        "source_dirty_at_run": git_status.as_ref().map(|s| !s.is_empty()),
        "source_status_at_run": git_status,
        "compiler": env!("HARNESS_RUSTC"), "profile": env!("HARNESS_PROFILE"),
        "opt_level": env!("HARNESS_OPT_LEVEL"), "target": env!("HARNESS_TARGET"),
        "encoded_rustflags": env!("HARNESS_CARGO_ENCODED_RUSTFLAGS"),
        "dependency_lock": include_str!("../Cargo.lock"),
        "hardware": null, "os_family": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "os_details": command_text("uname", &["-sr"]),
        "workload": report.workload, "adapter": report.adapter, "scheduling": report.scheduling,
        "sample_format": "f32", "sample_rate": 48000, "channels": 1, "quality": "exact-copy-no-smoothing",
        "seed": null, "seed_reason": "literal deterministic fixture; no random input",
        "conformance_partitions": [[10],[1,1,1,1,1,1,1,1,1,1],[3,2,5]],
        "timing_source": source, "warmup": if source == Some(TimingSource::Monotonic) { 3 } else { 0 },
        "repeats": trials.len(), "trial_order": "sequential; one adapter",
        "timing_scope": trials.first().map(|t| &t.scope),
        "timing_external_partitions": trials.first().map(|t| &t.external_partitions),
        "metadata_limitations": "Hardware model and preparation/memory instrumentation unavailable. Revision/dirty state describe checkout at run, not a reproducible dirty-source snapshot."
    });
    let mut raw = Vec::new();
    for trial in trials {
        serde_json::to_writer(&mut raw, trial).map_err(io::Error::other)?;
        raw.push(b'\n');
    }
    let mut outputs = vec![
        ("manifest.json".to_owned(), json_bytes(&manifest)?),
        ("conformance.json".to_owned(), json_bytes(report)?),
        ("timings.jsonl".to_owned(), raw),
    ];
    if save_failed_pcm {
        for (index, case) in report
            .cases
            .iter()
            .enumerate()
            .filter(|(_, c)| c.outcome != Outcome::Pass)
        {
            let mut pcm = Vec::new();
            let mut blocks = Vec::new();
            for block in &case.blocks {
                if let Some(bits) = &block.sample_bits {
                    blocks.push(serde_json::json!({"output_frame":block.output_frame,"requested_frames":block.requested_frames,"actual_samples":block.actual_samples}));
                    for sample in bits {
                        pcm.extend_from_slice(&sample.to_le_bytes());
                    }
                }
            }
            if !blocks.is_empty() {
                let filename = format!("failed-{index}.f32le");
                outputs.push((filename.clone(), pcm));
                outputs.push((format!("failed-{index}.json"),json_bytes(&serde_json::json!({
                    "schema_version":SCHEMA_VERSION,"case":case.id,"file":filename,"format":"f32le",
                    "sample_rate":48000,"channels":1,"blocks":blocks
                }))?));
            }
        }
    }
    outputs.push(("summary.json".to_owned(), json_bytes(&summary)?));
    write_artifacts(dir, outputs)
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
#[derive(Serialize, Deserialize)]
struct Completion {
    schema_version: u32,
    artifacts: Vec<(String, u64, u64)>,
}
pub fn write_artifacts(dir: &Path, outputs: Vec<(String, Vec<u8>)>) -> io::Result<()> {
    checked_root(dir)?;
    if fs::read_dir(dir)?.next().is_some() {
        return Err(invalid("run directory is not empty; refusing overwrite"));
    }
    let mut marker = Completion {
        schema_version: SCHEMA_VERSION,
        artifacts: Vec::new(),
    };
    for (name, bytes) in outputs {
        if Path::new(&name).components().count() != 1 || name == "complete.json" {
            return Err(invalid("invalid artifact name"));
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&name))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        marker
            .artifacts
            .push((name, bytes.len() as u64, checksum(&bytes)));
    }
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("summary.json"))?).map_err(io::Error::other)?;
    if summary["schema_version"] != SCHEMA_VERSION {
        return Err(invalid("invalid summary schema"));
    }
    let bytes = json_bytes(&marker)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join("complete.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}
pub fn is_complete(dir: &Path) -> bool {
    let Ok(bytes) = fs::read(dir.join("complete.json")) else {
        return false;
    };
    let Ok(marker) = serde_json::from_slice::<Completion>(&bytes) else {
        return false;
    };
    marker.schema_version == SCHEMA_VERSION
        && fs::read(dir.join("summary.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|summary| summary["schema_version"] == SCHEMA_VERSION)
        && !marker.artifacts.is_empty()
        && marker
            .artifacts
            .iter()
            .any(|(name, _, _)| name == "summary.json")
        && marker.artifacts.iter().all(|(name, size, hash)| {
            Path::new(name).components().count() == 1
                && fs::read(dir.join(name))
                    .is_ok_and(|bytes| bytes.len() as u64 == *size && checksum(&bytes) == *hash)
        })
}
