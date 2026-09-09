//! Literal offline conformance gates, also exercised through deliberately faulty test adapters.
use crate::{offline::*, study::first_mismatch};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub trait Runtime {
    fn enqueue(&mut self, batch: Batch) -> Result<(), Error>;
    fn advance_revision(&mut self, revision: u64) -> Result<(), Error>;
    fn render(&mut self, frames: usize) -> Result<Vec<Vec<f32>>, Error>;
    fn observe(&self) -> Observation;
}
impl Runtime for RenderState {
    fn enqueue(&mut self, batch: Batch) -> Result<(), Error> {
        RenderState::enqueue(self, batch)
    }
    fn advance_revision(&mut self, revision: u64) -> Result<(), Error> {
        RenderState::advance_revision(self, revision)
    }
    fn render(&mut self, frames: usize) -> Result<Vec<Vec<f32>>, Error> {
        RenderState::render(self, frames)
    }
    fn observe(&self) -> Observation {
        RenderState::observe(self)
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Gate {
    pub id: String,
    pub passed: bool,
    pub partitions: Vec<usize>,
    pub detail: Option<String>,
    pub first_mismatch: Option<usize>,
    pub actual_bits: Vec<u32>,
    pub state: Observation,
}
fn batch(frame: u64, commands: Vec<Command>) -> Batch {
    Batch {
        frame,
        revision: 0,
        commands,
    }
}
fn program(samples: &[f32]) -> Arc<Program> {
    let mut assets = Assets::new(Limits::default()).unwrap();
    assets.load(samples, 48000).unwrap();
    Arc::new(
        Graph {
            nodes: vec![Node::Player { asset: 0 }],
            outputs: vec![("main".into(), 0)],
        }
        .compile(&assets, Limits::default())
        .unwrap(),
    )
}
pub fn verify<R: Runtime>(mut factory: impl FnMut(Arc<Program>) -> R) -> Vec<Gate> {
    let mut gates = vec![];
    for id in [
        "first-ten",
        "stop-seek",
        "half-open-loop",
        "atomic-rejection",
        "stale-cancellation",
    ] {
        let samples: &[f32] = match id {
            "first-ten" => &[0.25, -0.5, 0.75, -1.],
            "half-open-loop" => &[0.125, 0.25, 0.5, 0.75],
            _ => &[1., 2., 3., 4.],
        };
        let expected: &[f32] = match id {
            "first-ten" => &[0., 0., 0., 0.25, -0.5, 0.75, -1., 0., 0., 0.],
            "stop-seek" => &[0., 1., 2., 0., 0., 1., 2., 3.],
            "half-open-loop" => &[0.25, 0.5, 0.25, 0.5, 0.25, 0.5],
            _ => &[0.; 10],
        };
        for partitions in [
            vec![expected.len()],
            vec![1; expected.len()],
            vec![3, 2, expected.len() - 5],
        ] {
            let mut state = factory(program(samples));
            let mut actual = vec![];
            let result = (|| {
                match id {
                    "first-ten" => state.enqueue(batch(3, vec![Command::Start(0)]))?,
                    "stop-seek" => {
                        state.enqueue(batch(1, vec![Command::Start(0)]))?;
                        state.enqueue(batch(3, vec![Command::Stop(0)]))?;
                        state.enqueue(batch(5, vec![Command::Seek(0, 0), Command::Start(0)]))?;
                    }
                    "half-open-loop" => state.enqueue(batch(
                        0,
                        vec![
                            Command::Seek(0, 1),
                            Command::SetLoop(0, Some((1, 3))),
                            Command::Start(0),
                        ],
                    ))?,
                    "atomic-rejection" => {
                        let before = state.observe();
                        if state.enqueue(batch(0, vec![Command::Start(0), Command::Seek(0, 99)]))
                            != Err(Error::InvalidSeek)
                            || state.observe() != before
                        {
                            return Err(Error::InvalidSeek);
                        }
                    }
                    "stale-cancellation" => {
                        state.enqueue(batch(3, vec![Command::Start(0)]))?;
                        state.advance_revision(1)?;
                        let before = state.observe();
                        if state.enqueue(batch(4, vec![Command::Start(0)]))
                            != Err(Error::StaleRevision)
                            || state.observe() != before
                        {
                            return Err(Error::StaleRevision);
                        }
                    }
                    _ => unreachable!(),
                }
                for &frames in &partitions {
                    let block = state.render(frames)?;
                    if block.len() != 1 || block[0].len() != frames {
                        return Err(Error::InvalidBuffer);
                    }
                    actual.extend_from_slice(&block[0]);
                }
                Ok::<(), Error>(())
            })();
            let observation = state.observe();
            let expected_player = match id {
                "first-ten" => (4, Playback::Ended, None),
                "stop-seek" => (3, Playback::Playing, None),
                "half-open-loop" => (1, Playback::Playing, Some((1, 3))),
                _ => (0, Playback::Stopped, None),
            };
            let expected_state = Observation {
                output_frame: expected.len() as u64,
                revision: if id == "stale-cancellation" { 1 } else { 0 },
                pending_batches: 0,
                players: vec![PlayerState {
                    position: expected_player.0,
                    status: expected_player.1,
                    loop_region: expected_player.2,
                }],
            };
            let mismatch = first_mismatch(expected, &actual);
            let passed = result.is_ok() && mismatch.is_none() && observation == expected_state;
            gates.push(Gate {
                id: id.into(),
                passed,
                partitions,
                detail: if passed {
                    None
                } else {
                    Some(format!(
                        "request result {result:?}; expected state {expected_state:?}"
                    ))
                },
                first_mismatch: mismatch,
                actual_bits: actual.iter().map(|x| x.to_bits()).collect(),
                state: observation,
            });
        }
    }
    gates
}
