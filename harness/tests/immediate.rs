use audio_flow_harness::offline::*;
use std::sync::Arc;

fn state() -> RenderState {
    let limits = Limits::default();
    let mut assets = Assets::new(limits).unwrap();
    assets.load(&[1., 2., 3., 4.], 48000).unwrap();
    RenderState::new(Arc::new(
        Graph {
            nodes: vec![Node::Player { asset: 0 }],
            outputs: vec![("main".into(), 0)],
        }
        .compile(&assets, limits)
        .unwrap(),
    ))
}

#[test]
fn atomic_rejections_and_exact_seek_loop_pause() {
    let mut s = state();
    for (commands, error) in [
        (
            vec![Command::Start(0), Command::Seek(0, 5)],
            Error::InvalidSeek,
        ),
        (
            vec![Command::Start(0), Command::SetLoop(0, Some((2, 2)))],
            Error::InvalidLoop,
        ),
        (
            vec![Command::Start(0), Command::Stop(1)],
            Error::UnknownPlayer,
        ),
    ] {
        let before = s.observe();
        assert_eq!(s.apply_immediate(0, &commands), Err(error));
        assert_eq!(s.observe(), before);
    }
    assert_eq!(
        s.apply_immediate(1, &[Command::Start(0)]),
        Err(Error::StaleRevision)
    );
    assert_eq!(s.apply_immediate(0, &[]), Err(Error::EmptyBatch));
    assert_eq!(
        s.apply_immediate(0, &[Command::Start(0); 257]),
        Err(Error::ResourceLimit)
    );
    s.apply_immediate(
        0,
        &[
            Command::Seek(0, 1),
            Command::SetLoop(0, Some((1, 3))),
            Command::Start(0),
        ],
    )
    .unwrap();
    assert_eq!(s.render(5).unwrap()[0], [2., 3., 2., 3., 2.]);
    assert_eq!(s.observe_ref().players[0].position, 2);
    s.apply_immediate(0, &[Command::Stop(0), Command::Seek(0, 4)])
        .unwrap();
    assert_eq!(s.render(2).unwrap()[0], [0., 0.]);
    assert_eq!(s.observe_ref().output_frame, 7);
    assert_eq!(s.observe_ref().players[0].position, 4);
    s.apply_immediate(0, &[Command::Start(0)]).unwrap();
    assert_eq!(s.render(1).unwrap()[0], [2.]);
}

#[test]
fn immediate_precedes_pending_at_same_frame_and_prefix_retains_tail() {
    let mut s = state();
    s.enqueue(Batch {
        frame: 0,
        revision: 0,
        commands: vec![Command::Stop(0)],
    })
    .unwrap();
    s.apply_immediate(0, &[Command::Start(0)]).unwrap();
    let mut output = vec![vec![9.; 4]];
    s.render_into_prefix(&mut output, 1).unwrap();
    assert_eq!(output[0], [0., 9., 9., 9.]);
    assert_eq!(s.observe_ref().players[0].status, Playback::Stopped);
}
