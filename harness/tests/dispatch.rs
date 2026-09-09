use audio_flow_harness::{
    contracts,
    offline::*,
    study::{self, Candidate},
};
use std::sync::Arc;
fn compile(graph: Graph, samples: &[&[f32]]) -> Arc<Program> {
    let mut assets = Assets::new(Limits::default()).unwrap();
    for samples in samples {
        assets.load(samples, 48000).unwrap();
    }
    Arc::new(graph.compile(&assets, Limits::default()).unwrap())
}
fn flat_one() -> Arc<Program> {
    compile(
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain { input: 0, gain: 1. },
                Node::Sum { inputs: vec![1] },
            ],
            outputs: vec![("main".into(), 2)],
        },
        &[&[1., 2., 3., 4.]],
    )
}
fn batch(frame: u64, commands: Vec<Command>) -> Batch {
    Batch {
        frame,
        revision: 0,
        commands,
    }
}
fn same_render(a: &mut RenderState, b: &mut RenderState, n: usize) -> Vec<Vec<f32>> {
    let x = a.render(n).unwrap();
    let y = b.render(n).unwrap();
    let bits = |v: &Vec<Vec<f32>>| {
        v.iter()
            .map(|v| v.iter().map(|s| s.to_bits()).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    assert_eq!(bits(&x), bits(&y));
    assert_eq!(a.observe(), b.observe());
    x
}
#[test]
fn literal_gates_and_all_seeded_workloads_pass_both_paths() {
    for strategy in [RenderStrategy::Generic, RenderStrategy::FlatFused] {
        assert!(
            contracts::verify(|p| RenderState::new_with_strategy(p, strategy))
                .iter()
                .all(|g| g.passed)
        );
        for w in study::workloads() {
            let prepared =
                study::Prepared::new_with_strategy(&w, Candidate::Offline, strategy).unwrap();
            if let study::Prepared::Offline(s) = prepared {
                assert_eq!(s.active_strategy(), strategy);
            }
            for block in [1, 64, 257] {
                assert_eq!(
                    study::check_with_strategy(&w, Candidate::Offline, block, strategy).outcome,
                    "pass"
                );
            }
        }
    }
}
#[test]
fn flat_literal_stop_seek_loop_and_exact_block_end() {
    for strategy in [RenderStrategy::Generic, RenderStrategy::FlatFused] {
        for parts in [vec![8], vec![1; 8], vec![3, 2, 3]] {
            let mut s = RenderState::new_with_strategy(flat_one(), strategy);
            for b in [
                batch(1, vec![Command::Start(0)]),
                batch(3, vec![Command::Stop(0)]),
                batch(5, vec![Command::Seek(0, 0), Command::Start(0)]),
            ] {
                s.enqueue(b).unwrap();
            }
            let rendered: Vec<_> = parts
                .into_iter()
                .flat_map(|n| s.render(n).unwrap().remove(0))
                .collect();
            assert_eq!(rendered, vec![0., 1., 2., 0., 0., 1., 2., 3.]);
        }
        let mut s = RenderState::new_with_strategy(flat_one(), strategy);
        s.enqueue(batch(
            2,
            vec![
                Command::Seek(0, 1),
                Command::SetLoop(0, Some((1, 3))),
                Command::Start(0),
            ],
        ))
        .unwrap();
        assert_eq!(s.render(2).unwrap()[0], vec![0., 0.]);
        assert_eq!(s.observe().pending_batches, 1);
        assert_eq!(s.render(6).unwrap()[0], vec![2., 3., 2., 3., 2., 3.]);
    }
}
#[test]
fn fused_batches_revisions_rejections_and_buffer_boundaries_match_control() {
    let p = flat_one();
    let mut a = RenderState::new_with_strategy(p.clone(), RenderStrategy::Generic);
    let mut b = RenderState::new_with_strategy(p, RenderStrategy::FlatFused);
    for command in [
        batch(8, vec![Command::Start(0)]),
        batch(0, vec![Command::Seek(0, 4), Command::Start(0)]),
        batch(
            2,
            vec![Command::SetLoop(0, Some((1, 3))), Command::Start(0)],
        ),
        batch(2, vec![Command::Stop(0), Command::Start(0)]),
        batch(5, vec![Command::SetLoop(0, None), Command::Seek(0, 0)]),
    ] {
        assert_eq!(a.enqueue(command.clone()), b.enqueue(command));
    }
    let before = b.observe();
    assert_eq!(
        b.enqueue(batch(1, vec![Command::Start(0), Command::Seek(0, 99)])),
        Err(Error::InvalidSeek)
    );
    assert_eq!(b.observe(), before);
    assert_eq!(b.render_into(&mut []), Err(Error::InvalidBuffer));
    assert_eq!(b.observe(), before);
    same_render(&mut a, &mut b, 0);
    same_render(&mut a, &mut b, 2);
    same_render(&mut a, &mut b, 3);
    assert_eq!(a.advance_revision(1), b.advance_revision(1));
    assert_eq!(
        a.enqueue(batch(5, vec![Command::Start(0)])),
        b.enqueue(batch(5, vec![Command::Start(0)]))
    );
    let new = Batch {
        frame: 5,
        revision: 1,
        commands: vec![Command::Stop(0), Command::Seek(0, 3), Command::Start(0)],
    };
    a.enqueue(new.clone()).unwrap();
    b.enqueue(new).unwrap();
    same_render(&mut a, &mut b, 1);
    same_render(&mut a, &mut b, 6);
}
#[test]
fn unsupported_topologies_fall_back_without_skipping_unused_players() {
    let graphs = vec![
        Graph {
            nodes: vec![Node::Player { asset: 0 }],
            outputs: vec![("x".into(), 0)],
        },
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain {
                    input: 0,
                    gain: 0.5,
                },
                Node::Sum { inputs: vec![1] },
            ],
            outputs: vec![("x".into(), 2), ("dry".into(), 0)],
        },
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain {
                    input: 0,
                    gain: 0.5,
                },
                Node::Gain {
                    input: 0,
                    gain: 0.25,
                },
                Node::Sum { inputs: vec![1, 2] },
            ],
            outputs: vec![("x".into(), 3)],
        },
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain {
                    input: 0,
                    gain: 0.5,
                },
                Node::Sum { inputs: vec![1, 1] },
            ],
            outputs: vec![("x".into(), 2)],
        },
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain {
                    input: 0,
                    gain: 0.5,
                },
                Node::Sum { inputs: vec![1] },
                Node::Player { asset: 0 },
            ],
            outputs: vec![("x".into(), 2)],
        },
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain {
                    input: 0,
                    gain: 0.5,
                },
                Node::Gain {
                    input: 1,
                    gain: 0.25,
                },
                Node::Sum { inputs: vec![2] },
            ],
            outputs: vec![("x".into(), 3)],
        },
    ];
    for g in graphs {
        let p = compile(g, &[&[1., 2., 3.]]);
        assert!(!p.supports_flat_fused());
        let count = p.player_count();
        let mut a = RenderState::new(p.clone());
        let mut b = RenderState::new_with_strategy(p, RenderStrategy::FlatFused);
        assert_eq!(b.active_strategy(), RenderStrategy::Generic);
        for s in [&mut a, &mut b] {
            s.enqueue(batch(0, (0..count).map(Command::Start).collect()))
                .unwrap();
        }
        same_render(&mut a, &mut b, 2);
        same_render(&mut a, &mut b, 5);
    }
}
#[test]
fn nonassociative_sum_order_and_signed_zero_are_preserved() {
    let graph = |order| Graph {
        nodes: vec![
            Node::Player { asset: 0 },
            Node::Gain { input: 0, gain: 1. },
            Node::Player { asset: 1 },
            Node::Gain { input: 2, gain: 1. },
            Node::Player { asset: 2 },
            Node::Gain { input: 4, gain: 1. },
            Node::Sum { inputs: order },
        ],
        outputs: vec![("main".into(), 6)],
    };
    for (order, expected) in [(vec![1, 3, 5], 3.25), (vec![1, 5, 3], 0.)] {
        let p = compile(graph(order), &[&[1e20, -0.], &[-1e20, 0.], &[3.25, -0.]]);
        assert!(p.supports_flat_fused());
        let mut a = RenderState::new(p.clone());
        let mut b = RenderState::new_with_strategy(p, RenderStrategy::FlatFused);
        for s in [&mut a, &mut b] {
            s.enqueue(batch(
                0,
                vec![Command::Start(0), Command::Start(1), Command::Start(2)],
            ))
            .unwrap();
        }
        assert_eq!(same_render(&mut a, &mut b, 3)[0][0], expected);
    }
}
#[test]
fn multiply_and_add_keep_separate_f32_rounding() {
    let a = f32::from_bits(1f32.to_bits() + 1);
    let gain = 1. - f32::EPSILON;
    assert_ne!(a.mul_add(gain, -1.), 0.);
    let p = compile(
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain { input: 0, gain: 1. },
                Node::Player { asset: 1 },
                Node::Gain { input: 2, gain },
                Node::Sum { inputs: vec![1, 3] },
            ],
            outputs: vec![("main".into(), 4)],
        },
        &[&[-1.], &[a]],
    );
    for strategy in [RenderStrategy::Generic, RenderStrategy::FlatFused] {
        let mut s = RenderState::new_with_strategy(p.clone(), strategy);
        s.enqueue(batch(0, vec![Command::Start(0), Command::Start(1)]))
            .unwrap();
        assert_eq!(s.render(1).unwrap()[0][0].to_bits(), 0f32.to_bits());
    }
}
