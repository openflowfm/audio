//! Fixed-budget offline experiment. Oracles use closed-form source positions, not a renderer.
use crate::{
    offline::*,
    results::{self, create_run_dir, json_bytes, write_artifacts},
};
use fundsp::{
    audionode::AudioNode,
    audiounit::AudioUnit,
    hacker32::mul,
    wave::{Wave, WavePlayer},
};
use serde::{Deserialize, Serialize};
use std::{
    hint::black_box,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Once,
    Loop,
    Bursts,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workload {
    pub schema_version: u32,
    pub id: String,
    pub seed: u64,
    pub voices: usize,
    pub frames: usize,
    pub period: usize,
    pub mode: Mode,
    pub samples: Vec<f32>,
}
impl Workload {
    pub fn new(
        id: &str,
        voices: usize,
        frames: usize,
        mode: Mode,
        period: usize,
        seed: u64,
    ) -> Self {
        let mut random = seed;
        let samples = (0..257)
            .map(|_| {
                random = random
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (((random >> 32) % 17) as i32 - 8) as f32 / 16.
            })
            .collect();
        Self {
            schema_version: 1,
            id: id.into(),
            seed,
            voices,
            frames,
            mode,
            period,
            samples,
        }
    }
    pub fn start(&self, voice: usize) -> usize {
        3 + voice % 11
    }
    pub fn gain(&self, voice: usize) -> f32 {
        [0.5, 0.25, 0.125, 0.0625][voice % 4]
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.voices == 0
            || self.voices > 64
            || self.frames < 16
            || self.frames > 96000
            || self.samples.len() != 257
            || self.samples.iter().any(|x| !x.is_finite())
            || self.period < 16
        {
            return Err("workload exceeds study budgets".into());
        }
        Ok(())
    }
    pub fn graph(&self) -> Graph {
        let mut nodes = vec![];
        let mut gains = vec![];
        for voice in 0..self.voices {
            let input = nodes.len();
            nodes.push(Node::Player { asset: 0 });
            gains.push(nodes.len());
            nodes.push(Node::Gain {
                input,
                gain: self.gain(voice),
            });
        }
        let sum = nodes.len();
        nodes.push(Node::Sum { inputs: gains });
        Graph {
            nodes,
            outputs: vec![("main".into(), sum)],
        }
    }
    pub fn batches(&self) -> Vec<Batch> {
        let mut batches = vec![];
        for voice in 0..self.voices {
            if self.mode != Mode::Once {
                batches.push(Batch {
                    frame: 0,
                    revision: 0,
                    commands: vec![Command::SetLoop(voice, Some((1, 256)))],
                });
            }
            if self.mode == Mode::Bursts {
                let mut epoch = 0;
                let mut at = self.start(voice);
                while at < self.frames {
                    batches.push(Batch {
                        frame: at as u64,
                        revision: 0,
                        commands: vec![
                            Command::Seek(voice, 1 + (epoch * 7 + voice * 3) % 255),
                            Command::Start(voice),
                        ],
                    });
                    if at + 7 < self.frames {
                        batches.push(Batch {
                            frame: (at + 7) as u64,
                            revision: 0,
                            commands: vec![Command::Stop(voice)],
                        });
                    }
                    epoch += 1;
                    at += self.period;
                }
            } else {
                batches.push(Batch {
                    frame: self.start(voice) as u64,
                    revision: 0,
                    commands: vec![Command::Start(voice)],
                });
            }
        }
        batches.sort_by_key(|b| b.frame);
        batches
    }
    /// Independent analytic oracle. No command queues, Graph, Program or adapter calls.
    pub fn expected(&self) -> Vec<f32> {
        (0..self.frames)
            .map(|frame| {
                let mut sum = 0.;
                for voice in 0..self.voices {
                    let start = self.start(voice);
                    if frame < start {
                        continue;
                    }
                    let elapsed = frame - start;
                    let position = match self.mode {
                        Mode::Once => {
                            if elapsed >= self.samples.len() {
                                continue;
                            }
                            elapsed
                        }
                        Mode::Loop => 1 + elapsed % 255,
                        Mode::Bursts => {
                            let phase = elapsed % self.period;
                            if phase >= 7 {
                                continue;
                            }
                            1 + ((elapsed / self.period * 7 + voice * 3) % 255 + phase) % 255
                        }
                    };
                    sum += self.samples[position] * self.gain(voice);
                }
                sum
            })
            .collect()
    }
    pub fn expected_state(&self) -> Observation {
        let players = (0..self.voices)
            .map(|voice| {
                let elapsed = self.frames.saturating_sub(self.start(voice));
                let (position, status) = if elapsed == 0 {
                    (0, Playback::Stopped)
                } else {
                    match self.mode {
                        Mode::Once => (
                            elapsed.min(257),
                            if elapsed >= 257 {
                                Playback::Ended
                            } else {
                                Playback::Playing
                            },
                        ),
                        Mode::Loop => (1 + elapsed % 255, Playback::Playing),
                        Mode::Bursts => {
                            let epoch = (elapsed - 1) / self.period;
                            let age = elapsed - epoch * self.period;
                            let moved = age.min(7);
                            (
                                1 + ((epoch * 7 + voice * 3) % 255 + moved) % 255,
                                if age > 7 {
                                    Playback::Stopped
                                } else {
                                    Playback::Playing
                                },
                            )
                        }
                    }
                };
                PlayerState {
                    position,
                    status,
                    loop_region: if self.mode == Mode::Once {
                        None
                    } else {
                        Some((1, 256))
                    },
                }
            })
            .collect();
        Observation {
            output_frame: self.frames as u64,
            revision: 0,
            pending_batches: 0,
            players,
        }
    }
}
pub fn workloads() -> Vec<Workload> {
    vec![
        Workload::new("one-shot", 1, 1024, Mode::Once, 128, 11),
        Workload::new("loop-1", 1, 8192, Mode::Loop, 128, 12),
        Workload::new("loop-8", 8, 8192, Mode::Loop, 128, 13),
        Workload::new("loop-32", 32, 48000, Mode::Loop, 128, 14),
        Workload::new("loop-64", 64, 48000, Mode::Loop, 128, 15),
        Workload::new("bursts-8", 8, 4096, Mode::Bursts, 128, 16),
        Workload::new("bursts-16-dense", 16, 4096, Mode::Bursts, 64, 17),
        Workload::new("long-loop-8", 8, 96000, Mode::Loop, 128, 18),
    ]
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Candidate {
    Offline,
    Direct,
    Fundsp,
}
impl Candidate {
    pub fn name(self) -> &'static str {
        match self {
            Self::Offline => "offline-mvp",
            Self::Direct => "direct-loop",
            Self::Fundsp => "fundsp-0.20.0",
        }
    }
    pub fn scope(self) -> &'static str {
        match self {
            Self::Offline => "native graph, command batches and observable state",
            Self::Direct => "specialized flat player/gain/sum; adapter scheduled starts",
            Self::Fundsp => {
                "native WavePlayer + gain; adapter scheduled starts, position tracking and sum"
            }
        }
    }
}
struct Voice {
    position: usize,
    status: Playback,
    start: usize,
    gain: f32,
    wave: Option<WavePlayer>,
    gain_unit: Option<Box<dyn AudioUnit>>,
}
pub struct Bank {
    workload: Workload,
    frame: usize,
    voices: Vec<Voice>,
}
pub enum Prepared {
    Offline(RenderState),
    Bank(Bank),
}
impl Prepared {
    pub fn new(workload: &Workload, candidate: Candidate) -> Result<Self, String> {
        workload.validate()?;
        if candidate == Candidate::Offline {
            let mut assets = Assets::new(Limits::default()).map_err(|e| format!("{e:?}"))?;
            assets
                .load(&workload.samples, 48000)
                .map_err(|e| format!("{e:?}"))?;
            let program = workload
                .graph()
                .compile(&assets, Limits::default())
                .map_err(|e| format!("{e:?}"))?;
            return Ok(Self::Offline(RenderState::new(Arc::new(program))));
        }
        if workload.mode == Mode::Bursts {
            return Err("capability gap: timed stop/seek batches are not implemented by this reference adapter".into());
        }
        let wave = if candidate == Candidate::Fundsp {
            Some(Arc::new(Wave::from_samples(48000., &workload.samples)))
        } else {
            None
        };
        let voices = (0..workload.voices)
            .map(|voice| Voice {
                position: 0,
                status: Playback::Stopped,
                start: usize::MAX,
                gain: workload.gain(voice),
                wave: if candidate == Candidate::Fundsp {
                    Some(WavePlayer::new(
                        wave.as_ref().unwrap(),
                        0,
                        if workload.mode == Mode::Loop { 1 } else { 0 },
                        if workload.mode == Mode::Loop {
                            256
                        } else {
                            257
                        },
                        if workload.mode == Mode::Loop {
                            Some(1)
                        } else {
                            None
                        },
                    ))
                } else {
                    None
                },
                gain_unit: if candidate == Candidate::Fundsp {
                    Some(Box::new(mul(workload.gain(voice))) as Box<dyn AudioUnit>)
                } else {
                    None
                },
            })
            .collect();
        Ok(Self::Bank(Bank {
            workload: workload.clone(),
            frame: 0,
            voices,
        }))
    }
    pub fn enqueue(&mut self, batches: Vec<Batch>) -> Result<(), String> {
        match self {
            Self::Offline(state) => {
                for batch in batches {
                    state.enqueue(batch).map_err(|e| format!("{e:?}"))?;
                }
            }
            Self::Bank(bank) => {
                for batch in &batches {
                    for command in &batch.commands {
                        match command {
                        Command::Start(voice) if *voice < bank.voices.len() && batch.frame >= bank.frame as u64 && batch.revision == 0 => {},
                        Command::SetLoop(voice,Some((1,256))) if *voice < bank.voices.len() && bank.workload.mode == Mode::Loop && batch.frame == 0 && batch.revision == 0 => {},
                        _ => return Err("capability gap: this reference supports fixed loops and one scheduled start per player".into())
                    }
                    }
                }
                for batch in &batches {
                    for command in &batch.commands {
                        if let Command::Start(voice) = command {
                            bank.voices[*voice].start = batch.frame as usize;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn render_into(&mut self, output: &mut [Vec<f32>]) -> Result<(), String> {
        match self {
            Self::Offline(state) => state.render_into(output).map_err(|e| format!("{e:?}")),
            Self::Bank(bank) => {
                if output.len() != 1 || output[0].len() > 4096 {
                    return Err("invalid reference buffer".into());
                }
                for sample in &mut output[0] {
                    let mut sum = 0.;
                    for voice in &mut bank.voices {
                        if bank.frame == voice.start {
                            voice.status = Playback::Playing;
                            if bank.workload.mode == Mode::Loop {
                                voice.position = 1;
                            }
                        }
                        if voice.status == Playback::Playing {
                            let value = if let Some(player) = &mut voice.wave {
                                player.get_mono()
                            } else {
                                bank.workload.samples[voice.position]
                            };
                            sum += if let Some(gain) = &mut voice.gain_unit {
                                gain.filter_mono(value)
                            } else {
                                value * voice.gain
                            };
                            voice.position += 1;
                            if bank.workload.mode == Mode::Loop && voice.position == 256 {
                                voice.position = 1;
                            } else if voice.position == 257 {
                                voice.status = Playback::Ended;
                            }
                        }
                    }
                    *sample = sum;
                    bank.frame += 1;
                }
                Ok(())
            }
        }
    }
    pub fn observe(&self) -> Observation {
        match self {
            Self::Offline(state) => state.observe(),
            Self::Bank(bank) => Observation {
                output_frame: bank.frame as u64,
                revision: 0,
                pending_batches: 0,
                players: bank
                    .voices
                    .iter()
                    .map(|v| PlayerState {
                        position: v.position,
                        status: v.status,
                        loop_region: if bank.workload.mode == Mode::Loop {
                            Some((1, 256))
                        } else {
                            None
                        },
                    })
                    .collect(),
            },
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Check {
    pub workload: String,
    pub candidate: Candidate,
    pub block: usize,
    pub outcome: String,
    pub detail: Option<String>,
    pub first_mismatch: Option<usize>,
    pub expected_samples: usize,
    pub actual_samples: usize,
    pub expected_state: Observation,
    pub actual_state: Option<Observation>,
    pub failed_sample_bits: Option<Vec<u32>>,
}
pub fn check(workload: &Workload, candidate: Candidate, block: usize) -> Check {
    let mut result = Check {
        workload: workload.id.clone(),
        candidate,
        block,
        outcome: "pass".into(),
        detail: None,
        first_mismatch: None,
        expected_samples: workload.frames,
        actual_samples: 0,
        expected_state: Observation {
            output_frame: 0,
            revision: 0,
            pending_batches: 0,
            players: vec![],
        },
        actual_state: None,
        failed_sample_bits: None,
    };
    if let Err(error) = workload.validate() {
        result.outcome = "fail".into();
        result.detail = Some(error);
        return result;
    }
    result.expected_state = workload.expected_state();
    if block == 0 || block > 4096 {
        result.outcome = "fail".into();
        result.detail = Some("invalid external block size".into());
        return result;
    }
    let mut adapter = match Prepared::new(workload, candidate) {
        Ok(adapter) => adapter,
        Err(error) => {
            result.outcome = if error.starts_with("capability gap") {
                "capability_gap"
            } else {
                "fail"
            }
            .into();
            result.detail = Some(error);
            return result;
        }
    };
    if let Err(error) = adapter.enqueue(workload.batches()) {
        result.outcome = "fail".into();
        result.detail = Some(error);
        return result;
    }
    let mut actual = vec![];
    while actual.len() < workload.frames {
        let n = block.min(workload.frames - actual.len());
        let mut output = vec![vec![0.; n]];
        if let Err(error) = adapter.render_into(&mut output) {
            result.outcome = "fail".into();
            result.detail = Some(error);
            break;
        }
        actual.extend_from_slice(&output[0]);
    }
    result.actual_samples = actual.len();
    result.actual_state = Some(adapter.observe());
    result.first_mismatch = first_mismatch(&workload.expected(), &actual);
    if result.first_mismatch.is_some()
        || result.actual_state.as_ref() != Some(&result.expected_state)
    {
        result.outcome = "fail".into();
        result.detail =
            Some("PCM or observable state differs from independent analytic oracle".into());
    }
    if result.outcome == "fail" {
        result.failed_sample_bits = Some(actual.iter().map(|s| s.to_bits()).collect());
    }
    result
}
pub fn first_mismatch(expected: &[f32], actual: &[f32]) -> Option<usize> {
    expected
        .iter()
        .zip(actual)
        .position(|(a, b)| !b.is_finite() || a.to_bits() != b.to_bits())
        .or_else(|| {
            if expected.len() != actual.len() {
                Some(expected.len().min(actual.len()))
            } else {
                None
            }
        })
}
#[derive(Debug, Serialize, Deserialize)]
pub struct MeasuredTrial {
    pub schema_version: u32,
    pub workload: String,
    pub candidate: Candidate,
    pub trial: u64,
    pub order: u64,
    pub frames: u64,
    pub sample_count: u64,
    pub elapsed_ns: u64,
    pub preparation_ns: u64,
    pub source: String,
    pub scope: String,
    pub block: usize,
    pub asset_sample_bytes: u64,
    pub output_buffer_bytes: u64,
    pub allocator_memory_bytes: Option<u64>,
    pub conformance: String,
}
fn ns(begin: Instant) -> Result<u64, String> {
    u64::try_from(begin.elapsed().as_nanos()).map_err(|e| e.to_string())
}
pub fn measure(
    workload: &Workload,
    candidate: Candidate,
    trial: u64,
    order: u64,
) -> Result<MeasuredTrial, String> {
    let expected = workload.expected();
    let batches = workload.batches();
    let begin = Instant::now();
    let mut adapter = Prepared::new(workload, candidate)?;
    let mut output = vec![vec![0.; 256]];
    let mut tail = vec![vec![0.; workload.frames % 256]];
    let preparation_ns = ns(begin)?;
    let mut rendered = vec![0.; workload.frames]; // Validation capture outside preparation and render boundaries.
    let begin = Instant::now();
    adapter.enqueue(batches)?;
    let mut offset = 0;
    while offset + 256 <= workload.frames {
        adapter.render_into(&mut output)?;
        rendered[offset..offset + 256].copy_from_slice(&output[0]);
        offset += 256;
    }
    if offset < workload.frames {
        adapter.render_into(&mut tail)?;
        rendered[offset..].copy_from_slice(&tail[0]);
    }
    black_box(&rendered);
    let elapsed_ns = ns(begin)?;
    let conformance = if first_mismatch(&expected, &rendered).is_none()
        && adapter.observe() == workload.expected_state()
    {
        "pass"
    } else {
        "fail"
    };
    Ok(MeasuredTrial{schema_version:1,workload:workload.id.clone(),candidate,trial,order,frames:workload.frames as u64,sample_count:workload.frames as u64,elapsed_ns,preparation_ns,source:"monotonic".into(),
        scope:"enqueue + preallocated render + identical output capture copy; fixture/events/oracle construction excluded".into(),block:256,asset_sample_bytes:(workload.samples.len()*4) as u64,output_buffer_bytes:((256+workload.frames%256)*4) as u64,allocator_memory_bytes:None,conformance:conformance.into()})
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Aggregate {
    pub workload: String,
    pub candidate: Candidate,
    pub repeats: usize,
    pub eligible: bool,
    pub durations_ns: Vec<u64>,
    pub preparation_ns: Vec<u64>,
    pub total_ns: u64,
    pub total_frames: u64,
    pub min_ns: Option<u64>,
    pub max_ns: Option<u64>,
    pub median_ns: Option<u64>,
}
pub fn aggregate(
    workload: &str,
    candidate: Candidate,
    trials: &[MeasuredTrial],
    eligible: bool,
) -> Result<Aggregate, String> {
    let selected: Vec<_> = trials
        .iter()
        .filter(|t| t.workload == workload && t.candidate == candidate)
        .collect();
    let eligible = eligible
        && !selected.is_empty()
        && selected
            .iter()
            .all(|t| t.conformance == "pass" && t.source == "monotonic" && t.schema_version == 1);
    let mut result = Aggregate {
        workload: workload.into(),
        candidate,
        repeats: selected.len(),
        eligible,
        durations_ns: vec![],
        preparation_ns: vec![],
        total_ns: 0,
        total_frames: 0,
        min_ns: None,
        max_ns: None,
        median_ns: None,
    };
    if eligible {
        for trial in selected {
            result.total_ns = result
                .total_ns
                .checked_add(trial.elapsed_ns)
                .ok_or("timing overflow")?;
            result.total_frames = result
                .total_frames
                .checked_add(trial.frames)
                .ok_or("frame overflow")?;
            result.durations_ns.push(trial.elapsed_ns);
            result.preparation_ns.push(trial.preparation_ns);
        }
        let mut sorted = result.durations_ns.clone();
        sorted.sort();
        result.min_ns = sorted.first().copied();
        result.max_ns = sorted.last().copied();
        result.median_ns = Some(sorted[(sorted.len() - 1) / 2]);
    }
    Ok(result)
}
pub fn run(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let first_fixture = crate::model::Fixture::first()?;
    let mut first_direct =
        crate::conformance::run_suite(&first_fixture, crate::reference::DirectLoop::default);
    let mut first_external =
        crate::conformance::run_suite(&first_fixture, crate::external::FundspCopy::default);
    first_direct.adapter = "direct-loop".into();
    first_direct.scheduling = "reference-per-frame".into();
    first_external.adapter = "fundsp-0.20.0".into();
    first_external.scheduling = "adapter-emulated; position adapter-tracked".into();
    let literal_gates = crate::contracts::verify(RenderState::new);
    let workloads = workloads();
    let candidates = [Candidate::Offline, Candidate::Direct, Candidate::Fundsp];
    let mut checks = vec![];
    for workload in &workloads {
        for candidate in candidates {
            for block in [1, 64, 257] {
                checks.push(check(workload, candidate, block));
            }
        }
    }
    let mut trials = vec![];
    let mut execution_errors = vec![];
    let mut order = 0;
    for workload in &workloads {
        for round in 0..15 {
            // Three warmups; twelve retained repetitions, rotating candidate order.
            for index in 0..3 {
                let candidate = candidates[(index + round) % 3];
                let literal_pass = match candidate {
                    Candidate::Offline => literal_gates.iter().all(|g| g.passed),
                    Candidate::Direct => {
                        first_direct.outcome() == crate::conformance::Outcome::Pass
                    }
                    Candidate::Fundsp => {
                        first_external.outcome() == crate::conformance::Outcome::Pass
                    }
                };
                if !literal_pass {
                    continue;
                }
                if !checks
                    .iter()
                    .filter(|c| c.workload == workload.id && c.candidate == candidate)
                    .all(|c| c.outcome == "pass")
                {
                    continue;
                }
                match measure(workload,candidate,round.saturating_sub(3) as u64,order){Ok(t)=>{if t.conformance!="pass"{execution_errors.push(serde_json::json!({"workload":workload.id,"candidate":candidate,"round":round,"error":"timed output conformance failed"}));}if round>=3{trials.push(t);}},Err(error)=>execution_errors.push(serde_json::json!({"workload":workload.id,"candidate":candidate,"round":round,"error":error}))}
                order += 1;
            }
        }
    }
    let mut summaries = vec![];
    for workload in &workloads {
        for candidate in candidates {
            let eligible = checks
                .iter()
                .filter(|c| c.workload == workload.id && c.candidate == candidate)
                .all(|c| c.outcome == "pass")
                && !execution_errors.iter().any(|e| {
                    e["workload"] == workload.id
                        && e["candidate"] == serde_json::to_value(candidate).unwrap()
                });
            summaries.push(aggregate(&workload.id, candidate, &trials, eligible)?);
        }
    }
    let manifest = serde_json::json!({"schema_version":1,"kind":"offline-study","revision_at_run":results::command_text("git",&["rev-parse","HEAD"]),
        "dirty_status_at_run":results::command_text("git",&["status","--porcelain"]),"compiler":env!("HARNESS_RUSTC"),"profile":env!("HARNESS_PROFILE"),"opt_level":env!("HARNESS_OPT_LEVEL"),"target":env!("HARNESS_TARGET"),"encoded_rustflags":env!("HARNESS_CARGO_ENCODED_RUSTFLAGS"),
        "hardware":results::command_text("sysctl",&["-n","machdep.cpu.brand_string"]),"os":results::command_text("uname",&["-sr"]),"dependency_lock":include_str!("../Cargo.lock"),"sample_rate":48000,"channels":1,"sample_format":"f32","quality":"exact-copy and fixed ordered f32 gain/sum; no smoothing",
        "warmups":3,"repeats":12,"ordering":"per-workload rotating candidate order each round; deterministic","timing_source":"Instant monotonic","timing_block":256,"conformance_blocks":[1,64,257],"limits":Limits::default(),
        "memory_scope":"logical asset sample bytes and output buffer bytes only; allocator totals, RSS, peak and preparation breakdown unavailable",
        "timing_scope":"enqueue + preallocated render + identical output capture copy; event and fixture construction outside timer; state observation and full validation outside timer",
        "preparation_scope":"asset load/copy + compile or adapter setup + state + output buffers; no file decoding",
        "limitations":["One machine/build; offline throughput is not callback reliability","FunDSP scalar tick path, not optimized block/SIMD path","Native and adapter-emulated scheduling differ; overhead included","Position for references is adapter bookkeeping","No formal confidence intervals or universal fastest claim","Hardware model may be unavailable; dirty checkout recorded at run, not a reproducible source snapshot","Warmup samples are discarded; twelve raw measured repeats retained"]});
    let mut raw = vec![];
    for trial in &trials {
        serde_json::to_writer(&mut raw, trial)?;
        raw.push(b'\n');
    }
    let dir = create_run_dir(root)?;
    write_artifacts(
        &dir,
        vec![
            ("manifest.json".into(), json_bytes(&manifest)?),
            ("workloads.json".into(), json_bytes(&workloads)?),
            (
                "conformance.json".into(),
                json_bytes(
                    &serde_json::json!({"schema_version":1,"checks":checks,"execution_errors":execution_errors,"literal_offline":literal_gates,"first_direct":first_direct,"first_fundsp":first_external}),
                )?,
            ),
            ("timings.jsonl".into(), raw),
            (
                "summary.json".into(),
                json_bytes(
                    &serde_json::json!({"schema_version":1,"aggregates":summaries,"raw":"timings.jsonl"}),
                )?,
            ),
        ],
    )?;
    Ok(dir)
}
