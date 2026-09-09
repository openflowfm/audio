use crate::model::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    CapabilityGap,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Issue {
    pub kind: String,
    pub frame: Option<u64>,
    pub detail: String,
    pub expected_bits: Option<u32>,
    pub actual_bits: Option<u32>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct BlockEvidence {
    pub output_frame: u64,
    pub requested_frames: u64,
    pub actual_samples: u64,
    pub state: State,
    /// IEEE-754 bit patterns preserve NaN payloads and signed zero. Failed cases only on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_bits: Option<Vec<u32>>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Case {
    pub id: String,
    pub outcome: Outcome,
    pub blocks: Vec<BlockEvidence>,
    pub issues: Vec<Issue>,
    pub failure: Option<Failure>,
    pub expected_error: Option<RequestError>,
    pub observed_error: Option<Failure>,
    pub final_state: Option<State>,
}
impl Case {
    fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            outcome: Outcome::Pass,
            blocks: vec![],
            issues: vec![],
            failure: None,
            expected_error: None,
            observed_error: None,
            final_state: None,
        }
    }
    fn issue(&mut self, kind: &str, detail: impl Into<String>) {
        self.outcome = Outcome::Fail;
        self.issues.push(Issue {
            kind: kind.into(),
            frame: None,
            detail: detail.into(),
            expected_bits: None,
            actual_bits: None,
        });
    }
    fn failed(&mut self, failure: Failure) {
        if self.outcome != Outcome::Fail {
            self.outcome = if matches!(failure, Failure::CapabilityGap(_)) {
                Outcome::CapabilityGap
            } else {
                Outcome::Fail
            };
        }
        self.failure = Some(failure);
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub workload: String,
    pub adapter: String,
    pub scheduling: String,
    pub cases: Vec<Case>,
}
impl Report {
    pub fn outcome(&self) -> Outcome {
        if self.cases.is_empty() || self.cases.iter().any(|c| c.outcome == Outcome::Fail) {
            Outcome::Fail
        } else if self
            .cases
            .iter()
            .any(|c| c.outcome == Outcome::CapabilityGap)
        {
            Outcome::CapabilityGap
        } else {
            Outcome::Pass
        }
    }
}
pub fn prepare(adapter: &mut impl Adapter, fixture: &Fixture) -> Result<(), Failure> {
    adapter.load(&fixture.asset, fixture.sample_rate, fixture.channels)?;
    adapter.compile(0)?;
    adapter.initialize()
}
// Literal observations, independent of the reference adapter's playback algorithm.
fn oracle_state(frame: u64, started: bool) -> State {
    let (source, status, pending) = if !started {
        (0, Status::Stopped, None)
    } else {
        match frame {
            0..=3 => (0, Status::Stopped, Some(3)),
            4 => (1, Status::Playing, None),
            5 => (2, Status::Playing, None),
            6 => (3, Status::Playing, None),
            7..=10 => (4, Status::Ended, None),
            _ => unreachable!("validated ten-frame fixture"),
        }
    };
    State {
        output_frame: frame,
        source_frame: source,
        status,
        pending_start: pending,
        loaded: true,
        compiled: true,
        initialized: true,
    }
}
fn check_state(case: &mut Case, actual: &State, expected: &State) {
    if actual != expected {
        case.issue(
            "state",
            format!("expected {expected:?}; observed {actual:?}"),
        );
    }
}
fn render_case(
    mut adapter: impl Adapter,
    fixture: &Fixture,
    id: String,
    partitions: &[usize],
    started: bool,
) -> Case {
    let mut case = Case::new(id);
    let setup = prepare(&mut adapter, fixture).and_then(|()| {
        if started {
            adapter.start(0, fixture.start_frame)
        } else {
            Ok(())
        }
    });
    if let Err(error) = setup {
        case.failed(error);
        case.final_state = Some(adapter.observe());
        return case;
    }
    check_state(&mut case, &adapter.observe(), &oracle_state(0, started));
    let mut offset = 0;
    for &frames in partitions {
        let result = adapter.render(frames);
        let state = adapter.observe();
        case.final_state = Some(state.clone());
        let pcm = match result {
            Ok(pcm) => pcm,
            Err(error) => {
                case.failed(error);
                break;
            }
        };
        if pcm.len() != frames {
            case.issue(
                "length",
                format!(
                    "block at frame {offset}: expected {frames} samples, got {}",
                    pcm.len()
                ),
            );
        }
        for (i, &actual) in pcm.iter().enumerate() {
            let frame = offset + i;
            let expected = if started {
                fixture.expected.get(frame).copied()
            } else {
                Some(0.0)
            };
            let kind = if !actual.is_finite() {
                Some("non_finite")
            } else if i < frames
                && expected.is_some_and(|value| value.to_bits() != actual.to_bits())
            {
                Some("sample")
            } else {
                None
            };
            if let Some(kind) = kind {
                case.outcome = Outcome::Fail;
                case.issues.push(Issue {
                    kind: kind.into(),
                    frame: Some(frame as u64),
                    detail: "exact f32 mismatch".into(),
                    expected_bits: expected.map(f32::to_bits),
                    actual_bits: Some(actual.to_bits()),
                });
            }
        }
        offset += frames; // Requested boundaries, never candidate output lengths.
        check_state(&mut case, &state, &oracle_state(offset as u64, started));
        case.blocks.push(BlockEvidence {
            output_frame: (offset - frames) as u64,
            requested_frames: frames as u64,
            actual_samples: pcm.len() as u64,
            state,
            sample_bits: Some(pcm.iter().map(|x| x.to_bits()).collect()),
        });
    }
    if case.outcome == Outcome::Pass {
        for block in &mut case.blocks {
            block.sample_bits = None;
        }
    }
    case
}

