use crate::output::*;
use audio_flow_harness::offline::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};
struct Guard;
thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; static CALLS: Cell<usize> = const { Cell::new(0) }; }
fn record() {
    ACTIVE.with(|a| {
        if a.get() {
            CALLS.with(|c| c.set(c.get() + 1));
        }
    });
}
unsafe impl GlobalAlloc for Guard {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        record();
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        record();
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        record();
        System.realloc(p, l, n)
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        record();
        System.alloc_zeroed(l)
    }
}
#[global_allocator]
static ALLOCATOR: Guard = Guard;
pub(crate) fn assert_no_allocations(f: impl FnOnce()) {
    CALLS.with(|c| c.set(0));
    ACTIVE.with(|a| a.set(true));
    f();
    ACTIVE.with(|a| a.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}
fn program() -> Arc<Program> {
    let limits = Limits::default();
    let mut assets = Assets::new(limits).unwrap();
    assets.load(&[0.25, -0.5, 0.75, -1.], 48000).unwrap();
    Arc::new(
        Graph {
            nodes: vec![
                Node::Player { asset: 0 },
                Node::Gain {
                    input: 0,
                    gain: 0.5,
                },
            ],
            outputs: vec![("left".into(), 0), ("half".into(), 1)],
        }
        .compile(&assets, limits)
        .unwrap(),
    )
}
fn prepare(chunk: usize) -> (Controller, Callback) {
    Callback::prepare(
        program(),
        48000,
        "f32",
        4,
        chunk,
        &[("left".into(), 2), ("half".into(), 0)],
    )
    .unwrap()
}
fn send(c: &mut Controller, commands: &[Command]) {
    c.submit(ControlBatch::new(0, commands).unwrap()).unwrap();
}
#[test]
fn literal_channels_partition_invariance_and_no_allocation_or_free() {
    let expected = [
        0.125, 0., 0.25, 0., -0.25, 0., -0.5, 0., 0.375, 0., 0.75, 0., -0.5, 0., -1., 0., 0.125,
        0., 0.25, 0., -0.25, 0., -0.5, 0., 0.375, 0., 0.75, 0.,
    ];
    for (chunk, partitions) in [(1, vec![7]), (3, vec![1, 2, 4]), (4, vec![2, 1, 1, 3])] {
        let (mut c, mut callback) = prepare(chunk);
        send(
            &mut c,
            &[Command::SetLoop(0, Some((0, 4))), Command::Start(0)],
        );
        let mut output = [9.; 28];
        let mut offset = 0;
        CALLS.with(|c| c.set(0));
        ACTIVE.with(|a| a.set(true));
        for frames in partitions.iter().copied() {
            callback
                .process(&mut output[offset..offset + frames * 4])
                .unwrap();
            offset += frames * 4;
        }
        let frame = callback.snapshot().output_frame;
        let position = callback.snapshot().players[0].position;
        ACTIVE.with(|a| a.set(false));
        assert_eq!(CALLS.with(Cell::get), 0);
        assert_eq!(output, expected);
        assert_eq!((frame, position), (7, 3));
    }
}
#[test]
fn bounded_queue_atomic_rejections_pause_seek_and_readback() {
    let (mut c, mut callback) = prepare(2);
    CALLS.with(|c| c.set(0));
    ACTIVE.with(|a| a.set(true));
    send(&mut c, &[Command::Start(0), Command::Seek(0, 5)]);
    let mut output = [9.; 8];
    callback.process(&mut output).unwrap();
    assert_eq!(output, [0.; 8]);
    assert_eq!(
        callback.diagnostics.last_rejection,
        Some(Error::InvalidSeek)
    );
    send(&mut c, &[Command::Seek(0, 1), Command::Start(0)]);
    callback.process(&mut output).unwrap();
    assert_eq!(output, [-0.25, 0., -0.5, 0., 0.375, 0., 0.75, 0.]);
    send(&mut c, &[Command::Stop(0)]);
    callback.process(&mut output).unwrap();
    assert_eq!(output, [0.; 8]);
    assert_eq!(callback.snapshot().output_frame, 6);
    assert_eq!(callback.snapshot().players[0].position, 3);
    for _ in 0..QUEUE_BATCHES {
        send(&mut c, &[Command::Stop(0)]);
    }
    assert!(c
        .submit(ControlBatch::new(0, &[Command::Start(0)]).unwrap())
        .is_err());
    assert_eq!(c.queue_full, 1);
    let applied = callback.diagnostics.applied_batches;
    callback.process(&mut output).unwrap();
    assert_eq!(
        callback.diagnostics.applied_batches - applied,
        BATCHES_PER_CALLBACK as u64
    );
    ACTIVE.with(|a| a.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}
#[test]
fn configuration_and_span_rejections() {
    for (rate, format, channels, chunk, routes) in [
        (44100, "f32", 4, 2, vec![("left".into(), 0)]),
        (48000, "i16", 4, 2, vec![("left".into(), 0)]),
        (48000, "f32", 4, 0, vec![("left".into(), 0)]),
        (48000, "f32", 4, 2, vec![("absent".into(), 0)]),
        (48000, "f32", 4, 2, vec![("left".into(), 4)]),
        (
            48000,
            "f32",
            4,
            2,
            vec![("left".into(), 0), ("half".into(), 0)],
        ),
        (
            48000,
            "f32",
            4,
            2,
            vec![("left".into(), 0), ("left".into(), 1)],
        ),
    ] {
        assert!(Callback::prepare(program(), rate, format, channels, chunk, &routes).is_err());
    }
    let (mut c, mut callback) = prepare(2);
    send(&mut c, &[Command::Start(0)]);
    assert_eq!(
        callback.process(&mut [9.; 3]),
        Err(OutputError::InvalidBuffer)
    );
    callback.process(&mut []).unwrap();
    assert_eq!(callback.snapshot().output_frame, 0);
    assert_eq!(callback.diagnostics.applied_batches, 0);
}
