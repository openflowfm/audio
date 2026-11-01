//! Simulated native callback seam. Preparation and destruction belong off callback.
//! No device streams are opened by this module.
use audio_flow_harness::offline::{Command, Error, ObservationRef, Program, RenderState};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::{sync::Arc, time::Instant};

pub const BATCH_COMMANDS: usize = 16;
pub const QUEUE_BATCHES: usize = 32;
pub const BATCHES_PER_CALLBACK: usize = 4;

/// Copy-only queue payload: no heap storage can be reclaimed when it is consumed.
#[derive(Clone, Copy, Debug)]
pub struct ControlBatch {
    revision: u64,
    len: usize,
    commands: [Command; BATCH_COMMANDS],
}
impl ControlBatch {
    pub fn new(revision: u64, commands: &[Command]) -> Result<Self, Error> {
        if commands.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if commands.len() > BATCH_COMMANDS {
            return Err(Error::ResourceLimit);
        }
        let mut batch = Self {
            revision,
            len: commands.len(),
            commands: [Command::Stop(0); BATCH_COMMANDS],
        };
        batch.commands[..commands.len()].copy_from_slice(commands);
        Ok(batch)
    }
}

pub struct Controller {
    producer: Producer<ControlBatch>,
    pub queue_full: u64,
}
impl Controller {
    /// Single producer, bounded nonblocking delivery. Full queues never overwrite commands.
    pub fn submit(&mut self, batch: ControlBatch) -> Result<(), ControlBatch> {
        self.producer.push(batch).map_err(|PushError::Full(batch)| {
            self.queue_full = self.queue_full.saturating_add(1);
            batch
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputError {
    UnsupportedFormat,
    InvalidMapping,
    InvalidChunk,
    InvalidBuffer,
    Render(Error),
}

#[derive(Default, Debug)]
pub struct Diagnostics {
    pub callbacks: u64,
    pub applied_batches: u64,
    pub rejected_batches: u64,
    pub last_rejection: Option<Error>,
    pub deadline_misses: u64,
    pub max_callback_nanos: u64,
}

pub struct Callback {
    state: RenderState,
    planar: Vec<Vec<f32>>,
    routes: Vec<(usize, usize)>,
    channels: usize,
    chunk: usize,
    consumer: Consumer<ControlBatch>,
    pub diagnostics: Diagnostics,
}
impl Callback {
    /// Preparation only. Each graph output and physical index may occur at most once.
    /// Unmapped physical channels are silent; no inferred routing or downmixing.
    pub fn prepare(
        program: Arc<Program>,
        rate: u32,
        format: &str,
        channels: usize,
        chunk: usize,
        mappings: &[(String, usize)],
    ) -> Result<(Controller, Self), OutputError> {
        if rate != 48000 || format != "f32" {
            return Err(OutputError::UnsupportedFormat);
        }
        if channels == 0 || channels > 128 || mappings.is_empty() {
            return Err(OutputError::InvalidMapping);
        }
        if chunk == 0 || chunk > program.block_frame_limit() {
            return Err(OutputError::InvalidChunk);
        }
        let names = program.output_names();
        let mut routes = Vec::new();
        for (name, physical) in mappings {
            let output = names
                .iter()
                .position(|n| *n == name)
                .ok_or(OutputError::InvalidMapping)?;
            if *physical >= channels || routes.iter().any(|&(o, p)| o == output || p == *physical) {
                return Err(OutputError::InvalidMapping);
            }
            routes.push((output, *physical));
        }
        let planar = vec![vec![0.; chunk]; program.output_count()];
        let (producer, consumer) = RingBuffer::new(QUEUE_BATCHES);
        Ok((
            Controller {
                producer,
                queue_full: 0,
            },
            Self {
                state: RenderState::new(program),
                planar,
                routes,
                channels,
                chunk,
                consumer,
                diagnostics: Diagnostics::default(),
            },
        ))
    }
    pub fn snapshot(&self) -> ObservationRef<'_> {
        self.state.observe_ref()
    }

    /// Commands are applied at the beginning of a nonempty device callback, not per chunk.
    /// Malformed spans are silenced and rejected without consuming commands or advancing time.
    pub fn process(&mut self, interleaved: &mut [f32]) -> Result<(), OutputError> {
        let started = Instant::now();
        interleaved.fill(0.);
        if interleaved.len() % self.channels != 0 {
            return Err(OutputError::InvalidBuffer);
        }
        let frames = interleaved.len() / self.channels;
        if frames == 0 {
            return Ok(());
        }
        self.snapshot()
            .output_frame
            .checked_add(frames as u64)
            .ok_or(OutputError::Render(Error::FrameOverflow))?;
        self.diagnostics.callbacks = self.diagnostics.callbacks.saturating_add(1);
        for _ in 0..BATCHES_PER_CALLBACK {
            let Ok(batch) = self.consumer.pop() else {
                break;
            };
            match self
                .state
                .apply_immediate(batch.revision, &batch.commands[..batch.len])
            {
                Ok(()) => {
                    self.diagnostics.applied_batches =
                        self.diagnostics.applied_batches.saturating_add(1)
                }
                Err(error) => {
                    self.diagnostics.rejected_batches =
                        self.diagnostics.rejected_batches.saturating_add(1);
                    self.diagnostics.last_rejection = Some(error);
                }
            }
        }
        for destination in interleaved.chunks_mut(self.chunk * self.channels) {
            let count = destination.len() / self.channels;
            self.state
                .render_into_prefix(&mut self.planar, count)
                .map_err(OutputError::Render)?;
            for &(output, physical) in &self.routes {
                for frame in 0..count {
                    destination[frame * self.channels + physical] = self.planar[output][frame];
                }
            }
        }
        let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        self.diagnostics.max_callback_nanos = self.diagnostics.max_callback_nanos.max(elapsed);
        if elapsed as u128 > frames as u128 * 1_000_000_000 / 48000 {
            self.diagnostics.deadline_misses = self.diagnostics.deadline_misses.saturating_add(1);
        }
        Ok(())
    }
}
