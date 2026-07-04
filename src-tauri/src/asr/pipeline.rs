use anyhow::Result;
use rtrb::Consumer;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::asr::client::{submit_utterance, AsrEvent};
use crate::audio::dsp::DspPipe;
use crate::db::repository::insert_transcription;
use crate::events::{emit_asr_status, emit_subtitle_update, SubtitleUpdate};
use crate::state::AppState;
use crate::vad::{SileroVad, VadState};

const VAD_FRAME_SAMPLES: usize = 512; // 32ms @ 16kHz
const UTTERANCE_CHANNEL_CAP: usize = 8;

pub struct PipelineHandle {
    pub stop_tx: mpsc::Sender<()>,
    pub tasks: Vec<JoinHandle<()>>,
}

pub fn spawn(
    app: AppHandle,
    consumer: Consumer<f32>,
    source_rate: usize,
    api_key: String,
    session_guid: String,
) -> Result<PipelineHandle> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
    let (utt_tx, utt_rx) = mpsc::channel::<UtteranceTask>(UTTERANCE_CHANNEL_CAP);

    let vad = SileroVad::new()?;
    let main_handle = tokio::spawn(run_main(
        app.clone(),
        consumer,
        source_rate,
        utt_tx,
        stop_rx,
        vad,
    ));
    let drainer_handle = tokio::spawn(run_drainer(
        app.clone(),
        utt_rx,
        api_key,
        session_guid,
    ));

    Ok(PipelineHandle {
        stop_tx,
        tasks: vec![main_handle, drainer_handle],
    })
}

struct UtteranceTask {
    samples: Vec<f32>,
    start_ts: i64,
    end_ts: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

async fn run_main(
    app: AppHandle,
    mut consumer: Consumer<f32>,
    source_rate: usize,
    utt_tx: mpsc::Sender<UtteranceTask>,
    mut stop_rx: mpsc::Receiver<()>,
    vad_model: SileroVad,
) {
    emit_asr_status(&app, true, 0, None);

    let mut dsp = match DspPipe::new(source_rate) {
        Ok(d) => d,
        Err(e) => {
            log::error!("dsp init: {e}");
            emit_asr_status(&app, false, 0, Some(&format!("dsp init: {e}")));
            return;
        }
    };

    let mut vad_state = VadState::new();
    let mut sample_acc: Vec<f32> = Vec::with_capacity(VAD_FRAME_SAMPLES * 2);
    let mut last_prob_log = std::time::Instant::now();
    let mut frame_count: u64 = 0;

    loop {
        // Pull a batch of source samples from the ring buffer.
        let mut batch = Vec::with_capacity(4096);
        while let Ok(s) = consumer.pop() {
            batch.push(s);
            if batch.len() >= 4096 {
                break;
            }
        }

        if !batch.is_empty() {
            if let Err(e) = dsp.push_interleaved(&batch) {
                log::warn!("dsp push: {e}");
            }
        }

        // Drain resampled 16k mono f32 frames and feed VAD in 512-sample windows.
        while let Some(frame) = dsp.next_f32_frame() {
            sample_acc.extend_from_slice(&frame);
            while sample_acc.len() >= VAD_FRAME_SAMPLES {
                let window: Vec<f32> = sample_acc.drain(..VAD_FRAME_SAMPLES).collect();
                let now = now_ms();
                let prob = match vad_model.predict(&window) {
                    Ok(p) => p,
                    Err(e) => {
                        log::warn!("vad predict: {e}");
                        continue;
                    }
                };
                frame_count += 1;
                // Sample prob every ~1s for diagnostics.
                if last_prob_log.elapsed() >= std::time::Duration::from_secs(1) {
                    let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
                    log::info!("vad frame={} prob={:.3} rms={:.4}", frame_count, prob, rms);
                    last_prob_log = std::time::Instant::now();
                }
                if let Some(utt) = vad_state.push_frame(&window, prob, now) {
                    log::info!(
                        "vad committed utterance: samples={} start={} end={} dur_ms={}",
                        utt.samples.len(),
                        utt.start_ts,
                        utt.end_ts,
                        utt.end_ts - utt.start_ts
                    );
                    let task = UtteranceTask {
                        samples: utt.samples,
                        start_ts: utt.start_ts,
                        end_ts: utt.end_ts,
                    };
                    // Backpressure: if channel full, wait. Check stop signal too.
                    tokio::select! {
                        ret = utt_tx.send(task) => {
                            if ret.is_err() {
                                log::warn!("utterance channel closed, stopping main loop");
                                return;
                            }
                        }
                        _ = stop_rx.recv() => {
                            vad_model.reset();
                            return;
                        }
                    }
                }
            }
        }

        if batch.is_empty() {
            // No audio available; brief yield to avoid busy-loop.
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {}
                _ = stop_rx.recv() => {
                    vad_model.reset();
                    return;
                }
            }
        } else if stop_rx.try_recv().is_ok() {
            vad_model.reset();
            return;
        }
    }
}

async fn run_drainer(
    app: AppHandle,
    mut utt_rx: mpsc::Receiver<UtteranceTask>,
    api_key: String,
    session_guid: String,
) {
    while let Some(utt) = utt_rx.recv().await {
        let pcm_s16le = crate::audio::dsp::quantize_to_pcm_s16le(&utt.samples);
        log::info!(
            "sse submitting utterance: pcm_bytes={} start={} end={}",
            pcm_s16le.len(),
            utt.start_ts,
            utt.end_ts
        );
        let mut event_rx = match submit_utterance(&api_key, pcm_s16le, utt.start_ts, utt.end_ts).await {
            Ok(rx) => {
                log::info!("sse connection established, streaming events");
                rx
            }
            Err(e) => {
                log::warn!("sse submit failed: {e}");
                emit_asr_status(&app, true, 0, Some(&format!("submit: {e}")));
                continue;
            }
        };

        let mut last_text = String::new();
        while let Some(evt) = event_rx.recv().await {
            match evt {
                AsrEvent::Partial { text, start_ts, end_ts: _ } => {
                    log::info!("sse delta partial: {:?}", text);
                    last_text = text.clone();
                    emit_subtitle_update(&app, SubtitleUpdate {
                        state: "partial".into(),
                        text,
                        start_ts,
                        end_ts: None,
                        paralinguistic: None,
                    });
                }
                AsrEvent::Final { text, start_ts, end_ts } => {
                    let final_text = if text.is_empty() { last_text.clone() } else { text };
                    log::info!("sse done final: {:?}", final_text);
                    if let Some(state) = app.try_state::<AppState>() {
                        let pool = state.pool.clone();
                        let _ = insert_transcription(
                            &pool,
                            &session_guid,
                            &final_text,
                            start_ts,
                            end_ts,
                            None,
                        )
                        .await;
                    }
                    emit_subtitle_update(&app, SubtitleUpdate {
                        state: "final".into(),
                        text: final_text,
                        start_ts,
                        end_ts: Some(end_ts),
                        paralinguistic: None,
                    });
                    emit_asr_status(&app, true, 0, None);
                }
                AsrEvent::Error { message } => {
                    log::warn!("asr error: {message}");
                    emit_asr_status(&app, true, 0, Some(&message));
                }
            }
        }
    }
}
