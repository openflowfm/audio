use audio_flow_harness::offline::*;
use std::sync::Arc;
fn one(asset: &[f32]) -> Arc<Program> {
    let mut assets = Assets::new(Limits::default()).unwrap();
    assets.load(asset, 48000).unwrap();
    Graph {
        nodes: vec![Node::Player { asset: 0 }],
        outputs: vec![("main".into(), 0)],
    }
    .compile(&assets, Limits::default())
    .map(Arc::new)
    .unwrap()
}
fn render_parts(state: &mut RenderState, parts: &[usize]) -> Vec<f32> {
    parts
        .iter()
        .flat_map(|&n| state.render(n).unwrap().remove(0))
        .collect()
}
fn batch(frame: u64, commands: Vec<Command>) -> Batch {
    Batch {
        frame,
        revision: 0,
        commands,
    }
}
#[test]
fn literal_start_stop_seek_and_positions_are_partition_invariant() {
    for parts in [vec![8], vec![1; 8], vec![3, 2, 3]] {
        let mut state = RenderState::new(one(&[1., 2., 3., 4.]));
        state.enqueue(batch(1, vec![Command::Start(0)])).unwrap();
        state.enqueue(batch(3, vec![Command::Stop(0)])).unwrap();
        state
            .enqueue(batch(5, vec![Command::Seek(0, 0), Command::Start(0)]))
            .unwrap();
        assert_eq!(
            render_parts(&mut state, &parts),
            vec![0., 1., 2., 0., 0., 1., 2., 3.]
        );
        assert_eq!(state.observe().players[0].position, 3);
        assert_eq!(state.observe().players[0].status, Playback::Playing);
    }
}
#[test]
fn half_open_loop_oracle_and_end_boundary() {
    for parts in [vec![6], vec![1; 6], vec![1, 3, 2]] {
        let mut state = RenderState::new(one(&[0.125, 0.25, 0.5, 0.75]));
        state
            .enqueue(batch(
                0,
                vec![
                    Command::Seek(0, 1),
                    Command::SetLoop(0, Some((1, 3))),
                    Command::Start(0),
                ],
            ))
            .unwrap();
        assert_eq!(
            render_parts(&mut state, &parts),
            vec![0.25, 0.5, 0.25, 0.5, 0.25, 0.5]
        );
        assert_eq!(state.observe().players[0].position, 1);
    }
    let mut state = RenderState::new(one(&[1., 2.]));
    state.enqueue(batch(2, vec![Command::Start(0)])).unwrap();
    assert_eq!(state.render(2).unwrap()[0], vec![0., 0.]);
    assert_eq!(state.observe().pending_batches, 1);
    assert_eq!(state.render(3).unwrap()[0], vec![1., 2., 0.]);
}
#[test]
fn independent_players_gain_sum_and_named_outputs() {
    let mut assets = Assets::new(Limits::default()).unwrap();
    assets.load(&[1., 2.], 48000).unwrap();
    let program = Graph {
        nodes: vec![
            Node::Player { asset: 0 },
            Node::Player { asset: 0 },
            Node::Gain {
                input: 0,
                gain: 0.5,
            },
            Node::Gain {
                input: 1,
                gain: 0.25,
            },
            Node::Sum { inputs: vec![2, 3] },
        ],
        outputs: vec![("main".into(), 4), ("first".into(), 2)],
    }
    .compile(&assets, Limits::default())
    .unwrap();
    assert_eq!(program.output_names(), vec!["main", "first"]);
    let mut state = RenderState::new(Arc::new(program));
    state.enqueue(batch(0, vec![Command::Start(0)])).unwrap();
    state.enqueue(batch(1, vec![Command::Start(1)])).unwrap();
    assert_eq!(
        state.render(4).unwrap(),
        vec![vec![0.5, 1.25, 0.5, 0.], vec![0.5, 1., 0., 0.]]
    );
}
#[test]
fn invalid_atomic_batch_and_stale_revision_have_no_side_effects() {
    let mut state = RenderState::new(one(&[1., 2.]));
    let before = state.observe();
    assert_eq!(
        state.enqueue(batch(0, vec![Command::Start(0), Command::Seek(0, 3)])),
        Err(Error::InvalidSeek)
    );
    assert_eq!(state.observe(), before);
    state.enqueue(batch(1, vec![Command::Start(0)])).unwrap();
    state.advance_revision(1).unwrap();
    let after = state.observe();
    assert_eq!(
        state.enqueue(batch(2, vec![Command::Start(0)])),
        Err(Error::StaleRevision)
    );
    assert_eq!(state.observe(), after);
    assert_eq!(state.render(3).unwrap()[0], vec![0., 0., 0.]);
}
#[test]
fn same_frame_order_and_past_event_rejection() {
    let mut state = RenderState::new(one(&[1., 2.]));
    state
        .enqueue(batch(0, vec![Command::Start(0), Command::Stop(0)]))
        .unwrap();
    state.enqueue(batch(0, vec![Command::Start(0)])).unwrap();
    assert_eq!(state.render(1).unwrap()[0], vec![1.]);
    let before = state.observe();
    assert_eq!(
        state.enqueue(batch(0, vec![Command::Stop(0)])),
        Err(Error::PastEvent)
    );
    assert_eq!(state.observe(), before);
}
#[test]
fn malformed_graphs_resources_and_queue_limits_reject_cleanly() {
    let mut assets = Assets::new(Limits::default()).unwrap();
    assert_eq!(
        assets.load(&[f32::INFINITY], 48000),
        Err(Error::InvalidAsset)
    );
    assets.load(&[1.], 48000).unwrap();
    for graph in [
        Graph {
            nodes: vec![Node::Gain { input: 0, gain: 1. }],
            outputs: vec![("main".into(), 0)],
        },
        Graph {
            nodes: vec![Node::Player { asset: 9 }],
            outputs: vec![("main".into(), 0)],
        },
        Graph {
            nodes: vec![Node::Player { asset: 0 }],
            outputs: vec![("main".into(), 9)],
        },
        Graph {
            nodes: vec![Node::Player { asset: 0 }],
            outputs: vec![("x".into(), 0), ("x".into(), 0)],
        },
    ] {
        assert!(graph.compile(&assets, Limits::default()).is_err());
    }
    let limits = Limits {
        pending_commands: 1,
        ..Limits::default()
    };
    let program = Graph {
        nodes: vec![Node::Player { asset: 0 }],
        outputs: vec![("main".into(), 0)],
    }
    .compile(&assets, limits)
    .unwrap();
    let mut state = RenderState::new(Arc::new(program));
    state.enqueue(batch(1, vec![Command::Start(0)])).unwrap();
    let before = state.observe();
    assert_eq!(
        state.enqueue(batch(2, vec![Command::Stop(0)])),
        Err(Error::ResourceLimit)
    );
    assert_eq!(state.observe(), before);
    assert_eq!(state.render(4097), Err(Error::ResourceLimit));
    assert_eq!(state.observe(), before);
}

