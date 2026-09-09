use audio_flow_harness::{
    contracts,
    offline::*,
    study::{self, Candidate},
};
use std::sync::Arc;
fn program(flat: bool) -> Arc<Program> {
    let limits = Limits {
        pending_commands: 4,
        batch_commands: 2,
        commands_per_frame: 2,
        ..Limits::default()
    };
    let mut assets = Assets::new(limits).unwrap();
    assets.load(&[1., 2., 3., 4.], 48000).unwrap();
    let nodes = if flat {
        vec![
            Node::Player { asset: 0 },
            Node::Gain { input: 0, gain: 1. },
            Node::Sum { inputs: vec![1] },
        ]
    } else {
        vec![Node::Player { asset: 0 }]
    };
    let output = nodes.len() - 1;
    Arc::new(
        Graph {
            nodes,
            outputs: vec![("main".into(), output)],
        }
        .compile(&assets, limits)
        .unwrap(),
    )
}
fn batch(frame: u64, revision: u64, commands: Vec<Command>) -> Batch {
    Batch {
        frame,
        revision,
        commands,
    }
}
#[test]
fn default_promotes_supported_graph_and_falls_back() {
    assert_eq!(
        RenderState::new(program(true)).active_strategy(),
        RenderStrategy::FlatFused
    );
    assert_eq!(
        RenderState::new(program(false)).active_strategy(),
        RenderStrategy::Generic
    );
}
#[test]
fn counted_passes_literal_and_seeded_contracts_for_both_renderers() {
    for renderer in [RenderStrategy::Generic, RenderStrategy::FlatFused] {
        assert!(contracts::verify(|p| RenderState::new_with_strategies(
            p,
            renderer,
            EnqueueStrategy::Counted
        ))
        .iter()
        .all(|g| g.passed));
        for workload in study::workloads() {
            for block in [1, 64, 257] {
                assert_eq!(
                    study::check_with_strategies(
                        &workload,
                        Candidate::Offline,
                        block,
                        renderer,
                        EnqueueStrategy::Counted
                    )
                    .outcome,
                    "pass"
                );
            }
        }
    }
}
#[test]
fn budgets_recover_after_consumption_reclamation_and_revision() {
    for renderer in [RenderStrategy::Generic, RenderStrategy::FlatFused] {
        for enqueue in [EnqueueStrategy::Scan, EnqueueStrategy::Counted] {
            let mut s = RenderState::new_with_strategies(program(true), renderer, enqueue);
            s.enqueue(batch(2, 0, vec![Command::Seek(0, 2), Command::Start(0)]))
                .unwrap();
            s.enqueue(batch(0, 0, vec![Command::Start(0)])).unwrap();
            // Equal-frame batches retain acceptance order.
            s.enqueue(batch(0, 0, vec![Command::Stop(0)])).unwrap();
            let before = s.observe();
            assert_eq!(
                s.enqueue(batch(1, 0, vec![Command::Start(0)])),
                Err(Error::ResourceLimit)
            );
            assert_eq!(s.observe(), before);
            assert_eq!(s.render(1).unwrap()[0], vec![0.]);
            // Consumed commands no longer count, even before reclamation.
            assert_eq!(
                s.enqueue(batch(2, 0, vec![Command::Stop(0)])),
                Err(Error::ResourceLimit)
            );
            assert_eq!(
                s.enqueue(batch(1, 0, vec![Command::Seek(0, 5)])),
                Err(Error::InvalidSeek)
            );
            assert_eq!(
                s.enqueue(batch(1, 0, vec![Command::Start(1)])),
                Err(Error::UnknownPlayer)
            );
            assert_eq!(
                s.enqueue(batch(1, 0, vec![Command::SetLoop(0, Some((2, 2)))])),
                Err(Error::InvalidLoop)
            );
            s.enqueue(batch(1, 0, vec![Command::Start(0), Command::Stop(0)]))
                .unwrap();
            assert_eq!(s.render(2).unwrap()[0], vec![0., 3.]);
            s.enqueue(batch(3, 0, vec![Command::Seek(0, 0), Command::Start(0)]))
                .unwrap();
            s.enqueue(batch(4, 0, vec![Command::Stop(0), Command::Start(0)]))
                .unwrap();
            assert_eq!(s.advance_revision(0), Err(Error::StaleRevision));
            s.advance_revision(1).unwrap();
            assert_eq!(
                s.enqueue(batch(3, 0, vec![Command::Stop(0)])),
                Err(Error::StaleRevision)
            );
            assert_eq!(
                s.enqueue(batch(2, 1, vec![Command::Stop(0)])),
                Err(Error::PastEvent)
            );
            assert_eq!(s.enqueue(batch(3, 1, vec![])), Err(Error::EmptyBatch));
            s.enqueue(batch(3, 1, vec![Command::Seek(0, 0), Command::Start(0)]))
                .unwrap();
            s.enqueue(batch(4, 1, vec![Command::Stop(0), Command::Start(0)]))
                .unwrap();
            assert_eq!(s.render(2).unwrap()[0], vec![1., 2.]);
        }
    }
}
