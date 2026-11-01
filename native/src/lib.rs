//! Experimental binding. Streams open only through explicit fixture-host opt-in.
mod fixture_host;
pub mod output;
#[cfg(test)]
mod output_tests;
use audio_flow_harness::offline::{Assets, Batch, Command, Graph, Limits, Node, RenderState};
use cpal::traits::{DeviceTrait, HostTrait};
use napi::{Error, Result};
use napi_derive::napi;
use serde_json::{json, Value};
use std::sync::Arc;

fn failure(error: impl std::fmt::Debug) -> Error {
    Error::from_reason(format!("audio-flow experimental: {error:?}"))
}

#[napi]
pub fn engine_info() -> Value {
    json!({
        "experimental": true, "nativeOutputAvailable": false,
        "experimentalFixtureHostAvailable": cfg!(target_os = "linux"),
        "status": "recording output unavailable; opt-in RAM fixture only; no realtime proof",
        "sampleRate": 48000, "assetFormat": "mono f32 RAM, finite nonempty samples",
        "limits": Limits::default(), "deviceIds": "ephemeral host/name/index; not persistent"
    })
}

#[napi]
pub fn list_output_devices() -> Result<Value> {
    let host = cpal::default_host();
    let host_name = format!("{:?}", host.id());
    let devices = host.output_devices().map_err(failure)?;
    let result: Vec<Value> = devices
        .enumerate()
        .map(|(index, device)| {
            let (name, name_error) = match device.name() {
                Ok(name) => (Some(name), None),
                Err(error) => (None, Some(error.to_string())),
            };
            let (configs, config_error) = match device.supported_output_configs() {
                Ok(configs) => (
                    configs
                        .map(|config| {
                            json!({
                                "channels": config.channels(),
                                "minSampleRate": config.min_sample_rate().0,
                                "maxSampleRate": config.max_sample_rate().0,
                                "sampleFormat": config.sample_format().to_string()
                            })
                        })
                        .collect::<Vec<_>>(),
                    None,
                ),
                Err(error) => (vec![], Some(error.to_string())),
            };
            json!({"id": format!("{host_name}:{index}"), "index": index,
            "name": name, "nameError": name_error, "configurations": configs,
            "configurationError": config_error})
        })
        .collect();
    Ok(json!({"host": host_name, "idsEphemeral": true, "devices": result}))
}

fn fixture(frames: usize) -> Result<Value> {
    let limits = Limits::default();
    if frames > limits.block_frames {
        return Err(failure("frames must be an integer in 0..=4096"));
    }
    let mut assets = Assets::new(limits).map_err(failure)?;
    let left = assets
        .load(&[0.25, -0.5, 0.75, -1.0], 48000)
        .map_err(failure)?;
    let right = assets
        .load(&[0.125, 0.25, 0.5, 0.75], 48000)
        .map_err(failure)?;
    let graph = Graph {
        nodes: vec![
            Node::Player { asset: left },
            Node::Player { asset: right },
            Node::Gain {
                input: 1,
                gain: 0.5,
            },
            Node::Sum { inputs: vec![0, 2] },
        ],
        outputs: vec![
            ("fixture.left".into(), 0),
            ("fixture.right".into(), 2),
            ("fixture.sum".into(), 3),
        ],
    };
    let program = Arc::new(graph.compile(&assets, limits).map_err(failure)?);
    let names: Vec<String> = program
        .output_names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut state = RenderState::new(program);
    for (frame, player) in [(1, 0), (2, 1)] {
        state
            .enqueue(Batch {
                frame,
                revision: 0,
                commands: vec![Command::Start(player)],
            })
            .map_err(failure)?;
    }
    let samples = state.render(frames).map_err(failure)?;
    Ok(
        json!({"sampleRate": 48000, "frames": frames, "channels": names.len(),
        "layout": "planar; samples[channel][frame]; graph outputs, not hardware routes",
        "outputNames": names, "samples": samples, "observation": state.observe()}),
    )
}

#[napi]
pub fn render_fixture(options: Option<Value>) -> Result<Value> {
    let frames = match options {
        None => 10,
        Some(Value::Object(options)) if options.keys().all(|key| key == "frames") => {
            match options.get("frames") {
                None => 10,
                Some(value) => value
                    .as_u64()
                    .filter(|n| *n <= 4096)
                    .ok_or_else(|| failure("frames must be an integer in 0..=4096"))?
                    as usize,
            }
        }
        _ => return Err(failure("expected optional { frames: integer }")),
    };
    fixture(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_named_channels_and_boundaries() {
        let rendered = fixture(10).unwrap();
        assert_eq!(
            rendered["outputNames"],
            json!(["fixture.left", "fixture.right", "fixture.sum"])
        );
        assert_eq!(
            rendered["samples"],
            json!([
                [0., 0.25, -0.5, 0.75, -1., 0., 0., 0., 0., 0.],
                [0., 0., 0.0625, 0.125, 0.25, 0.375, 0., 0., 0., 0.],
                [0., 0.25, -0.4375, 0.875, -0.75, 0.375, 0., 0., 0., 0.]
            ])
        );
        assert_eq!(rendered["observation"]["output_frame"], 10);
        assert_eq!(rendered["observation"]["pending_batches"], 0);
        assert_eq!(fixture(0).unwrap()["samples"], json!([[], [], []]));
        assert!(fixture(4097).is_err());
        assert!(render_fixture(Some(json!({"frames": -1}))).is_err());
        assert!(render_fixture(Some(json!({"frames": 1.5}))).is_err());
        assert!(render_fixture(Some(json!({"unknown": true}))).is_err());
    }
}
