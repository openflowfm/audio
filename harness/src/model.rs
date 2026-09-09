use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub schema_version: u32,
    pub id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub asset: Vec<f32>,
    pub start_frame: i64,
    pub output_frames: usize,
    pub partitions: Vec<Vec<usize>>,
    pub expected: Vec<f32>,
}
impl Fixture {
    pub fn first() -> Result<Self, String> {
        let fixture: Self = serde_json::from_str(include_str!("../fixtures/first-copy.json"))
            .map_err(|e| e.to_string())?;
        fixture.validate()?;
        Ok(fixture)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION || self.id != "first-copy-v1" {
            return Err("only first-copy-v1 schema 1 is supported".into());
        }
        if self.sample_rate != 48_000
            || self.channels != 1
            || self.start_frame != 3
            || self.output_frames != 10
            || self.asset.len() != 4
            || self.expected.len() != 10
            || self
                .asset
                .iter()
                .chain(&self.expected)
                .any(|x| !x.is_finite())
        {
            return Err("invalid first-slice fixture format or counts".into());
        }
        if self.partitions.is_empty()
            || self.partitions.iter().any(|p| {
                p.is_empty()
                    || p.contains(&0)
                    || p.iter().try_fold(0usize, |sum, n| sum.checked_add(*n))
                        != Some(self.output_frames)
            })
        {
            return Err("partitions must contain positive blocks totaling ten frames".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Stopped,
    Playing,
    Ended,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub output_frame: u64,
    pub source_frame: u64,
    pub status: Status,
    pub pending_start: Option<u64>,
    pub loaded: bool,
    pub compiled: bool,
    pub initialized: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            output_frame: 0,
            source_frame: 0,
            status: Status::Stopped,
            pending_start: None,
            loaded: false,
            compiled: false,
            initialized: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestError {
    InvalidAsset,
    UnsupportedFormat,
    UnknownAsset,
    UnknownPlayer,
    InvalidFrame,
    Unprepared,
    PendingStart,
    PastEvent,
    NotStopped,
    AlreadyPrepared,
    InvalidBlock,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum Failure {
    Rejected(RequestError),
    AdapterError(String),
    CapabilityGap(String),
}

/// Experimental single-asset/player harness boundary. Handles are both zero.
/// A render error returns no PCM; successful blocks must contain exactly `frames` samples.
pub trait Adapter {
    fn load(&mut self, asset: &[f32], rate: u32, channels: u16) -> Result<(), Failure>;
    fn compile(&mut self, asset: u32) -> Result<(), Failure>;
    fn initialize(&mut self) -> Result<(), Failure>;
    fn start(&mut self, player: u32, frame: i64) -> Result<(), Failure>;
    fn render(&mut self, frames: usize) -> Result<Vec<f32>, Failure>;
    fn observe(&self) -> State;
}
