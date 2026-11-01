//! Opt-in Linux/ALSA RAM fixture. No device access occurs at import or in tests.
use crate::{
    failure,
    output::{Callback, ControlBatch, Controller},
};
use audio_flow_harness::offline::{Assets, Command, Graph, Limits, Node, Playback, Program};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use napi::Result;
use napi_derive::napi;
use rtrb::{Consumer, Producer, RingBuffer};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const PUBLICATIONS: usize = 32;
const FRAMES: usize = 12000;

pub(crate) fn program() -> Result<Arc<Program>> {
    let limits = Limits::default();
    let mut assets = Assets::new(limits).map_err(failure)?;
    for hz in [440., 660.] {
        let samples: Vec<f32> = (0..FRAMES)
            .map(|i| 0.02 * (std::f32::consts::TAU * hz * i as f32 / 48000.).sin())
            .collect();
        assets.load(&samples, 48000).map_err(failure)?;
    }
    Ok(Arc::new(
        Graph {
            nodes: vec![Node::Player { asset: 0 }, Node::Player { asset: 1 }],
            outputs: vec![("fixture.left".into(), 0), ("fixture.right".into(), 1)],
        }
        .compile(&assets, limits)
        .map_err(failure)?,
    ))
}

/// Pure validation shared by real open and fake capability tests. No inference.
pub(crate) fn validate(
    selected: &str,
    expected: &str,
    id: &str,
    name: Option<&str>,
    channels: usize,
    supported: bool,
) -> Result<()> {
    if selected != id || name != Some(expected) || channels == 0 || channels > 128 || !supported {
        return Err(failure(
            "stale descriptor or unsupported 48kHz f32 channels",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Player {
    position: usize,
    status: u8,
    region: Option<(usize, usize)>,
}
#[derive(Clone, Copy, Default)]
struct Publication {
    frame: u64,
    players: [Player; 2],
    callbacks: u64,
    applied: u64,
    rejected: u64,
    misses: u64,
    max_nanos: u64,
    dropped: u64,
    process_errors: u64,
    last_error: u8,
}
struct Publisher {
    queue: Producer<Publication>,
    dropped: u64,
    errors: u64,
    last_error: u8,
}
impl Publisher {
    fn publish(&mut self, callback: &Callback, failed: bool) {
        if failed {
            self.errors = self.errors.saturating_add(1);
            self.last_error = 1;
        }
        let observation = callback.snapshot();
        let mut p = Publication {
            frame: observation.output_frame,
            callbacks: callback.diagnostics.callbacks,
            applied: callback.diagnostics.applied_batches,
            rejected: callback.diagnostics.rejected_batches,
            misses: callback.diagnostics.deadline_misses,
            max_nanos: callback.diagnostics.max_callback_nanos,
            dropped: self.dropped,
            process_errors: self.errors,
            last_error: self.last_error,
            ..Publication::default()
        };
        if p.rejected > 0 && p.last_error == 0 {
            p.last_error = 2;
        }
        for (destination, source) in p.players.iter_mut().zip(observation.players) {
            *destination = Player {
                position: source.position,
                status: match source.status {
                    Playback::Stopped => 0,
                    Playback::Playing => 1,
                    Playback::Ended => 2,
                },
                region: source.loop_region,
            };
        }
        if self.queue.push(p).is_err() {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
}

/// Explicitly opened stream; pause controls players, NOT the device clock.
#[napi]
pub struct ExperimentalFixtureHost {
    // Dropped before the controller/publication consumer: ALSA Drop wakes + joins worker.
    stream: Option<cpal::Stream>,
    controller: Controller,
    observations: Consumer<Publication>,
    latest: Publication,
    observed: bool,
    stream_errors: Arc<AtomicUsize>,
    queued: u64,
}
#[napi]
impl ExperimentalFixtureHost {
    fn submit(&mut self, commands: &[Command]) -> Result<Value> {
        if self.stream.is_none() {
            return Err(failure("host closed"));
        }
        self.controller
            .submit(ControlBatch::new(0, commands).map_err(failure)?)
            .map_err(|_| failure("command queue full"))?;
        self.queued += 1;
        Ok(json!({"queuedBatch": self.queued, "applied": false}))
    }
    #[napi]
    pub fn play(&mut self) -> Result<Value> {
        self.submit(&[Command::Start(0), Command::Start(1)])
    }
    #[napi]
    pub fn pause(&mut self) -> Result<Value> {
        self.submit(&[Command::Stop(0), Command::Stop(1)])
    }
    #[napi]
    pub fn seek(&mut self, source_frame: u32) -> Result<Value> {
        self.submit(&[
            Command::Seek(0, source_frame as usize),
            Command::Seek(1, source_frame as usize),
        ])
    }
    #[napi]
    pub fn set_loop(&mut self, region: Option<Vec<u32>>) -> Result<Value> {
        let region = match region {
            None => None,
            Some(v) if v.len() == 2 => Some((v[0] as usize, v[1] as usize)),
            _ => return Err(failure("expected null or [start,end]")),
        };
        self.submit(&[Command::SetLoop(0, region), Command::SetLoop(1, region)])
    }
    #[napi]
    pub fn snapshot(&mut self) -> Value {
        // Fixed work even if the callback keeps publishing. Never waits for freshness.
        for _ in 0..PUBLICATIONS {
            match self.observations.pop() {
                Ok(p) => {
                    self.latest = p;
                    self.observed = true;
                }
                Err(_) => break,
            }
        }
        let p = self.latest;
        json!({"closed": self.stream.is_none(), "queuedBatches": self.queued, "queueFull": self.controller.queue_full,
            "streamErrors": self.stream_errors.load(Ordering::Relaxed),
            "observation": if self.observed { Some(json!({"outputFrame": p.frame,
                "players": p.players.map(|v| json!({"sourceFrame": v.position, "status": v.status, "loop": v.region})),
                "callbacks": p.callbacks, "appliedBatches": p.applied, "rejectedBatches": p.rejected,
                "processedBatches": p.applied + p.rejected, "deadlineMisses": p.misses, "maxCallbackNanos": p.max_nanos,
                "publicationDrops": p.dropped, "processErrors": p.process_errors, "lastErrorCode": p.last_error})) } else { None }})
    }
    #[napi]
    pub fn close(&mut self) {
        drop(self.stream.take());
    }
}

/// This function alone opens/starts hardware. Linux is gated by audited CPAL ALSA join.
#[napi]
pub fn open_experimental_fixture_host(options: Value) -> Result<ExperimentalFixtureHost> {
    if !cfg!(target_os = "linux") {
        return Err(failure("fixture host shutdown audited only on Linux/ALSA"));
    }
    let o = options
        .as_object()
        .ok_or_else(|| failure("expected explicit selection object"))?;
    if o.keys()
        .any(|k| !["deviceId", "expectedName", "channels", "mappings"].contains(&k.as_str()))
    {
        return Err(failure("unknown option"));
    }
    let id = o
        .get("deviceId")
        .and_then(Value::as_str)
        .ok_or_else(|| failure("deviceId required"))?;
    let expected = o
        .get("expectedName")
        .and_then(Value::as_str)
        .ok_or_else(|| failure("expectedName required"))?;
    let channels = o
        .get("channels")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0 && *n <= 128)
        .ok_or_else(|| failure("channels required in 1..128"))? as usize;
    let routes = o
        .get("mappings")
        .and_then(Value::as_array)
        .ok_or_else(|| failure("mappings required"))?;
    let mut mappings = Vec::new();
    for route in routes {
        let name = route
            .get("output")
            .and_then(Value::as_str)
            .ok_or_else(|| failure("mapping.output required"))?;
        let physical = route
            .get("physical")
            .and_then(Value::as_u64)
            .filter(|n| *n < channels as u64)
            .ok_or_else(|| failure("invalid physical index"))?;
        mappings.push((name.to_owned(), physical as usize));
    }
    let (controller, mut callback) =
        Callback::prepare(program()?, 48000, "f32", channels, 256, &mappings).map_err(failure)?;
    let host = cpal::default_host();
    let host_name = format!("{:?}", host.id());
    let mut selected = None;
    for (index, device) in host.output_devices().map_err(failure)?.enumerate() {
        let current = format!("{host_name}:{index}");
        if current != id {
            continue;
        }
        let name = device.name().map_err(failure)?;
        let supported = device
            .supported_output_configs()
            .map_err(failure)?
            .any(|c| {
                c.channels() as usize == channels
                    && c.sample_format() == cpal::SampleFormat::F32
                    && c.min_sample_rate().0 <= 48000
                    && c.max_sample_rate().0 >= 48000
            });
        validate(id, expected, &current, Some(&name), channels, supported)?;
        selected = Some(device);
        break;
    }
    let device = selected.ok_or_else(|| failure("stale device ID"))?;
    let (queue, observations) = RingBuffer::new(PUBLICATIONS);
    let mut publisher = Publisher {
        queue,
        dropped: 0,
        errors: 0,
        last_error: 0,
    };
    let stream_errors = Arc::new(AtomicUsize::new(0));
    let errors = stream_errors.clone();
    let config = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: cpal::SampleRate(48000),
        buffer_size: cpal::BufferSize::Default,
    };
    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                let failed = callback.process(data).is_err();
                if failed {
                    data.fill(0.);
                }
                publisher.publish(&callback, failed);
            },
            move |_| {
                errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(failure)?;
    stream.play().map_err(failure)?;
    Ok(ExperimentalFixtureHost {
        stream: Some(stream),
        controller,
        observations,
        latest: Publication::default(),
        observed: false,
        stream_errors,
        queued: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fake_selection_and_signal_publication() {
        assert!(validate("Alsa:1", "x", "Alsa:1", Some("x"), 4, true).is_ok());
        for (id, name, channels, supported) in [
            ("Alsa:2", Some("x"), 4, true),
            ("Alsa:1", Some("y"), 4, true),
            ("Alsa:1", None, 4, true),
            ("Alsa:1", Some("x"), 0, true),
            ("Alsa:1", Some("x"), 4, false),
        ] {
            assert!(validate("Alsa:1", "x", id, name, channels, supported).is_err());
        }
        let (mut control, mut callback) = Callback::prepare(
            program().unwrap(),
            48000,
            "f32",
            2,
            256,
            &[("fixture.left".into(), 0), ("fixture.right".into(), 1)],
        )
        .unwrap();
        let (queue, mut read) = RingBuffer::new(PUBLICATIONS);
        let mut publish = Publisher {
            queue,
            dropped: 0,
            errors: 0,
            last_error: 0,
        };
        control
            .submit(ControlBatch::new(0, &[Command::Start(0), Command::Start(1)]).unwrap())
            .unwrap();
        let mut data = [0.; 128];
        callback.process(&mut data).unwrap();
        assert!(data.iter().all(|v| v.abs() <= 0.02));
        assert!(data.iter().any(|v| *v != 0.));
        publish.publish(&callback, false);
        let p = read.pop().unwrap();
        assert_eq!(
            (
                p.frame,
                p.players[0].position,
                p.players[1].position,
                p.applied
            ),
            (64, 64, 64, 1)
        );
        control
            .submit(
                ControlBatch::new(0, &[Command::Seek(0, FRAMES + 1), Command::Stop(1)]).unwrap(),
            )
            .unwrap();
        callback.process(&mut data).unwrap();
        publish.publish(&callback, false);
        let p = read.pop().unwrap();
        assert_eq!((p.rejected, p.players[1].status, p.frame), (1, 1, 128));
        for _ in 0..PUBLICATIONS + 2 {
            publish.publish(&callback, false);
        }
        assert_eq!(publish.dropped, 2);
    }
}
