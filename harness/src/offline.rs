//! Small offline graph library. No harness oracle, device, transport or application policy.
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Limits {
    pub assets: usize,
    pub asset_frames: usize,
    pub nodes: usize,
    pub players: usize,
    pub outputs: usize,
    pub edges: usize,
    pub pending_commands: usize,
    pub batch_commands: usize,
    pub commands_per_frame: usize,
    pub block_frames: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            assets: 128,
            asset_frames: 1_048_576,
            nodes: 1024,
            players: 128,
            outputs: 16,
            edges: 4096,
            pending_commands: 4096,
            batch_commands: 256,
            commands_per_frame: 256,
            block_frames: 4096,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self, Error> {
        let max = Self::default();
        let a = [
            self.assets,
            self.asset_frames,
            self.nodes,
            self.players,
            self.outputs,
            self.edges,
            self.pending_commands,
            self.batch_commands,
            self.commands_per_frame,
            self.block_frames,
        ];
        let b = [
            max.assets,
            max.asset_frames,
            max.nodes,
            max.players,
            max.outputs,
            max.edges,
            max.pending_commands,
            max.batch_commands,
            max.commands_per_frame,
            max.block_frames,
        ];
        if a.iter().zip(b).any(|(&x, y)| x == 0 || x > y) {
            Err(Error::ResourceLimit)
        } else {
            Ok(self)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Error {
    InvalidAsset,
    UnsupportedFormat,
    InvalidGraph,
    Cycle,
    ResourceLimit,
    UnknownPlayer,
    InvalidSeek,
    InvalidLoop,
    PastEvent,
    StaleRevision,
    EmptyBatch,
    InvalidBuffer,
    FrameOverflow,
}
#[derive(Clone)]
pub struct Assets {
    samples: Vec<Arc<[f32]>>,
    frames: usize,
    limits: Limits,
}
impl Assets {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        Ok(Self {
            samples: vec![],
            frames: 0,
            limits: limits.validate()?,
        })
    }
    pub fn load(&mut self, samples: &[f32], rate: u32) -> Result<usize, Error> {
        if samples.is_empty() || samples.iter().any(|x| !x.is_finite()) {
            return Err(Error::InvalidAsset);
        }
        if rate != 48000 {
            return Err(Error::UnsupportedFormat);
        }
        if self.samples.len() == self.limits.assets
            || samples.len() > self.limits.asset_frames.saturating_sub(self.frames)
        {
            return Err(Error::ResourceLimit);
        }
        let id = self.samples.len();
        self.samples.push(Arc::from(samples));
        self.frames += samples.len();
        Ok(id)
    }
    pub fn sample_bytes(&self) -> usize {
        self.frames * std::mem::size_of::<f32>()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Node {
    Player { asset: usize },
    Gain { input: usize, gain: f32 },
    Sum { inputs: Vec<usize> },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub outputs: Vec<(String, usize)>,
}
enum Op {
    Player(usize),
    Gain(usize, f32),
    Sum(Vec<usize>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RenderStrategy {
    Generic,
    #[default]
    FlatFused,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EnqueueStrategy {
    #[default]
    Scan,
    Counted,
}
struct FlatVoice {
    player: usize,
    gain: f32,
}
pub struct Program {
    ops: Vec<(usize, Op)>,
    flat: Option<Vec<FlatVoice>>,
    assets: Vec<Arc<[f32]>>,
    outputs: Vec<(String, usize)>,
    nodes: usize,
    limits: Limits,
}
impl Graph {
    pub fn compile(&self, assets: &Assets, limits: Limits) -> Result<Program, Error> {
        let limits = limits.validate()?;
        if self.nodes.is_empty()
            || self.nodes.len() > limits.nodes
            || self.outputs.is_empty()
            || self.outputs.len() > limits.outputs
            || assets.samples.len() > limits.assets
            || assets.frames > limits.asset_frames
        {
            return Err(Error::ResourceLimit);
        }
        let mut names = HashSet::new();
        for (name, node) in &self.outputs {
            if name.is_empty() || !names.insert(name) || *node >= self.nodes.len() {
                return Err(Error::InvalidGraph);
            }
        }
        let mut dependencies = Vec::with_capacity(self.nodes.len());
        let mut player_assets = vec![];
        let mut player_ids = vec![None; self.nodes.len()];
        let mut edges = 0;
        for (id, node) in self.nodes.iter().enumerate() {
            let inputs = match node {
                Node::Player { asset } => {
                    let data = assets.samples.get(*asset).ok_or(Error::InvalidGraph)?;
                    if player_assets.len() == limits.players {
                        return Err(Error::ResourceLimit);
                    }
                    player_ids[id] = Some(player_assets.len());
                    player_assets.push(data.clone());
                    vec![]
                }
                Node::Gain { input, gain } => {
                    if !gain.is_finite() {
                        return Err(Error::InvalidGraph);
                    }
                    vec![*input]
                }
                Node::Sum { inputs } => {
                    if inputs.is_empty() {
                        return Err(Error::InvalidGraph);
                    }
                    inputs.clone()
                }
            };
            edges += inputs.len();
            if edges > limits.edges {
                return Err(Error::ResourceLimit);
            }
            if inputs.iter().any(|&i| i >= self.nodes.len()) {
                return Err(Error::InvalidGraph);
            }
            dependencies.push(inputs);
        }
        fn visit(
            id: usize,
            deps: &[Vec<usize>],
            marks: &mut [u8],
            order: &mut Vec<usize>,
        ) -> Result<(), Error> {
            if marks[id] == 1 {
                return Err(Error::Cycle);
            }
            if marks[id] == 2 {
                return Ok(());
            }
            marks[id] = 1;
            for &input in &deps[id] {
                visit(input, deps, marks, order)?;
            }
            marks[id] = 2;
            order.push(id);
            Ok(())
        }
        let mut order = vec![];
        let mut marks = vec![0; self.nodes.len()];
        for id in 0..self.nodes.len() {
            visit(id, &dependencies, &mut marks, &mut order)?;
        }
        let ops = order
            .into_iter()
            .map(|id| {
                (
                    id,
                    match &self.nodes[id] {
                        Node::Player { .. } => Op::Player(player_ids[id].unwrap()),
                        Node::Gain { input, gain } => Op::Gain(*input, *gain),
                        Node::Sum { inputs } => Op::Sum(inputs.clone()),
                    },
                )
            })
            .collect();
        let flat = self.flat_voices(&player_ids);
        Ok(Program {
            ops,
            flat,
            assets: player_assets,
            outputs: self.outputs.clone(),
            nodes: self.nodes.len(),
            limits,
        })
    }
    // Narrow shape only: exactly one sum of distinct gain(player) branches,
    // all nodes/players used exactly once. Everything else retains generic execution.
    fn flat_voices(&self, player_ids: &[Option<usize>]) -> Option<Vec<FlatVoice>> {
        if self.outputs.len() != 1 {
            return None;
        }
        let output = self.outputs[0].1;
        let Node::Sum { inputs } = &self.nodes[output] else {
            return None;
        };
        let mut seen = vec![false; self.nodes.len()];
        seen[output] = true;
        let mut voices = Vec::with_capacity(inputs.len());
        for &input in inputs {
            if seen[input] {
                return None;
            }
            let Node::Gain {
                input: player,
                gain,
            } = self.nodes[input]
            else {
                return None;
            };
            if seen[player] {
                return None;
            }
            let id = player_ids[player]?;
            seen[input] = true;
            seen[player] = true;
            voices.push(FlatVoice { player: id, gain });
        }
        seen.into_iter().all(|value| value).then_some(voices)
    }
}
impl Program {
    pub fn supports_flat_fused(&self) -> bool {
        self.flat.is_some()
    }
    pub fn output_names(&self) -> Vec<&str> {
        self.outputs.iter().map(|(name, _)| name.as_str()).collect()
    }
    pub fn block_frame_limit(&self) -> usize {
        self.limits.block_frames
    }
    pub fn player_count(&self) -> usize {
        self.assets.len()
    }
    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playback {
    Stopped,
    Playing,
    Ended,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub position: usize,
    pub status: Playback,
    pub loop_region: Option<(usize, usize)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub output_frame: u64,
    pub revision: u64,
    pub pending_batches: usize,
    pub players: Vec<PlayerState>,
}
/// Borrowed authoritative state; constructing this view neither allocates nor clones.
pub struct ObservationRef<'a> {
    pub output_frame: u64,
    pub revision: u64,
    pub pending_batches: usize,
    pub players: &'a [PlayerState],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Start(usize),
    Stop(usize),
    Seek(usize, usize),
    SetLoop(usize, Option<(usize, usize)>),
}
impl Command {
    fn player(self) -> usize {
        match self {
            Self::Start(p) | Self::Stop(p) | Self::Seek(p, _) | Self::SetLoop(p, _) => p,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Batch {
    pub frame: u64,
    pub revision: u64,
    pub commands: Vec<Command>,
}
pub struct RenderState {
    program: Arc<Program>,
    strategy: RenderStrategy,
    enqueue_strategy: EnqueueStrategy,
    pending_commands: usize,
    players: Vec<PlayerState>,
    scratch: Vec<f32>,
    frame: u64,
    revision: u64,
    queue: Vec<Batch>,
    cursor: usize,
}
impl RenderState {
    pub fn new(program: Arc<Program>) -> Self {
        Self::new_with_strategy(program, RenderStrategy::default())
    }
    pub fn new_with_strategy(program: Arc<Program>, strategy: RenderStrategy) -> Self {
        Self::new_with_strategies(program, strategy, EnqueueStrategy::default())
    }
    pub fn new_with_strategies(
        program: Arc<Program>,
        strategy: RenderStrategy,
        enqueue_strategy: EnqueueStrategy,
    ) -> Self {
        Self {
            strategy,
            enqueue_strategy,
            pending_commands: 0,
            players: vec![
                PlayerState {
                    position: 0,
                    status: Playback::Stopped,
                    loop_region: None
                };
                program.assets.len()
            ],
            scratch: vec![0.; program.nodes],
            frame: 0,
            revision: 0,
            queue: Vec::with_capacity(program.limits.pending_commands),
            cursor: 0,
            program,
        }
    }
    pub fn active_strategy(&self) -> RenderStrategy {
        if self.strategy == RenderStrategy::FlatFused && self.program.flat.is_some() {
            RenderStrategy::FlatFused
        } else {
            RenderStrategy::Generic
        }
    }
    pub fn observe(&self) -> Observation {
        Observation {
            output_frame: self.frame,
            revision: self.revision,
            pending_batches: self.queue.len() - self.cursor,
            players: self.players.clone(),
        }
    }
    pub fn observe_ref(&self) -> ObservationRef<'_> {
        ObservationRef {
            output_frame: self.frame,
            revision: self.revision,
            pending_batches: self.queue.len() - self.cursor,
            players: &self.players,
        }
    }
    /// Apply a borrowed batch now, before any queued commands at the next frame.
    /// The caller must bound the number of invocations per callback. No storage is retained.
    pub fn apply_immediate(&mut self, revision: u64, commands: &[Command]) -> Result<(), Error> {
        if revision != self.revision {
            return Err(Error::StaleRevision);
        }
        if commands.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if commands.len() > self.program.limits.batch_commands
            || commands.len() > self.program.limits.commands_per_frame
        {
            return Err(Error::ResourceLimit);
        }
        self.validate_commands(commands)?;
        for &command in commands {
            apply_command(&mut self.players, command);
        }
        Ok(())
    }
    fn validate_commands(&self, commands: &[Command]) -> Result<(), Error> {
        for &command in commands {
            let asset = self
                .program
                .assets
                .get(command.player())
                .ok_or(Error::UnknownPlayer)?;
            match command {
                Command::Seek(_, position) if position > asset.len() => {
                    return Err(Error::InvalidSeek)
                }
                Command::SetLoop(_, Some((start, end))) if start >= end || end > asset.len() => {
                    return Err(Error::InvalidLoop)
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn advance_revision(&mut self, new: u64) -> Result<(), Error> {
        if new <= self.revision {
            return Err(Error::StaleRevision);
        }
        self.revision = new;
        self.queue.clear();
        self.pending_commands = 0;
        self.cursor = 0;
        Ok(())
    }
    pub fn enqueue(&mut self, batch: Batch) -> Result<(), Error> {
        if batch.revision != self.revision {
            return Err(Error::StaleRevision);
        }
        if batch.frame < self.frame {
            return Err(Error::PastEvent);
        }
        if batch.commands.is_empty() {
            return Err(Error::EmptyBatch);
        }
        let limits = self.program.limits;
        let pending = &self.queue[self.cursor..];
        // Keep identical accounting on both controls; only the budget lookup differs.
        let (total, same_frame) = match self.enqueue_strategy {
            EnqueueStrategy::Scan => (
                pending.iter().map(|b| b.commands.len()).sum::<usize>(),
                pending
                    .iter()
                    .filter(|b| b.frame == batch.frame)
                    .map(|b| b.commands.len())
                    .sum::<usize>(),
            ),
            EnqueueStrategy::Counted => {
                let start = pending.partition_point(|b| b.frame < batch.frame);
                let end = pending.partition_point(|b| b.frame <= batch.frame);
                (
                    self.pending_commands,
                    pending[start..end]
                        .iter()
                        .map(|b| b.commands.len())
                        .sum::<usize>(),
                )
            }
        };
        if batch.commands.len() > limits.batch_commands
            || batch.commands.len() > limits.pending_commands.saturating_sub(total)
            || batch.commands.len() > limits.commands_per_frame.saturating_sub(same_frame)
        {
            return Err(Error::ResourceLimit);
        }
        self.validate_commands(&batch.commands)?;
        // Preparation-side reclamation only; render never drops queued command storage.
        if self.cursor > 0 {
            self.queue.drain(..self.cursor);
            self.cursor = 0;
        }
        let index = self.queue.partition_point(|b| b.frame <= batch.frame);
        self.pending_commands += batch.commands.len();
        self.queue.insert(index, batch);
        Ok(())
    }
    /// Convenience allocation for tests. Timed runs use render_into with prepared buffers.
    pub fn render(&mut self, frames: usize) -> Result<Vec<Vec<f32>>, Error> {
        if frames > self.program.limits.block_frames {
            return Err(Error::ResourceLimit);
        }
        let mut output = vec![vec![0.; frames]; self.program.outputs.len()];
        self.render_into(&mut output)?;
        Ok(output)
    }
    /// All buffer validation precedes mutation. A zero-frame buffer is an exact no-op.
    pub fn render_into(&mut self, output: &mut [Vec<f32>]) -> Result<(), Error> {
        if output.len() != self.program.outputs.len() {
            return Err(Error::InvalidBuffer);
        }
        let frames = output.first().map_or(0, Vec::len);
        if output.iter().any(|channel| channel.len() != frames) {
            return Err(Error::InvalidBuffer);
        }
        self.render_into_prefix(output, frames)
    }
    /// Render into the first `frames` of prepared planar buffers, retaining their lengths.
    pub fn render_into_prefix(
        &mut self,
        output: &mut [Vec<f32>],
        frames: usize,
    ) -> Result<(), Error> {
        if output.len() != self.program.outputs.len()
            || output.iter().any(|channel| channel.len() < frames)
        {
            return Err(Error::InvalidBuffer);
        }
        if frames > self.program.limits.block_frames {
            return Err(Error::ResourceLimit);
        }
        self.frame
            .checked_add(frames as u64)
            .ok_or(Error::FrameOverflow)?;
        if self.active_strategy() == RenderStrategy::FlatFused {
            return self.render_flat_into(output, frames);
        }
        for frame in 0..frames {
            while self.cursor < self.queue.len() && self.queue[self.cursor].frame == self.frame {
                for &command in &self.queue[self.cursor].commands {
                    let player = &mut self.players[command.player()];
                    match command {
                        Command::Start(_) => player.status = Playback::Playing,
                        Command::Stop(_) => player.status = Playback::Stopped,
                        Command::Seek(_, position) => {
                            player.position = position;
                            if player.status == Playback::Ended {
                                player.status = Playback::Stopped;
                            }
                        }
                        Command::SetLoop(_, region) => player.loop_region = region,
                    }
                }
                self.pending_commands -= self.queue[self.cursor].commands.len();
                self.cursor += 1;
            }
            for (id, op) in &self.program.ops {
                self.scratch[*id] = match op {
                    Op::Player(index) => {
                        let player = &mut self.players[*index];
                        let asset = &self.program.assets[*index];
                        let mut sample = 0.;
                        if player.status == Playback::Playing {
                            if let Some((start, end)) = player.loop_region {
                                if player.position < start || player.position >= end {
                                    player.position = start;
                                }
                            }
                            if player.position < asset.len() {
                                sample = asset[player.position];
                                player.position += 1;
                            }
                            if let Some((start, end)) = player.loop_region {
                                if player.position == end {
                                    player.position = start;
                                }
                            } else if player.position == asset.len() {
                                player.status = Playback::Ended;
                            }
                        }
                        sample
                    }
                    Op::Gain(input, gain) => self.scratch[*input] * gain,
                    Op::Sum(inputs) => {
                        let mut sum = 0.;
                        for input in inputs {
                            sum += self.scratch[*input];
                        }
                        sum
                    }
                };
            }
            for (channel, (_, node)) in output.iter_mut().zip(&self.program.outputs) {
                channel[frame] = self.scratch[*node];
            }
            self.frame += 1;
        }
        Ok(())
    }
    // One block-level dispatch; command application remains per sample, unchanged.
    #[inline(never)]
    fn render_flat_into(&mut self, output: &mut [Vec<f32>], frames: usize) -> Result<(), Error> {
        let voices = self.program.flat.as_ref().unwrap();
        for frame in 0..frames {
            while self.cursor < self.queue.len() && self.queue[self.cursor].frame == self.frame {
                for &command in &self.queue[self.cursor].commands {
                    let player = &mut self.players[command.player()];
                    match command {
                        Command::Start(_) => player.status = Playback::Playing,
                        Command::Stop(_) => player.status = Playback::Stopped,
                        Command::Seek(_, position) => {
                            player.position = position;
                            if player.status == Playback::Ended {
                                player.status = Playback::Stopped;
                            }
                        }
                        Command::SetLoop(_, region) => player.loop_region = region,
                    }
                }
                self.pending_commands -= self.queue[self.cursor].commands.len();
                self.cursor += 1;
            }
            let mut sum = 0.;
            for voice in voices {
                let player = &mut self.players[voice.player];
                let asset = &self.program.assets[voice.player];
                let mut sample = 0.;
                if player.status == Playback::Playing {
                    if let Some((start, end)) = player.loop_region {
                        if player.position < start || player.position >= end {
                            player.position = start;
                        }
                    }
                    if player.position < asset.len() {
                        sample = asset[player.position];
                        player.position += 1;
                    }
                    if let Some((start, end)) = player.loop_region {
                        if player.position == end {
                            player.position = start;
                        }
                    } else if player.position == asset.len() {
                        player.status = Playback::Ended;
                    }
                }
                // Separate multiply/add, preserving the declared sum order.
                let scaled = sample * voice.gain;
                sum += scaled;
            }
            output[0][frame] = sum;
            self.frame += 1;
        }
        Ok(())
    }
}

fn apply_command(players: &mut [PlayerState], command: Command) {
    let player = &mut players[command.player()];
    match command {
        Command::Start(_) => player.status = Playback::Playing,
        Command::Stop(_) => player.status = Playback::Stopped,
        Command::Seek(_, position) => {
            player.position = position;
            if player.status == Playback::Ended {
                player.status = Playback::Stopped;
            }
        }
        Command::SetLoop(_, region) => player.loop_region = region,
    }
}