#[derive(Clone, Copy)]
enum Rejection {
    Empty,
    NonFinite,
    Format,
    UnknownAsset,
    UnknownPlayer,
    Negative,
    Unprepared,
    Duplicate,
    Past,
    Ended,
    Playing,
    Reload,
    Recompile,
    Reinitialize,
}
fn rejection_case(mut adapter: impl Adapter, fixture: &Fixture, rejection: Rejection) -> Case {
    use Rejection::*;
    let (id, expected) = match rejection {
        Empty => ("empty-asset", RequestError::InvalidAsset),
        NonFinite => ("non-finite-asset", RequestError::InvalidAsset),
        Format => ("unsupported-format", RequestError::UnsupportedFormat),
        UnknownAsset => ("unknown-asset", RequestError::UnknownAsset),
        UnknownPlayer => ("unknown-player", RequestError::UnknownPlayer),
        Negative => ("negative-frame", RequestError::InvalidFrame),
        Unprepared => ("unprepared-start", RequestError::Unprepared),
        Duplicate => ("duplicate-start", RequestError::PendingStart),
        Past => ("past-start", RequestError::PastEvent),
        Ended => ("ended-restart", RequestError::NotStopped),
        Playing => ("playing-restart", RequestError::NotStopped),
        Reload => ("reload", RequestError::AlreadyPrepared),
        Recompile => ("recompile", RequestError::AlreadyPrepared),
        Reinitialize => ("reinitialize", RequestError::AlreadyPrepared),
    };
    let mut case = Case::new(id);
    case.expected_error = Some(expected);
    let setup = (|| {
        if !matches!(rejection, Unprepared) {
            prepare(&mut adapter, fixture)?;
        }
        if matches!(rejection, Duplicate | Ended | Playing) {
            adapter.start(0, 3)?;
        }
        if matches!(rejection, Past) {
            adapter.render(1)?;
        }
        if matches!(rejection, Ended) {
            adapter.render(10)?;
        }
        if matches!(rejection, Playing) {
            adapter.render(4)?;
        }
        Ok::<_, Failure>(())
    })();
    if let Err(error) = setup {
        case.failed(error);
        case.final_state = Some(adapter.observe());
        return case;
    }
    let before = adapter.observe();
    let result = match rejection {
        Empty => adapter.load(&[], 48000, 1),
        NonFinite => adapter.load(&[f32::NAN], 48000, 1),
        Format => adapter.load(&[1.0], 44100, 1),
        UnknownAsset => adapter.compile(1),
        UnknownPlayer => adapter.start(1, 3),
        Negative => adapter.start(0, -1),
        Unprepared | Duplicate => adapter.start(0, 3),
        Past => adapter.start(0, 0),
        Ended => adapter.start(0, 10),
        Playing => adapter.start(0, 4),
        Reload => adapter.load(&[1.0], 48000, 1),
        Recompile => adapter.compile(0),
        Reinitialize => adapter.initialize(),
    };
    case.observed_error = result.clone().err();
    match result {
        Err(Failure::Rejected(error)) if error == expected => {}
        Err(error @ Failure::CapabilityGap(_)) | Err(error @ Failure::AdapterError(_)) => {
            case.failed(error)
        }
        other => case.issue("error", format!("expected {expected:?}, got {other:?}")),
    }
    let after = adapter.observe();
    check_state(&mut case, &after, &before);
    case.final_state = Some(after);
    // Verify rejected asset requests did not replace hidden sample storage.
    if matches!(rejection, Empty | NonFinite | Format | Reload) && case.outcome == Outcome::Pass {
        if let Err(error) = adapter.start(0, 3) {
            case.failed(error);
        } else {
            check_continuation(&mut case, &mut adapter, &fixture.expected);
        }
    } else if matches!(rejection, Duplicate) && case.outcome == Outcome::Pass {
        check_continuation(&mut case, &mut adapter, &fixture.expected);
    }
    case
}
fn check_continuation(case: &mut Case, adapter: &mut impl Adapter, expected: &[f32]) {
    match adapter.render(10) {
        Err(error) => case.failed(error),
        Ok(pcm) => {
            if pcm.len() != expected.len()
                || pcm
                    .iter()
                    .zip(expected)
                    .any(|(a, b)| a.to_bits() != b.to_bits())
            {
                case.issue(
                    "rejection_side_effect",
                    "rejected request changed subsequent PCM",
                );
            }
            let state = adapter.observe();
            check_state(case, &state, &oracle_state(10, true));
            case.final_state = Some(state.clone());
            case.blocks.push(BlockEvidence {
                output_frame: 0,
                requested_frames: 10,
                actual_samples: pcm.len() as u64,
                state,
                sample_bits: if case.outcome == Outcome::Pass {
                    None
                } else {
                    Some(pcm.iter().map(|x| x.to_bits()).collect())
                },
            });
        }
    }
}
fn preparation_case(mut adapter: impl Adapter, fixture: &Fixture) -> Case {
    let mut case = Case::new("preparation-and-zero-render");
    let mut expected = State::default();
    check_state(&mut case, &adapter.observe(), &expected);
    let result = (|| {
        adapter.load(&fixture.asset, fixture.sample_rate, fixture.channels)?;
        expected.loaded = true;
        check_state(&mut case, &adapter.observe(), &expected);
        adapter.compile(0)?;
        expected.compiled = true;
        check_state(&mut case, &adapter.observe(), &expected);
        adapter.initialize()?;
        expected.initialized = true;
        check_state(&mut case, &adapter.observe(), &expected);
        adapter.start(0, 0)?;
        expected.pending_start = Some(0);
        let empty = adapter.render(0)?;
        if !empty.is_empty() {
            case.issue("length", "zero render returned samples");
        }
        check_state(&mut case, &adapter.observe(), &expected);
        Ok::<(), Failure>(())
    })();
    if let Err(error) = result {
        case.failed(error);
    }
    case.final_state = Some(adapter.observe());
    case
}
pub fn run_suite<A: Adapter>(fixture: &Fixture, mut factory: impl FnMut() -> A) -> Report {
    let mut report = Report {
        schema_version: SCHEMA_VERSION,
        workload: fixture.id.clone(),
        adapter: "unspecified".into(),
        scheduling: "unspecified".into(),
        cases: vec![],
    };
    if let Err(error) = fixture.validate() {
        let mut case = Case::new("fixture-validation");
        case.issue("fixture", error);
        report.cases.push(case);
        return report;
    }
    for (i, partition) in fixture.partitions.iter().enumerate() {
        report.cases.push(render_case(
            factory(),
            fixture,
            format!("partition-{i}"),
            partition,
            true,
        ));
    }
    report.cases.push(render_case(
        factory(),
        fixture,
        "boundary-probes".into(),
        &[3, 2, 2, 3],
        true,
    ));
    report.cases.push(render_case(
        factory(),
        fixture,
        "stopped-silence".into(),
        &[10],
        false,
    ));
    report.cases.push(preparation_case(factory(), fixture));
    use Rejection::*;
    for rejection in [
        Empty,
        NonFinite,
        Format,
        UnknownAsset,
        UnknownPlayer,
        Negative,
        Unprepared,
        Duplicate,
        Past,
        Ended,
        Playing,
        Reload,
        Recompile,
        Reinitialize,
    ] {
        report
            .cases
            .push(rejection_case(factory(), fixture, rejection));
    }
    report
}
