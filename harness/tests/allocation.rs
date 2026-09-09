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
