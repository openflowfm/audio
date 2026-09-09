use audio_flow_harness::{conformance::*, model::*, reference::DirectLoop};

#[derive(Clone, Copy)]
enum Fault {
    None,
    Shift,
    Truncate,
    Stale,
    Nan,
    State,
    Error,
    Gap,
    Extra,
}
struct Faulty {
    inner: DirectLoop,
    fault: Fault,
}
impl Adapter for Faulty {
    fn load(&mut self, asset: &[f32], rate: u32, channels: u16) -> Result<(), Failure> {
        self.inner.load(asset, rate, channels)
    }
    fn compile(&mut self, asset: u32) -> Result<(), Failure> {
        self.inner.compile(asset)
    }
    fn initialize(&mut self) -> Result<(), Failure> {
        self.inner.initialize()
    }
    fn start(&mut self, player: u32, frame: i64) -> Result<(), Failure> {
        self.inner.start(player, frame)
    }
    fn observe(&self) -> State {
        let mut state = self.inner.observe();
        if matches!(self.fault, Fault::State) && state.output_frame > 0 {
            state.source_frame = 99;
        }
        state
    }
    fn render(&mut self, frames: usize) -> Result<Vec<f32>, Failure> {
        let begin = self.inner.observe().output_frame;
        let mut pcm = self.inner.render(frames)?;
        match self.fault {
            Fault::Shift => {
                for (i, value) in pcm.iter_mut().enumerate() {
                    *value = match begin + i as u64 {
                        4 => 0.25,
                        5 => -0.5,
                        6 => 0.75,
                        7 => -1.0,
                        _ => 0.0,
                    };
                }
            }
            Fault::Truncate => {
                pcm.pop();
            }
            Fault::Extra => pcm.push(0.0),
            Fault::Stale => {
                for (i, value) in pcm.iter_mut().enumerate() {
                    if begin + i as u64 >= 7 {
                        *value = -1.0;
                    }
                }
            }
            Fault::Nan => {
                if let Some(value) = pcm.first_mut() {
                    *value = f32::from_bits(0x7fc00042);
                }
            }
            Fault::Error => return Err(Failure::AdapterError("deliberate render failure".into())),
            Fault::Gap => return Err(Failure::CapabilityGap("render unavailable".into())),
            _ => {}
        }
        Ok(pcm)
    }
}
fn run_fault(fault: Fault) -> Report {
    run_suite(&Fixture::first().unwrap(), || Faulty {
        inner: DirectLoop::default(),
        fault,
    })
}
#[test]
fn literal_fixture_and_all_partitions_pass() {
    let fixture = Fixture::first().unwrap();
    assert_eq!(
        fixture.expected,
        vec![0.0, 0.0, 0.0, 0.25, -0.5, 0.75, -1.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        fixture.partitions,
        vec![vec![10], vec![1; 10], vec![3, 2, 5]]
    );
    let report = run_fault(Fault::None);
    assert_eq!(report.outcome(), Outcome::Pass, "{report:#?}");
    assert_eq!(
        report
            .cases
            .iter()
            .filter(|c| c.id.starts_with("partition-"))
            .count(),
        3
    );
}
#[test]
fn faulty_adapters_are_rejected_and_localized() {
    for (fault, kind, frame) in [
        (Fault::Shift, "sample", Some(3)),
        (Fault::Truncate, "length", None),
        (Fault::Extra, "length", None),
        (Fault::Stale, "sample", Some(7)),
        (Fault::Nan, "non_finite", Some(0)),
        (Fault::State, "state", None),
    ] {
        let report = run_fault(fault);
        assert_eq!(report.outcome(), Outcome::Fail);
        assert!(
            report.cases[0]
                .issues
                .iter()
                .any(|i| i.kind == kind && i.frame == frame),
            "{report:#?}"
        );
        assert_eq!(
            report.cases.len(),
            run_fault(Fault::None).cases.len(),
            "failure must not short circuit the suite"
        );
    }
}
#[test]
fn adapter_errors_and_gaps_are_not_passes() {
    assert_eq!(run_fault(Fault::Error).outcome(), Outcome::Fail);
    assert_eq!(run_fault(Fault::Gap).outcome(), Outcome::CapabilityGap);
    assert!(run_fault(Fault::Error).cases[0].failure.is_some());
}
#[test]
fn malformed_partitions_and_fixture_versions_are_rejected() {
    for partitions in [
        vec![],
        vec![vec![]],
        vec![vec![0, 10]],
        vec![vec![9]],
        vec![vec![11]],
        vec![vec![usize::MAX, 1]],
    ] {
        let mut fixture = Fixture::first().unwrap();
        fixture.partitions = partitions;
        assert!(fixture.validate().is_err());
    }
    let mut fixture = Fixture::first().unwrap();
    fixture.schema_version = 2;
    assert!(fixture.validate().is_err());
    fixture = Fixture::first().unwrap();
    fixture.expected[0] = f32::NAN;
    assert!(fixture.validate().is_err());
}
#[test]
fn preparation_and_rejections_preserve_state_and_pending_audio() {
    let mut adapter = DirectLoop::default();
    let initial = adapter.observe();
    assert_eq!(
        adapter.start(0, 3),
        Err(Failure::Rejected(RequestError::Unprepared))
    );
    assert_eq!(adapter.observe(), initial);
    adapter.load(&[0.25, -0.5, 0.75, -1.0], 48000, 1).unwrap();
    assert!(!adapter.observe().compiled);
    adapter.compile(0).unwrap();
    assert!(!adapter.observe().initialized);
    adapter.initialize().unwrap();
    adapter.start(0, 3).unwrap();
    let before = adapter.observe();
    assert_eq!(
        adapter.load(&[], 48000, 1),
        Err(Failure::Rejected(RequestError::InvalidAsset))
    );
    assert_eq!(adapter.observe(), before);
    assert_eq!(
        adapter.render(10).unwrap(),
        vec![0.0, 0.0, 0.0, 0.25, -0.5, 0.75, -1.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn failed_pcm_and_all_cases_survive_serialization() {
    use audio_flow_harness::results::*;
    let report = run_fault(Fault::Nan);
    let dir = create_run_dir(&std::env::temp_dir()).unwrap();
    write_run(&dir, &report, &[Trial::synthetic(0, 10, 99)], true).unwrap();
    assert!(is_complete(&dir));
    let restored: Report =
        serde_json::from_slice(&std::fs::read(dir.join("conformance.json")).unwrap()).unwrap();
    assert_eq!(restored.cases.len(), report.cases.len());
    assert_eq!(
        restored.cases[0].blocks[0].sample_bits.as_ref().unwrap()[0],
        0x7fc00042
    );
    let pcm = std::fs::read(dir.join("failed-0.f32le")).unwrap();
    assert_eq!(&pcm[..4], &0x7fc00042u32.to_le_bytes());
    let summary: Summary =
        serde_json::from_slice(&std::fs::read(dir.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary.excluded_trials, 1);
    assert_eq!(summary.total_ns, 0);
    assert!(!summary.comparative_speed_eligible);
}
#[test]
fn zero_length_render_is_a_noop_even_with_a_pending_start() {
    let mut adapter = DirectLoop::default();
    assert_eq!(adapter.render(0).unwrap(), Vec::<f32>::new());
    prepare(&mut adapter, &Fixture::first().unwrap()).unwrap();
    adapter.start(0, 0).unwrap();
    let before = adapter.observe();
    assert!(adapter.render(0).unwrap().is_empty());
    assert_eq!(adapter.observe(), before);
    assert_eq!(adapter.render(1).unwrap(), vec![0.25]);
}
#[test]
fn pinned_fundsp_copy_adapter_meets_first_slice() {
    let report = run_suite(
        &Fixture::first().unwrap(),
        audio_flow_harness::external::FundspCopy::default,
    );
    assert_eq!(report.outcome(), Outcome::Pass, "{report:#?}");
}
