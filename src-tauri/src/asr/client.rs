use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use futures_util::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;

use super::protocol::{endpoint, AsrRequest, SseEvent};

#[derive(Debug, Clone)]
pub enum AsrEvent {
    /// Incremental accumulated transcript text (overall replace in UI).
    Partial { text: String, start_ts: i64, end_ts: i64 },
    /// Final complete transcript for the utterance.
    Final { text: String, start_ts: i64, end_ts: i64 },
    Error { message: String },
}

/// Submit one utterance's PCM to the SSE endpoint and stream ASR events.
/// Returns a receiver from which the caller reads AsrEvent until None (stream end).
pub async fn submit_utterance(
    api_key: &str,
    pcm_s16le: Vec<u8>,
    start_ts: i64,
    end_ts: i64,
) -> Result<mpsc::Receiver<AsrEvent>> {
    let b64 = B64.encode(&pcm_s16le);
    let body = serde_json::to_value(AsrRequest::new(b64))?;

    let client = Client::builder()
        .build()
        .context("build reqwest client")?;

    let resp = client
        .post(endpoint())
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("sse request")?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("sse http {}: {}", status, text));
    }

    let (tx, rx) = mpsc::channel::<AsrEvent>(32);

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut accumulated = String::new();

    tokio::spawn(async move {
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    // SSE events separated by \n\n
                    while let Some(idx) = buffer.find("\n\n") {
                        let event_str = buffer[..idx].to_string();
                        buffer.drain(..idx + 2);
                        if let Some(evt) = parse_sse_event(&event_str) {
                            match evt {
                                SseEvent::Delta { delta, .. } => {
                                    accumulated.push_str(&delta);
                                    if tx
                                        .send(AsrEvent::Partial {
                                            text: accumulated.clone(),
                                            start_ts,
                                            end_ts,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                SseEvent::Done { text } => {
                                    let final_text = if text.is_empty() {
                                        accumulated.clone()
                                    } else {
                                        text
                                    };
                                    let _ = tx
                                        .send(AsrEvent::Final {
                                            text: final_text,
                                            start_ts,
                                            end_ts,
                                        })
                                        .await;
                                    return;
                                }
                                SseEvent::Error { message } => {
                                    let _ = tx.send(AsrEvent::Error { message }).await;
                                    return;
                                }
                                SseEvent::Other => {}
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("sse stream error: {e}");
                    let _ = tx
                        .send(AsrEvent::Error {
                            message: format!("stream error: {e}"),
                        })
                        .await;
                    return;
                }
            }
        }
        // Stream ended without Done event — emit a Final with accumulated text if any.
        if !accumulated.is_empty() {
            let _ = tx
                .send(AsrEvent::Final {
                    text: accumulated,
                    start_ts,
                    end_ts,
                })
                .await;
        }
    });

    Ok(rx)
}

/// Parse one SSE event block (lines between \n\n separators).
fn parse_sse_event(block: &str) -> Option<SseEvent> {
    let mut data_lines: Vec<&str> = Vec::new();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start_matches(' '));
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let data = data_lines.join("\n");
    if data == "[DONE]" {
        return None;
    }
    match serde_json::from_str::<SseEvent>(&data) {
        Ok(e) => Some(e),
        Err(e) => {
            log::debug!("unparseable sse data: {} ({})", data, e);
            None
        }
    }
}
