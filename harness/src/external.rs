//! First-copy adapter. Scheduling and observable position are adapter bookkeeping.
use crate::model::*;
use fundsp::{
    audionode::AudioNode,
    wave::{Wave, WavePlayer},
};
use std::sync::Arc;
#[derive(Default)]
pub struct FundspCopy {
    state: State,
    wave: Option<Arc<Wave>>,
    player: Option<WavePlayer>,
    len: usize,
}
impl Adapter for FundspCopy {
    fn load(&mut self, asset: &[f32], rate: u32, channels: u16) -> Result<(), Failure> {
        let invalid = if asset.is_empty() || asset.iter().any(|x| !x.is_finite()) {
            Some(RequestError::InvalidAsset)
        } else if rate != 48000 || channels != 1 {
            Some(RequestError::UnsupportedFormat)
        } else if self.state.loaded {
            Some(RequestError::AlreadyPrepared)
        } else {
            None
        };
        if let Some(error) = invalid {
            return Err(Failure::Rejected(error));
        }
        self.wave = Some(Arc::new(Wave::from_samples(rate as f64, asset)));
        self.len = asset.len();
        self.state.loaded = true;
        Ok(())
    }
    fn compile(&mut self, asset: u32) -> Result<(), Failure> {
        if asset != 0 {
            return Err(Failure::Rejected(RequestError::UnknownAsset));
        }
        if !self.state.loaded {
            return Err(Failure::Rejected(RequestError::Unprepared));
        }
        if self.state.compiled {
            return Err(Failure::Rejected(RequestError::AlreadyPrepared));
        }
        self.state.compiled = true;
        Ok(())
    }
    fn initialize(&mut self) -> Result<(), Failure> {
        if !self.state.compiled {
            return Err(Failure::Rejected(RequestError::Unprepared));
        }
        if self.state.initialized {
            return Err(Failure::Rejected(RequestError::AlreadyPrepared));
        }
        self.player = Some(WavePlayer::new(
            self.wave.as_ref().unwrap(),
            0,
            0,
            self.len,
            None,
        ));
        self.state.initialized = true;
        Ok(())
    }
    fn start(&mut self, player: u32, frame: i64) -> Result<(), Failure> {
        let error = if player != 0 {
            Some(RequestError::UnknownPlayer)
        } else if frame < 0 {
            Some(RequestError::InvalidFrame)
        } else if !self.state.initialized {
            Some(RequestError::Unprepared)
        } else if self.state.pending_start.is_some() {
            Some(RequestError::PendingStart)
        } else if self.state.status != Status::Stopped {
            Some(RequestError::NotStopped)
        } else if (frame as u64) < self.state.output_frame {
            Some(RequestError::PastEvent)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(Failure::Rejected(error));
        }
        self.state.pending_start = Some(frame as u64);
        Ok(())
    }
    fn render(&mut self, frames: usize) -> Result<Vec<f32>, Failure> {
        if frames == 0 {
            return Ok(vec![]);
        }
        if !self.state.initialized {
            return Err(Failure::Rejected(RequestError::Unprepared));
        }
        if frames > 1_048_576 || self.state.output_frame.checked_add(frames as u64).is_none() {
            return Err(Failure::Rejected(RequestError::InvalidBlock));
        }
        let mut pcm = vec![0.; frames];
        for sample in &mut pcm {
            if self.state.pending_start == Some(self.state.output_frame) {
                self.state.pending_start = None;
                self.state.status = Status::Playing;
            }
            if self.state.status == Status::Playing {
                *sample = self.player.as_mut().unwrap().get_mono();
                self.state.source_frame += 1;
                if self.state.source_frame == self.len as u64 {
                    self.state.status = Status::Ended;
                }
            }
            self.state.output_frame += 1;
        }
        Ok(pcm)
    }
    fn observe(&self) -> State {
        self.state.clone()
    }
}
