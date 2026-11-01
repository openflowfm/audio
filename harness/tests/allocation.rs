use audio_flow_harness::offline::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};
struct Count;
thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; static CALLS: Cell<usize> = const { Cell::new(0) }; }
fn record() {
    ACTIVE.with(|active| {
        if active.get() {
            CALLS.with(|calls| calls.set(calls.get() + 1));
        }
    });
}
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record();
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        System.realloc(ptr, layout, size)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        System.alloc_zeroed(layout)
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;
#[test]
fn prepared_render_allocates_and_deallocates_nothing() {
    let mut assets = Assets::new(Limits::default()).unwrap();
    assets.load(&[1., 2., 3., 4.], 48000).unwrap();
    let program = Graph {
        nodes: vec![Node::Player { asset: 0 }],
        outputs: vec![("main".into(), 0)],
    }
    .compile(&assets, Limits::default())
    .unwrap();
    let mut state = RenderState::new(Arc::new(program));
    for frame in 0..32 {
        state
            .enqueue(Batch {
                frame,
                revision: 0,
                commands: vec![Command::Seek(0, 1), Command::Start(0)],
            })
            .unwrap();
    }
    let mut output = vec![vec![0.; 64]];
    CALLS.with(|calls| calls.set(0));
    ACTIVE.with(|active| active.set(true));
    let result = state.render_into(&mut output);
    ACTIVE.with(|active| active.set(false));
    assert!(result.is_ok());
    assert_eq!(CALLS.with(Cell::get), 0);
    assert_eq!(output[0][0], 2.);
}

#[test]
fn prepared_mixed_and_dense_control_workloads_do_not_allocate_during_render() {
    use audio_flow_harness::study::{Candidate, Mode, Prepared, Workload};
    for (voices, mode, period) in [(64, Mode::Loop, 128), (16, Mode::Bursts, 64)] {
        let workload = Workload::new("allocation-only", voices, 4096, mode, period, 23);
        let mut adapter =
            Prepared::new_with_strategy(&workload, Candidate::Offline, RenderStrategy::Generic)
                .unwrap();
        adapter.enqueue(workload.batches()).unwrap();
        let mut output = vec![vec![0.; 4096]];
        CALLS.with(|calls| calls.set(0));
        ACTIVE.with(|active| active.set(true));
        let result = adapter.render_into(&mut output);
        ACTIVE.with(|active| active.set(false));
        assert!(result.is_ok());
        assert_eq!(CALLS.with(Cell::get), 0);
        assert_eq!(output[0], workload.expected());
    }
}

#[test]
fn fused_flat_render_preserves_no_allocation_or_free_boundary() {
    use audio_flow_harness::study::{Candidate, Mode, Prepared, Workload};
    for (voices, mode, period) in [(64, Mode::Loop, 128), (16, Mode::Bursts, 64)] {
        let workload = Workload::new("fused-allocation-only", voices, 4096, mode, period, 23);
        let mut adapter = Prepared::new_with_strategies(
            &workload,
            Candidate::Offline,
            RenderStrategy::FlatFused,
            EnqueueStrategy::Counted,
        )
        .unwrap();
        if let Prepared::Offline(ref state) = adapter {
            assert_eq!(state.active_strategy(), RenderStrategy::FlatFused);
        }
        adapter.enqueue(workload.batches()).unwrap();
        let mut output = vec![vec![0.; 4096]];
        CALLS.with(|calls| calls.set(0));
        ACTIVE.with(|active| active.set(true));
        let result = adapter.render_into(&mut output);
        ACTIVE.with(|active| active.set(false));
        assert!(result.is_ok());
        assert_eq!(CALLS.with(Cell::get), 0);
        assert_eq!(output[0], workload.expected());
    }
}

#[test]
fn native_block_loop_render_does_not_allocate_or_free() {
    use audio_flow_harness::study::{Candidate, Mode, Prepared, Workload};
    for candidate in [Candidate::FundspBlock, Candidate::Oddio] {
        let w = Workload::new("native-allocation", 64, 4096, Mode::Loop, 128, 23);
        let mut adapter = Prepared::new(&w, candidate).unwrap();
        adapter.enqueue(w.batches()).unwrap();
        let mut output = vec![vec![0.; 4096]];
        CALLS.with(|calls| calls.set(0));
        ACTIVE.with(|active| active.set(true));
        let result = adapter.render_into(&mut output);
        ACTIVE.with(|active| active.set(false));
        assert!(result.is_ok());
        assert_eq!(CALLS.with(Cell::get), 0);
        assert_eq!(output[0], w.expected());
    }
}

#[test]
fn borrowed_callback_control_observation_and_prefix_do_not_allocate_or_free() {
    let mut assets = Assets::new(Limits::default()).unwrap();
    assets.load(&[1., 2., 3., 4.], 48000).unwrap();
    let program = Graph {
        nodes: vec![Node::Player { asset: 0 }],
        outputs: vec![("main".into(), 0)],
    }
    .compile(&assets, Limits::default())
    .unwrap();
    let mut state = RenderState::new(Arc::new(program));
    let mut output = vec![vec![0.; 8]];
    CALLS.with(|calls| calls.set(0));
    ACTIVE.with(|active| active.set(true));
    let applied = state.apply_immediate(
        0,
        &[
            Command::Seek(0, 1),
            Command::SetLoop(0, Some((1, 3))),
            Command::Start(0),
        ],
    );
    let rejected = state.apply_immediate(0, &[Command::Stop(0), Command::Seek(0, 5)]);
    let rendered = state.render_into_prefix(&mut output, 5);
    let observation = state.observe_ref();
    let frame = observation.output_frame;
    let position = observation.players[0].position;
    ACTIVE.with(|active| active.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert!(applied.is_ok());
    assert_eq!(rejected, Err(Error::InvalidSeek));
    assert!(rendered.is_ok());
    assert_eq!((frame, position), (5, 2));
    assert_eq!(output[0], [2., 3., 2., 3., 2., 0., 0., 0.]);
}
