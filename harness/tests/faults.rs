use audio_flow_harness::{contracts::*, offline::*};
#[derive(Clone, Copy)]
enum Fault {
    None,
    DroppedSeek,
    InclusiveLoop,
    PartialAtomic,
    Stale,
}
struct Broken {
    state: RenderState,
    fault: Fault,
}
impl Runtime for Broken {
    fn enqueue(&mut self, mut batch: Batch) -> Result<(), Error> {
        match self.fault {
            Fault::DroppedSeek => batch
                .commands
                .retain(|c| !matches!(c,Command::Seek(_,p) if *p<99)),
            Fault::InclusiveLoop => {
                for command in &mut batch.commands {
                    if let Command::SetLoop(_, Some((_, end))) = command {
                        *end += 1;
                    }
                }
            }
            Fault::PartialAtomic => {
                if batch.commands.contains(&Command::Seek(0, 99)) {
                    batch
                        .commands
                        .retain(|c| !matches!(c, Command::Seek(_, 99)));
                    self.state.enqueue(batch)?;
                    return Err(Error::InvalidSeek);
                }
            }
            _ => {}
        }
        self.state.enqueue(batch)
    }
    fn advance_revision(&mut self, revision: u64) -> Result<(), Error> {
        if matches!(self.fault, Fault::Stale) {
            Ok(())
        } else {
            self.state.advance_revision(revision)
        }
    }
    fn render(&mut self, frames: usize) -> Result<Vec<Vec<f32>>, Error> {
        self.state.render(frames)
    }
    fn observe(&self) -> Observation {
        self.state.observe()
    }
}
#[test]
fn literal_gates_pass_and_each_control_fault_is_detected() {
    let good = verify(RenderState::new);
    assert!(good.iter().all(|g| g.passed), "{good:#?}");
    for (fault, id) in [
        (Fault::DroppedSeek, "stop-seek"),
        (Fault::InclusiveLoop, "half-open-loop"),
        (Fault::PartialAtomic, "atomic-rejection"),
        (Fault::Stale, "stale-cancellation"),
    ] {
        let gates = verify(|program| Broken {
            state: RenderState::new(program),
            fault,
        });
        assert_eq!(gates.len(), good.len());
        assert!(
            !gates.iter().find(|g| g.id == id).unwrap().passed,
            "{gates:#?}"
        );
    }
    assert!(verify(|program| Broken {
        state: RenderState::new(program),
        fault: Fault::None
    })
    .iter()
    .all(|g| g.passed));
}