#[test]
fn zero_render_and_invalid_buffers_leave_pending_commands_untouched() {
    let mut state = RenderState::new(one(&[1., 2.]));
    state.enqueue(batch(0, vec![Command::Start(0)])).unwrap();
    let before = state.observe();
    assert_eq!(state.render(0).unwrap(), vec![Vec::<f32>::new()]);
    assert_eq!(state.observe(), before);
    assert_eq!(state.render_into(&mut []), Err(Error::InvalidBuffer));
    assert_eq!(state.observe(), before);
    assert_eq!(state.render(1).unwrap()[0], vec![1.]);
}

#[test]
fn bounds_rejections_and_repeated_cancellation_preserve_active_players() {
    let mut state = RenderState::new(one(&[1., 2., 3., 4.]));
    for commands in [
        vec![],
        vec![Command::Start(1)],
        vec![Command::Seek(0, 5)],
        vec![Command::SetLoop(0, Some((2, 2)))],
        vec![Command::SetLoop(0, Some((1, 5)))],
    ] {
        let before = state.observe();
        assert!(state.enqueue(batch(0, commands)).is_err());
        assert_eq!(state.observe(), before);
    }
    for revision in 0..20 {
        state
            .enqueue(Batch {
                frame: 100,
                revision,
                commands: vec![Command::Start(0)],
            })
            .unwrap();
        state.advance_revision(revision + 1).unwrap();
        let before = state.observe();
        assert_eq!(
            state.advance_revision(revision + 1),
            Err(Error::StaleRevision)
        );
        assert_eq!(state.observe(), before);
    }
    assert_eq!(state.render(101).unwrap()[0], vec![0.; 101]);
}
#[test]
fn shared_program_states_are_independent_and_seek_end_is_silent() {
    let program = one(&[1., 2.]);
    let mut first = RenderState::new(program.clone());
    let mut second = RenderState::new(program);
    first.enqueue(batch(0, vec![Command::Start(0)])).unwrap();
    assert_eq!(first.render(1).unwrap()[0], vec![1.]);
    assert_eq!(second.render(1).unwrap()[0], vec![0.]);
    first.enqueue(batch(1, vec![Command::Seek(0, 2)])).unwrap();
    assert_eq!(first.render(2).unwrap()[0], vec![0., 0.]);
    assert_eq!(first.observe().players[0].status, Playback::Ended);
}
