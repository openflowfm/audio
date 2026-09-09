//! Single-thread same-binary paired experiment; never allocator-instrumented.
use audio_flow_harness::{
    contracts,
    offline::{EnqueueStrategy, RenderState, RenderStrategy},
    results,
    study::{self, Candidate},
};
use std::path::PathBuf;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: dispatch-experiment EXISTING_OUTPUT_ROOT".into());
    }
    let root = PathBuf::from(&args[1]);
    let dir = results::create_run_dir(&root)?;
    let workloads = study::workloads();
    let strategies = [
        ("Generic", RenderStrategy::Generic, EnqueueStrategy::Scan),
        (
            "FlatFused",
            RenderStrategy::FlatFused,
            EnqueueStrategy::Scan,
        ),
        (
            "FlatFusedCounted",
            RenderStrategy::FlatFused,
            EnqueueStrategy::Counted,
        ),
    ];
    let mut gates = vec![];
    let mut checks = vec![];
    for (strategy, renderer, enqueue) in strategies {
        gates.push(serde_json::json!({"strategy":strategy,"gates":contracts::verify(|p|RenderState::new_with_strategies(p,renderer,enqueue))}));
        for w in &workloads {
            for block in [1, 64, 257] {
                checks.push(serde_json::json!({"strategy":strategy,"check":study::check_with_strategies(w,Candidate::Offline,block,renderer,enqueue)}));
            }
        }
    }
    let eligible = gates.iter().all(|g| {
        g["gates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["passed"] == true)
    }) && checks.iter().all(|c| c["check"]["outcome"] == "pass");
    let mut trials = vec![];
    let mut errors = vec![];
    let mut order = 0;
    if eligible {
        // Two independently retained rounds, each with 3 warmups + 12 measurements.
        for replicate in 0..2 {
            for w in &workloads {
                for round in 0..15 {
                    for index in 0..strategies.len() {
                        let (strategy, renderer, enqueue) =
                            strategies[(index + round + replicate) % strategies.len()];
                        match study::measure_with_strategies(w,Candidate::Offline,round.saturating_sub(3) as u64,order,renderer,enqueue) {
                Ok(trial)=>{if trial.conformance!="pass"{errors.push(serde_json::json!({"replicate":replicate,"strategy":strategy,"trial":trial}));}else if round>=3{trials.push(serde_json::json!({"replicate":replicate,"strategy":strategy,"trial":trial}));}},
                Err(error)=>errors.push(serde_json::json!({"replicate":replicate,"strategy":strategy,"workload":w.id,"error":error}))
            }
                        order += 1;
                    }
                }
            }
        }
    }
    let passed = eligible && errors.is_empty();
    let source = serde_json::json!({"schema_version":1,"kind":"dispatch-experiment","passed":passed,"gates":gates,"checks":checks,"errors":errors,"trials":trials,"workloads":workloads,
      "compiler":env!("HARNESS_RUSTC"),"profile":env!("HARNESS_PROFILE"),"target":env!("HARNESS_TARGET"),"opt_level":env!("HARNESS_OPT_LEVEL"),"rustflags":env!("HARNESS_CARGO_ENCODED_RUSTFLAGS"),"dependency_lock":include_str!("../../Cargo.lock"),
      "revision":results::command_text("git",&["rev-parse","HEAD"]),"dirty":results::command_text("git",&["status","--porcelain"]),
      "strategies":["Generic","FlatFused","FlatFusedCounted"],"replicates":2,"warmups_per_replicate":3,"repeats_per_replicate":12,"ordering":"same binary, per workload three strategies rotating each round and replicate; serial single-thread", 
      "timing_scope":"unchanged study measure: enqueue + preallocated render + identical output capture; fixture/events/validation outside timer", "limits":"FlatFused+Scan default for supported graphs with Generic fallback; Counted enqueue opt-in; no FMA, reassociation, SIMD, parallel rendering or event-span batching"});
    results::write_artifacts(
        &dir,
        vec![
            ("experiment.json".into(), results::json_bytes(&source)?),
            (
                "summary.json".into(),
                results::json_bytes(
                    &serde_json::json!({"schema_version":1,"kind":"dispatch-experiment","passed":passed,"retained_trials":source["trials"].as_array().unwrap().len()}),
                )?,
            ),
        ],
    )?;
    println!("{}", dir.display());
    if !passed {
        return Err("experiment gates or timed validation failed; evidence retained".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
