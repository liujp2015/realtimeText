use anyhow::Result;
use rtrb::Consumer;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::asr::client::{submit_utterance as submit_stepfun, AsrEvent};
use crate::asr::provider::AsrConfig;
use crate::asr::volc::VolcStream;
use crate::audio::dsp::DspPipe;
use crate::db::repository::insert_transcription;
use crate::events::{emit_asr_status, emit_subtitle_update, SubtitleUpdate};
use crate::state::AppState;
use crate::vad::{SileroVad, VadEvent, VadState};

const VAD_FRAME_SAMPLES: usize = 512; // 32ms @ 16kHz
const VAD_CHANNEL_CAP: usize = 8;
/// volc 流式：攒到 100ms 音频发一个 WS 包（平衡延迟与包率）。
const STREAM_CHUNK_SAMPLES: usize = 16_000 / 10; // 1600 samples = 100ms
/// 等待 volc final 的超时（防止服务端不发 is_last 导致 drainer 卡死）。
const VOLC_FINAL_TIMEOUT: Duration = Duration::from_secs(20);

pub struct PipelineHandle {
    pub stop_tx: mpsc::Sender<()>,
    pub tasks: Vec<JoinHandle<()>>,
}

pub fn spawn(
    app: AppHandle,
    consumer: Consumer<f32>,
    source_rate: usize,
    asr: AsrConfig,
    session_guid: String,
) -> Result<PipelineHandle> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
    let (vad_tx, vad_rx) = mpsc::channel::<VadEvent>(VAD_CHANNEL_CAP);

    let vad = SileroVad::new()?;
    let main_handle = tokio::spawn(run_main(
        app.clone(),
        consumer,
        source_rate,
        vad_tx,
        stop_rx,
        vad,
    ));
    let drainer_handle = tokio::spawn(run_drainer(app.clone(), vad_rx, asr, session_guid));

    Ok(PipelineHandle {
        stop_tx,
        tasks: vec![main_handle, drainer_handle],
    })
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
    vad_tx: mpsc::Sender<VadEvent>,
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
                let events = vad_state.push_frame(&window, prob, now);
                for evt in events {
                    if let VadEvent::SpeechEnd { start_ts, end_ts } = &evt {
                        log::info!(
                            "vad committed utterance: start={} end={} dur_ms={}",
                            start_ts,
                            end_ts,
                            end_ts - start_ts
                        );
                    }
                    // Backpressure: if channel full, wait. Check stop signal too.
                    tokio::select! {
                        ret = vad_tx.send(evt) => {
                            if ret.is_err() {
                                log::warn!("vad channel closed, stopping main loop");
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

/// 处理一个 AsrEvent：emit 字幕 + (final 时) 入库。两条 drainer 共用。
async fn handle_asr_event(app: &AppHandle, evt: AsrEvent, session_guid: &str, last_text: &mut String) {
    match evt {
        AsrEvent::Partial { text, start_ts, end_ts: _ } => {
            log::info!("asr delta partial: {:?}", text);
            *last_text = text.clone();
            emit_subtitle_update(app, SubtitleUpdate {
                state: "partial".into(),
                text,
                start_ts,
                end_ts: None,
                paralinguistic: None,
            });
        }
        AsrEvent::Final { text, start_ts, end_ts } => {
            let final_text = if text.is_empty() { last_text.clone() } else { text };
            log::info!("asr done final: {:?}", final_text);
            if let Some(state) = app.try_state::<AppState>() {
                let pool = state.pool.clone();
                let _ = insert_transcription(
                    &pool,
                    session_guid,
                    &final_text,
                    start_ts,
                    end_ts,
                    None,
                )
                .await;
            }
            emit_subtitle_update(app, SubtitleUpdate {
                state: "final".into(),
                text: final_text,
                start_ts,
                end_ts: Some(end_ts),
                paralinguistic: None,
            });
            emit_asr_status(app, true, 0, None);
        }
        AsrEvent::Error { message } => {
            log::warn!("asr error: {message}");
            emit_asr_status(app, true, 0, Some(&message));
        }
    }
}

async fn run_drainer(
    app: AppHandle,
    mut vad_rx: mpsc::Receiver<VadEvent>,
    asr: AsrConfig,
    session_guid: String,
) {
    match asr {
        AsrConfig::Stepfun { api_key } => {
            run_drainer_stepfun(app, &mut vad_rx, api_key, session_guid).await
        }
        AsrConfig::Volc { api_key, resource_id, url } => {
            run_drainer_volc(app, &mut vad_rx, api_key, resource_id, url, session_guid).await
        }
    }
}

/// stepfun：one-shot SSE，整段提交。行为与事件化前完全一致——
/// 把 SpeechFrame 攒回整段（含尾部静音帧），SpeechEnd 时调 client::submit_utterance。
async fn run_drainer_stepfun(
    app: AppHandle,
    vad_rx: &mut mpsc::Receiver<VadEvent>,
    api_key: String,
    session_guid: String,
) {
    while let Some(first_evt) = vad_rx.recv().await {
        let start_ts = match first_evt {
            VadEvent::SpeechStart { start_ts } => start_ts,
            _ => continue, // 静音态不应有 Frame/End；跳过
        };

        // 攒整段（与事件化前 Utterance.samples 逐字节一致）
        let mut samples: Vec<f32> = Vec::new();
        let mut end_ts = start_ts;
        loop {
            match vad_rx.recv().await {
                Some(VadEvent::SpeechFrame { samples: fr }) => samples.extend_from_slice(&fr),
                Some(VadEvent::SpeechEnd { end_ts: et, .. }) => {
                    end_ts = et;
                    break;
                }
                Some(VadEvent::SpeechStart { .. }) => break, // 不应发生，防御
                None => return, // main 退出（stop）
            }
        }

        let pcm_s16le = crate::audio::dsp::quantize_to_pcm_s16le(&samples);
        log::info!(
            "sse submitting utterance: pcm_bytes={} start={} end={}",
            pcm_s16le.len(),
            start_ts,
            end_ts
        );
        let mut event_rx = match submit_stepfun(&api_key, pcm_s16le, start_ts, end_ts).await {
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
            handle_asr_event(&app, evt, &session_guid, &mut last_text).await;
        }
    }
}

/// volc：真正流式。开口即建 VolcStream，边喂音频边转发 partial，断句 finish 收 final。
async fn run_drainer_volc(
    app: AppHandle,
    vad_rx: &mut mpsc::Receiver<VadEvent>,
    api_key: String,
    resource_id: String,
    url: String,
    session_guid: String,
) {
    while let Some(first_evt) = vad_rx.recv().await {
        let start_ts = match first_evt {
            VadEvent::SpeechStart { start_ts } => start_ts,
            _ => continue,
        };

        let (mut stream, mut events_rx) = match VolcStream::start(&api_key, &resource_id, &url, start_ts).await {
            Ok(s) => {
                log::info!("volc stream started, feeding audio");
                emit_asr_status(&app, true, 0, None);
                s
            }
            Err(e) => {
                log::warn!("volc start failed: {e}");
                emit_asr_status(&app, true, 0, Some(&format!("volc start: {e}")));
                // 放弃本段，drain 到 SpeechEnd 以重新同步
                drain_until_end(vad_rx).await;
                continue;
            }
        };

        // 转发任务：把 AsrEvent（partial/final/error）emit 出去
        let app2 = app.clone();
        let sg2 = session_guid.clone();
        let fwd = tokio::spawn(async move {
            let mut last_text = String::new();
            while let Some(evt) = events_rx.recv().await {
                handle_asr_event(&app2, evt, &sg2, &mut last_text).await;
            }
        });

        // 喂音频：SpeechFrame 攒到 ~100ms 发一次，SpeechEnd 时 flush + finish
        let mut chunk_acc: Vec<f32> = Vec::new();
        let mut end_ts = start_ts;
        let mut aborted = false;
        loop {
            match vad_rx.recv().await {
                Some(VadEvent::SpeechFrame { samples: fr }) => {
                    chunk_acc.extend_from_slice(&fr);
                    if chunk_acc.len() >= STREAM_CHUNK_SAMPLES {
                        let pcm = crate::audio::dsp::quantize_to_pcm_s16le(&chunk_acc);
                        if let Err(e) = stream.send_audio(&pcm).await {
                            log::warn!("volc send_audio failed: {e}");
                            aborted = true;
                            break;
                        }
                        chunk_acc.clear();
                    }
                }
                Some(VadEvent::SpeechEnd { end_ts: et, .. }) => {
                    end_ts = et;
                    if !chunk_acc.is_empty() {
                        let pcm = crate::audio::dsp::quantize_to_pcm_s16le(&chunk_acc);
                        let _ = stream.send_audio(&pcm).await;
                    }
                    if let Err(e) = stream.finish(end_ts).await {
                        log::warn!("volc finish failed: {e}");
                        aborted = true;
                    }
                    break;
                }
                Some(VadEvent::SpeechStart { .. }) => {
                    // 不应发生；防御性结束当前段
                    let _ = stream.finish(end_ts).await;
                    break;
                }
                None => {
                    // main 退出（stop）：尽量收尾
                    let _ = stream.finish(end_ts).await;
                    aborted = true;
                    break;
                }
            }
        }

        // 等 final 转发完（带超时，防服务端不回 is_last）
        let _ = tokio::time::timeout(VOLC_FINAL_TIMEOUT, fwd).await;
        if aborted {
            // stop 或出错：不再等下一句
            return;
        }
    }
}

/// 丢弃 vad_rx 中直到（含）下一个 SpeechEnd 的事件，用于出错后重新同步。
async fn drain_until_end(vad_rx: &mut mpsc::Receiver<VadEvent>) {
    while let Some(evt) = vad_rx.recv().await {
        if matches!(evt, VadEvent::SpeechEnd { .. }) {
            break;
        }
    }
}
