use std::{
    process::Command,
    time::{Duration, Instant},
};
#[test]
fn profiles_preserve_gates_and_stay_out_of_speed_evidence() {
    for (candidate, workload, eligible) in [
        ("offline", "one-shot", true),
        ("direct", "loop-1", true),
        ("fundsp", "loop-1", true),
        ("offline", "bursts-16-dense", true),
        ("direct", "bursts-8", false),
    ] {
        let dir = audio_flow_harness::results::create_run_dir(&std::env::temp_dir()).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_audio-profile"))
            .args([dir.to_str().unwrap(), workload, candidate, "1", "1"])
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if dir.join("ready").exists() {
                std::fs::write(dir.join("go"), b"").unwrap();
                break;
            }
            if child.try_wait().unwrap().is_some() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(child.wait().unwrap().success(), eligible);
        let gates: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("gates.json")).unwrap()).unwrap();
        assert_eq!(gates["passed"], eligible);
        assert!(!dir.join("timings.jsonl").exists());
        if eligible {
            let profile: serde_json::Value =
                serde_json::from_slice(&std::fs::read(dir.join("profile.json")).unwrap()).unwrap();
            assert_eq!(profile["comparative_speed_eligible"], false);
            assert_eq!(profile["completed"], 1);
            assert_eq!(profile["phases"]["enqueue"]["calls"], 1);
            assert_eq!(
                profile["phases"]["render"]["calls"],
                profile["phases"]["capture"]["calls"]
            );
            assert_eq!(profile["phases"]["render"]["allocations"], 0);
        } else {
            assert!(!dir.join("profile.json").exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
