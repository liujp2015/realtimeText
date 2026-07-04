use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use rtrb::Producer;
use std::sync::mpsc;
use std::thread::JoinHandle;

pub struct CaptureHandle {
    pub stream: Stream,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone)]
pub struct CaptureInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

fn build_stream(mut producer: Producer<f32>) -> Result<CaptureHandle> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("no default output device"))?;

    let name = device.name().unwrap_or_else(|_| "unknown".to_string());

    let supported_configs: Vec<_> = device.supported_output_configs()?.collect();
    if supported_configs.is_empty() {
        return Err(anyhow!("no supported output config"));
    }

    // Pick an F32 config, preferring 48000 Hz. Avoids U8/S16 formats and
    // with_max_sample_rate() landing on 192kHz.
    let chosen = supported_configs
        .iter()
        .filter(|c| c.sample_format() == SampleFormat::F32)
        .min_by_key(|c| (c.min_sample_rate().0 as i64 - 48_000).abs())
        .ok_or_else(|| {
            let formats: Vec<_> = supported_configs
                .iter()
                .map(|c| format!("{:?}", c.sample_format()))
                .collect();
            anyhow!(
                "no f32 output config; device supports [{}]",
                formats.join(", ")
            )
        })?;

    let target = 48_000;
    let min_r = chosen.min_sample_rate().0;
    let max_r = chosen.max_sample_rate().0;
    let rate = if min_r <= target && target <= max_r {
        target
    } else if max_r <= target {
        max_r
    } else {
        min_r
    };
    let config = chosen.with_sample_rate(cpal::SampleRate(rate));
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    let stream_config: StreamConfig = config.into();

    let stream = device.build_input_stream(
        &stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            for &s in data.iter() {
                let _ = producer.push(s);
            }
        },
        |err| log::error!("audio stream error: {:?}", err),
        None,
    )?;

    stream.play()?;

    log::info!(
        "loopback capture started: device={}, rate={}, ch={}",
        name,
        sample_rate,
        channels
    );

    Ok(CaptureHandle {
        stream,
        device_name: name,
        sample_rate,
        channels,
    })
}

/// Spawns a dedicated OS thread that creates and holds the cpal stream.
/// Returns (join_handle, stop_sender, info_receiver).
/// The stream is created and dropped on the same thread (cpal::Stream is !Send).
pub fn start_loopback_thread(
    producer: Producer<f32>,
) -> (JoinHandle<()>, mpsc::Sender<()>, mpsc::Receiver<Result<CaptureInfo, String>>) {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (info_tx, info_rx) = mpsc::channel::<Result<CaptureInfo, String>>();

    let handle = std::thread::Builder::new()
        .name("audio-capture".into())
        .spawn(move || {
            match build_stream(producer) {
                Ok(capture) => {
                    let info = CaptureInfo {
                        device_name: capture.device_name.clone(),
                        sample_rate: capture.sample_rate,
                        channels: capture.channels,
                    };
                    let _ = info_tx.send(Ok(info));
                    // Block until stop signal; stream stays alive
                    let _ = stop_rx.recv();
                    // capture dropped here, on the same thread
                    log::info!("audio capture thread exiting, stream dropped");
                }
                Err(e) => {
                    let _ = info_tx.send(Err(e.to_string()));
                }
            }
        })
        .expect("spawn audio thread");

    (handle, stop_tx, info_rx)
}

pub fn current_default_output_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.name().ok())
}
