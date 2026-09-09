//! Native block references. Scheduling and positions are adapter bookkeeping.
use crate::{
    offline::*,
    study::{Candidate, Mode, Workload},
};
use fundsp::{
    audiounit::AudioUnit,
    buffer::BufferVec,
    hacker32::{mul, An},
    wave::{Wave, WavePlayer},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Scheduled<S> {
    inner: S,
    start: Arc<AtomicUsize>,
    frame: usize,
}
impl<S: oddio::Signal<Frame = f32>> oddio::Signal for Scheduled<S> {
    type Frame = f32;
    fn sample(&mut self, interval: f32, out: &mut [f32]) {
        let silence = self
            .start
            .load(Ordering::Relaxed)
            .saturating_sub(self.frame)
            .min(out.len());
        out[..silence].fill(0.);
        if silence < out.len() {
            self.inner.sample(interval, &mut out[silence..]);
        }
        self.frame += out.len();
    }
    fn is_finished(&self) -> bool {
        self.frame > self.start.load(Ordering::Relaxed) && self.inner.is_finished()
    }
}
enum Engine {
    Fundsp {
        voices: Vec<Box<dyn AudioUnit>>,
        input: BufferVec,
        output: BufferVec,
    },
    Oddio {
        mixer: oddio::Mixer<f32>,
        _control: oddio::MixerControl<f32>,
        _handles: Vec<oddio::Mixed>,
    },
}
pub struct NativeBank {
    workload: Workload,
    frame: usize,
    starts: Vec<Arc<AtomicUsize>>,
    engine: Engine,
}
impl NativeBank {
    pub fn new(w: &Workload, candidate: Candidate) -> Result<Self, String> {
        if w.mode == Mode::Bursts {
            return Err("capability gap: native block references currently support one scheduled start and a fixed loop; stop/seek batches are unsupported".into());
        }
        let starts: Vec<_> = (0..w.voices)
            .map(|_| Arc::new(AtomicUsize::new(usize::MAX)))
            .collect();
        let engine = match candidate {
            Candidate::FundspBlock => {
                let wave = Arc::new(Wave::from_samples(48000., &w.samples));
                let voices = (0..w.voices)
                    .map(|v| {
                        let player = WavePlayer::new(
                            &wave,
                            0,
                            if w.mode == Mode::Loop { 1 } else { 0 },
                            if w.mode == Mode::Loop { 256 } else { 257 },
                            if w.mode == Mode::Loop { Some(1) } else { None },
                        );
                        // Static composition behind one block-level dynamic call. WavePlayer's
                        // pinned process implementation falls back to native tick internally.
                        let mut node = An(player) >> mul(w.gain(v));
                        AudioUnit::allocate(&mut node);
                        Box::new(node) as Box<dyn AudioUnit>
                    })
                    .collect();
                Engine::Fundsp {
                    voices,
                    input: BufferVec::new(0),
                    output: BufferVec::new(1),
                }
            }
            Candidate::Oddio => {
                let (mut control, mixer) = oddio::Mixer::new();
                let samples = if w.mode == Mode::Loop {
                    &w.samples[1..256]
                } else {
                    &w.samples[..]
                };
                let frames = oddio::Frames::from_slice(48000, samples);
                let mut handles = Vec::with_capacity(w.voices);
                // Pinned Mixer traverses reverse insertion order. No removals occur
                // in loop workloads; only one voice exists in the once workload.
                for v in (0..w.voices).rev() {
                    if w.mode == Mode::Loop {
                        let (_, mut gain) = oddio::Gain::new(oddio::Cycle::new(frames.clone()));
                        gain.set_amplitude_ratio(w.gain(v));
                        handles.push(control.play(Scheduled {
                            inner: gain,
                            start: starts[v].clone(),
                            frame: 0,
                        }));
                    } else {
                        let (_, mut gain) =
                            oddio::Gain::new(oddio::FramesSignal::from(frames.clone()));
                        gain.set_amplitude_ratio(w.gain(v));
                        handles.push(control.play(Scheduled {
                            inner: gain,
                            start: starts[v].clone(),
                            frame: 0,
                        }));
                    }
                }
                Engine::Oddio {
                    mixer,
                    _control: control,
                    _handles: handles,
                }
            }
            _ => return Err("invalid native reference candidate".into()),
        };
        Ok(Self {
            workload: w.clone(),
            frame: 0,
            starts,
            engine,
        })
    }
    pub fn enqueue(&mut self, batches: Vec<Batch>) -> Result<(), String> {
        // Validate the whole submission before publishing scheduled starts.
        let mut seen = vec![false; self.starts.len()];
        for b in &batches {
            for c in &b.commands {
                match *c {
                Command::Start(v) if v<self.starts.len() && b.revision==0 && b.frame>=self.frame as u64 && !seen[v] && self.starts[v].load(Ordering::Relaxed)==usize::MAX => {seen[v]=true;},
                Command::SetLoop(v,Some((1,256))) if v<self.starts.len() && self.workload.mode==Mode::Loop && b.frame==0 && b.revision==0 => {},
                _=>return Err("capability gap: only fixed loops and one scheduled start per player are supported".into())
            }
            }
        }
        for b in batches {
            for c in b.commands {
                if let Command::Start(v) = c {
                    self.starts[v].store(b.frame as usize, Ordering::Relaxed);
                }
            }
        }
        Ok(())
    }
    pub fn render_into(&mut self, out: &mut [Vec<f32>]) -> Result<(), String> {
        if out.len() != 1 || out[0].len() > 4096 {
            return Err("invalid reference buffer".into());
        }
        if out[0].is_empty() {
            return Ok(());
        }
        match &mut self.engine {
            Engine::Oddio { mixer, .. } => oddio::run(mixer, 48000, &mut out[0]),
            Engine::Fundsp {
                voices,
                input,
                output,
            } => {
                out[0].fill(0.);
                for (v, node) in voices.iter_mut().enumerate() {
                    let start = self.starts[v].load(Ordering::Relaxed);
                    let begin = start.saturating_sub(self.frame).min(out[0].len());
                    let end = if self.workload.mode == Mode::Once {
                        start
                            .saturating_add(257)
                            .saturating_sub(self.frame)
                            .min(out[0].len())
                    } else {
                        out[0].len()
                    };
                    for chunk in out[0][begin..end].chunks_mut(64) {
                        node.process(chunk.len(), &input.buffer_ref(), &mut output.buffer_mut());
                        for (i, sample) in chunk.iter_mut().enumerate() {
                            *sample += output.buffer_ref().at_f32(0, i);
                        }
                    }
                }
            }
        }
        self.frame += out[0].len();
        Ok(())
    }
    pub fn observe(&self) -> Observation {
        Observation {
            output_frame: self.frame as u64,
            revision: 0,
            pending_batches: 0,
            players: self
                .starts
                .iter()
                .map(|s| {
                    let elapsed = self.frame.saturating_sub(s.load(Ordering::Relaxed));
                    let (position, status) = if elapsed == 0 {
                        (0, Playback::Stopped)
                    } else if self.workload.mode == Mode::Loop {
                        (1 + elapsed % 255, Playback::Playing)
                    } else {
                        (
                            elapsed.min(257),
                            if elapsed >= 257 {
                                Playback::Ended
                            } else {
                                Playback::Playing
                            },
                        )
                    };
                    PlayerState {
                        position,
                        status,
                        loop_region: if self.workload.mode == Mode::Loop {
                            Some((1, 256))
                        } else {
                            None
                        },
                    }
                })
                .collect(),
        }
    }
}

/// Independent short literal PCM/state gates; seeded workload checks still gate
/// every workload and external partition separately before timing.
pub fn gates(candidate: Candidate) -> Vec<crate::contracts::Gate> {
    let mut gates = Vec::new();
    for mode in [Mode::Once, Mode::Loop] {
        for partitions in [vec![8], vec![3, 5], vec![1; 8]] {
            let mut w = Workload::new("native-literal", 1, 8, mode, 128, 0);
            w.samples.fill(0.);
            w.samples[..4].copy_from_slice(&[2., 4., 6., 8.]);
            let expected = if mode == Mode::Once {
                vec![0., 0., 0., 1., 2., 3., 4., 0.]
            } else {
                vec![0., 0., 0., 2., 3., 4., 0., 0.]
            };
            let expected_state = Observation {
                output_frame: 8,
                revision: 0,
                pending_batches: 0,
                players: vec![PlayerState {
                    position: if mode == Mode::Once { 5 } else { 6 },
                    status: Playback::Playing,
                    loop_region: if mode == Mode::Once {
                        None
                    } else {
                        Some((1, 256))
                    },
                }],
            };
            let mut bank = NativeBank::new(&w, candidate).unwrap();
            bank.enqueue(w.batches()).unwrap();
            let mut actual = Vec::new();
            for n in &partitions {
                let mut out = vec![vec![0.; *n]];
                bank.render_into(&mut out).unwrap();
                actual.extend_from_slice(&out[0]);
            }
            let state = bank.observe();
            let first_mismatch = crate::study::first_mismatch(&expected, &actual);
            let passed = first_mismatch.is_none() && state == expected_state;
            gates.push(crate::contracts::Gate {
                id: format!("native-{mode:?}"),
                passed,
                partitions,
                detail: if passed {
                    None
                } else {
                    Some("native literal PCM/state mismatch".into())
                },
                first_mismatch,
                actual_bits: actual.iter().map(|x| x.to_bits()).collect(),
                state,
            });
        }
    }
    gates
}

pub fn mode_passes(gates: &[crate::contracts::Gate], mode: Mode) -> bool {
    let id = format!("native-{mode:?}");
    let relevant: Vec<_> = gates.iter().filter(|g| g.id == id).collect();
    !relevant.is_empty() && relevant.iter().all(|g| g.passed)
}

pub fn required_gate_ids(candidate: Candidate, mode: Mode) -> Vec<String> {
    if candidate == Candidate::Oddio && mode == Mode::Once {
        return vec![];
    }
    match mode {
        Mode::Once => vec!["native-Once".into()],
        Mode::Loop => vec!["native-Loop".into()],
        Mode::Bursts => vec![],
    }
}
