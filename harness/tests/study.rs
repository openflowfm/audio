use audio_flow_harness::study::*;
#[test]
fn seeded_oracles_and_reference_adapters_agree_across_partitions() {
    for mode in [Mode::Once, Mode::Loop, Mode::Bursts] {
        let workload = Workload::new("small", 3, 512, mode, 32, 42);
        assert_eq!(
            workload.samples,
            Workload::new("small", 3, 512, mode, 32, 42).samples
        );
        for candidate in [Candidate::Offline, Candidate::Direct, Candidate::Fundsp] {
            for block in [1, 31, 256] {
                let result = check(&workload, candidate, block);
                assert_eq!(
                    result.outcome,
                    if mode == Mode::Bursts && candidate != Candidate::Offline {
                        "capability_gap"
                    } else {
                        "pass"
                    },
                    "{result:#?}"
                );
            }
        }
    }
}
#[test]
fn study_detector_localizes_extra_short_shifted_and_nonfinite_output() {
    let oracle = [0., 0.25, -0.5, 0.];
    assert_eq!(first_mismatch(&oracle, &oracle), None);
    for (bad, at) in [
        (vec![0., 0., 0.25, -0.5], 1),
        (vec![0., 0.25, -0.5], 3),
        (vec![0., 0.25, -0.5, 0., 1.], 4),
        (vec![0., 0.25, f32::NAN, 0.], 2),
    ] {
        assert_eq!(first_mismatch(&oracle, &bad), Some(at));
    }
}
#[test]
fn synthetic_study_aggregation_keeps_integer_metadata_and_excludes_failed_trials() {
    let trial = MeasuredTrial {
        schema_version: 1,
        workload: "x".into(),
        candidate: Candidate::Offline,
        trial: 0,
        order: 0,
        frames: 10,
        sample_count: 10,
        elapsed_ns: 9_007_199_254_740_993,
        preparation_ns: 11,
        source: "synthetic-test-only".into(),
        scope: "synthetic".into(),
        block: 10,
        asset_sample_bytes: 16,
        output_buffer_bytes: 40,
        allocator_memory_bytes: None,
        conformance: "pass".into(),
        failure: None,
    };
    let json = serde_json::to_string(&trial).unwrap();
    let restored: MeasuredTrial = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.elapsed_ns, 9_007_199_254_740_993);
    let summary = aggregate("x", Candidate::Offline, &[trial], false).unwrap();
    assert!(!summary.eligible);
    assert_eq!(summary.total_ns, 0);
}

#[test]
fn malformed_study_workloads_and_blocks_fail_without_rendering() {
    let mut workload = Workload::new("bad", 1, 16, Mode::Bursts, 16, 9);
    assert_eq!(check(&workload, Candidate::Offline, 0).outcome, "fail");
    workload.period = 0;
    assert_eq!(check(&workload, Candidate::Offline, 1).outcome, "fail");
}

#[test]
fn timed_validation_preserves_failed_samples_and_state_without_measuring() {
    let workload = Workload::new("synthetic-validation", 1, 32, Mode::Once, 128, 2);
    let expected = workload.expected();
    assert!(assess_trial_output(&workload, &expected, workload.expected_state()).is_none());
    let mut faulty = expected.clone();
    faulty[3] = f32::from_bits(0x7fc00042);
    let failure = assess_trial_output(&workload, &faulty, workload.expected_state()).unwrap();
    assert_eq!(failure.first_mismatch, Some(3));
    assert_eq!(failure.actual_samples, 32);
    assert_eq!(failure.sample_bits[3], 0x7fc00042);
    let mut state = workload.expected_state();
    state.output_frame = 99;
    assert!(assess_trial_output(&workload, &expected, state).is_some());
}
