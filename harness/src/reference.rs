use crate::model::*;

/// Deliberately limited copying reference, not an audio graph engine.
#[derive(Default)]
pub struct DirectLoop {
    asset: Vec<f32>,
    state: State,
}
fn reject(error: RequestError) -> Failure {
    Failure::Rejected(error)
}
impl Adapter for DirectLoop {
    fn load(&mut self, asset: &[f32], rate: u32, channels: u16) -> Result<(), Failure> {
        if asset.is_empty() || asset.iter().any(|s| !s.is_finite()) {
            return Err(reject(RequestError::InvalidAsset));
        }
        if rate != 48_000 || channels != 1 {
            return Err(reject(RequestError::UnsupportedFormat));
        }
        if self.state.loaded {
            return Err(reject(RequestError::AlreadyPrepared));
        }
        self.asset = asset.to_vec();
        self.state.loaded = true;
        Ok(())
    }
    fn compile(&mut self, asset: u32) -> Result<(), Failure> {
        if asset != 0 {
            return Err(reject(RequestError::UnknownAsset));
        }
        if !self.state.loaded {
            return Err(reject(RequestError::Unprepared));
        }
        if self.state.compiled {
            return Err(reject(RequestError::AlreadyPrepared));
        }
        self.state.compiled = true;
        Ok(())
    }
    fn initialize(&mut self) -> Result<(), Failure> {
        if !self.state.compiled {
            return Err(reject(RequestError::Unprepared));
        }
        if self.state.initialized {
            return Err(reject(RequestError::AlreadyPrepared));
        }
        self.state.initialized = true;
        Ok(())
    }
    fn start(&mut self, player: u32, frame: i64) -> Result<(), Failure> {
        if player != 0 {
            return Err(reject(RequestError::UnknownPlayer));
        }
        let frame = u64::try_from(frame).map_err(|_| reject(RequestError::InvalidFrame))?;
        if !self.state.initialized {
            return Err(reject(RequestError::Unprepared));
        }
        if self.state.pending_start.is_some() {
            return Err(reject(RequestError::PendingStart));
        }
        if self.state.status != Status::Stopped {
            return Err(reject(RequestError::NotStopped));
        }
        if frame < self.state.output_frame {
            return Err(reject(RequestError::PastEvent));
        }
        self.state.pending_start = Some(frame);
        Ok(())
    }
    fn render(&mut self, frames: usize) -> Result<Vec<f32>, Failure> {
        if frames == 0 {
            return Ok(Vec::new());
        }
        if !self.state.initialized {
            return Err(reject(RequestError::Unprepared));
        }
        // Bound this tiny fixture adapter, including caller-controlled allocation.
        if frames > 1_048_576 || self.state.output_frame.checked_add(frames as u64).is_none() {
            return Err(reject(RequestError::InvalidBlock));
        }
        let mut output = vec![0.0; frames];
        for sample in &mut output {
            if self.state.pending_start == Some(self.state.output_frame) {
                self.state.pending_start = None;
                self.state.status = Status::Playing;
            }
            if self.state.status == Status::Playing {
                *sample = self.asset[self.state.source_frame as usize];
                self.state.source_frame += 1;
                if self.state.source_frame == self.asset.len() as u64 {
                    self.state.status = Status::Ended;
                }
            }
            self.state.output_frame += 1;
        }
        Ok(output)
    }
    fn observe(&self) -> State {
        self.state.clone()
    }
}
