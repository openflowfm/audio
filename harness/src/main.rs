use audio_flow_harness::{
    conformance::{run_suite, Outcome},
    model::Fixture,
    reference::DirectLoop,
    results::{create_run_dir, write_run, Summary},
};
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let mut root = std::env::temp_dir();
    let mut study = false;
    let mut save_pcm = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output-root" => {
                root = PathBuf::from(
                    args.next()
                        .ok_or("--output-root requires an existing directory")?,
                )
            }
            "--study" => study = true,
            "--save-failed-pcm" => save_pcm = true,
            "--help" => {
                println!("audio-flow-harness [--output-root EXISTING_DIRECTORY] [--study] [--save-failed-pcm]\nDefault: conformance only. --study runs the bounded offline comparison.");
                return Ok(true);
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if study {
        let dir = audio_flow_harness::study::run(&root)?;
        println!("Offline study results: {}", dir.display());
        let data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("conformance.json"))?)?;
        return Ok(data["execution_errors"]
            .as_array()
            .is_some_and(|e| e.is_empty())
            && data["checks"]
                .as_array()
                .is_some_and(|cs| cs.iter().all(|c| c["outcome"] != "fail"))
            && data["literal_offline"]
                .as_array()
                .is_some_and(|cs| cs.iter().all(|c| c["passed"] == true))
            && ["first_direct", "first_fundsp"].iter().all(|key| {
                data[key]["cases"]
                    .as_array()
                    .is_some_and(|cs| cs.iter().all(|c| c["outcome"] == "pass"))
            }));
    }
    let fixture = Fixture::first()?;
    let report = run_suite(&fixture, DirectLoop::default);
    let mut report = report;
    report.adapter = "direct-loop".into();
    report.scheduling = "reference-native-per-frame".into();
    let dir = create_run_dir(&root)?;
    let trials = Vec::new();
    write_run(&dir, &report, &trials, save_pcm)?;
    // Human output is derived from persisted results, never a second aggregation.
    let stored: Summary = serde_json::from_slice(&std::fs::read(dir.join("summary.json"))?)?;
    println!(
        "Conformance: {:?}; timing smoke trials: {}\nResults: {}\n{}",
        stored.conformance,
        stored.trials,
        dir.display(),
        stored.limitation
    );
    Ok(stored.conformance == Outcome::Pass)
}
