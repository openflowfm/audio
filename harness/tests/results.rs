use audio_flow_harness::{conformance::*, model::*, reference::DirectLoop, results::*};

#[test]
fn synthetic_integer_timings_aggregate_losslessly_and_gate_failures() {
    let trials = vec![
        Trial::synthetic(0, 10, 9_007_199_254_740_993),
        Trial::synthetic(1, 10, 9_007_199_254_740_995),
    ];
    let summary = summarize(Outcome::Pass, &trials).unwrap();
    assert_eq!(summary.total_ns, 18_014_398_509_481_988);
    assert_eq!(summary.total_frames, 20);
    assert_eq!(summary.min_ns, Some(9_007_199_254_740_993));
    assert_eq!(summary.max_ns, Some(9_007_199_254_740_995));
    assert!(
        !summary.comparative_speed_eligible,
        "synthetic data is never speed evidence"
    );
    let json = serde_json::to_string(&trials).unwrap();
    assert!(json.contains("9007199254740993"));
    assert_eq!(serde_json::from_str::<Vec<Trial>>(&json).unwrap(), trials);
    assert!(summarize(Outcome::Fail, &trials).is_err());
    assert!(summarize(Outcome::CapabilityGap, &trials).is_err());
    assert!(summarize(
        Outcome::Pass,
        &[Trial::synthetic(0, 1, u64::MAX), Trial::synthetic(1, 1, 1)]
    )
    .is_err());
}
#[test]
fn outputs_are_unique_complete_and_never_overwrite() {
    let root = std::env::temp_dir();
    let one = create_run_dir(&root).unwrap();
    let two = create_run_dir(&root).unwrap();
    assert_ne!(one, two);
    let report = run_suite(&Fixture::first().unwrap(), DirectLoop::default);
    write_run(&one, &report, &[], false).unwrap();
    for name in [
        "manifest.json",
        "conformance.json",
        "timings.jsonl",
        "summary.json",
    ] {
        assert!(one.join(name).is_file());
    }
    let original = std::fs::read(one.join("conformance.json")).unwrap();
    assert!(write_run(&one, &report, &[], false).is_err());
    assert_eq!(
        std::fs::read(one.join("conformance.json")).unwrap(),
        original
    );
    let json: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["cases"][0]["blocks"][0]["requested_frames"], 10);
}
#[test]
fn refuse_source_tree_results_and_bad_trial_sets() {
    assert!(create_run_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).is_err());
    assert!(summarize(Outcome::Pass, &[Trial::synthetic(0, 0, 1)]).is_err());
    assert!(summarize(Outcome::Pass, &[Trial::synthetic(1, 10, 1)]).is_err());
}

#[test]
fn incomplete_and_corrupted_runs_never_look_complete() {
    let dir = create_run_dir(&std::env::temp_dir()).unwrap();
    assert!(!is_complete(&dir));
    // Deliberate invalid second artifact simulates a write failure after the first succeeds.
    assert!(write_artifacts(
        &dir,
        vec![
            ("manifest.json".into(), b"{}".to_vec()),
            ("../bad".into(), vec![])
        ]
    )
    .is_err());
    assert!(dir.join("manifest.json").exists());
    assert!(!is_complete(&dir));
    let dir = create_run_dir(&std::env::temp_dir()).unwrap();
    let report = run_suite(&Fixture::first().unwrap(), DirectLoop::default);
    write_run(&dir, &report, &[], false).unwrap();
    assert!(is_complete(&dir));
    std::fs::write(dir.join("summary.json"), b"{}").unwrap();
    assert!(!is_complete(&dir));
}
