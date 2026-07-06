use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::io::{Read, Write};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use super::client::AsrEvent;
use crate::config::DEFAULT_VOLC_MODEL;

// ---- 二进制协议常量（移植自 Python 参考实现）----
const PROTOCOL_VERSION_V1: u8 = 0b0001;
const HEADER_SIZE: u8 = 1; // header 占 4 字节

const MSG_CLIENT_FULL: u8 = 0b0001;
const MSG_CLIENT_AUDIO_ONLY: u8 = 0b0010;
const MSG_SERVER_FULL: u8 = 0b1001;
const MSG_SERVER_ERROR: u8 = 0b1111;

const FLAG_POS_SEQ: u8 = 0b0001;
const FLAG_NEG_WITH_SEQ: u8 = 0b0011; // 最后一包：seq 取负
// 响应 flags 位含义：& 0x01 has sequence，& 0x02 is_last，& 0x04 has event

const SER_JSON: u8 = 0b0001;
const COMP_GZIP: u8 = 0b0001;

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).context("gzip write")?;
    enc.finish().context("gzip finish")
}

fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut dec = GzDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).context("gzip read")?;
    Ok(out)
}

fn build_header(msg_type: u8, flags: u8) -> [u8; 4] {
    [
        (PROTOCOL_VERSION_V1 << 4) | HEADER_SIZE,
        (msg_type << 4) | flags,
        (SER_JSON << 4) | COMP_GZIP,
        0x00,
    ]
}

fn build_full_request(seq: i32) -> Result<Vec<u8>> {
    let payload = serde_json::json!({
        "user": { "uid": "demo_uid" },
        "audio": { "format": "pcm", "codec": "raw", "rate": 16000, "bits": 16, "channel": 1 },
        "request": {
            "model_name": DEFAULT_VOLC_MODEL,
            "enable_itn": true,
            "enable_punc": true,
            "enable_ddc": true,
            "show_utterances": true,
            "enable_nonstream": false
        }
    });
    let payload_bytes = serde_json::to_vec(&payload)?;
    let compressed = gzip_compress(&payload_bytes)?;
    let mut req = Vec::with_capacity(4 + 4 + 4 + compressed.len());
    req.extend_from_slice(&build_header(MSG_CLIENT_FULL, FLAG_POS_SEQ));
    req.extend_from_slice(&seq.to_be_bytes());
    req.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    req.extend_from_slice(&compressed);
    Ok(req)
}

fn build_audio_request(seq: i32, segment: &[u8], is_last: bool) -> Result<Vec<u8>> {
    let (flags, seq_field) = if is_last {
        (FLAG_NEG_WITH_SEQ, -seq)
    } else {
        (FLAG_POS_SEQ, seq)
    };
    let compressed = gzip_compress(segment)?;
    let mut req = Vec::with_capacity(4 + 4 + 4 + compressed.len());
    req.extend_from_slice(&build_header(MSG_CLIENT_AUDIO_ONLY, flags));
    req.extend_from_slice(&seq_field.to_be_bytes());
    req.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    req.extend_from_slice(&compressed);
    Ok(req)
}

struct ParsedResponse {
    code: i32,
    is_last: bool,
    payload_msg: Option<Value>,
}

fn parse_response(msg: &[u8]) -> Result<ParsedResponse> {
    if msg.len() < 4 {
        anyhow::bail!("response too short");
    }
    let header_size = (msg[0] & 0x0f) as usize;
    let message_type = msg[1] >> 4;
    let flags = msg[1] & 0x0f;
    let serialization = msg[2] >> 4;
    let compression = msg[2] & 0x0f;

    let mut payload = &msg[header_size * 4..];
    let mut resp = ParsedResponse {
        code: 0,
        is_last: false,
        payload_msg: None,
    };

    if flags & 0x01 != 0 {
        if payload.len() < 4 {
            anyhow::bail!("truncated sequence");
        }
        let _seq = i32::from_be_bytes(payload[0..4].try_into().unwrap());
        payload = &payload[4..];
    }
    if flags & 0x02 != 0 {
        resp.is_last = true;
    }
    if flags & 0x04 != 0 {
        if payload.len() < 4 {
            anyhow::bail!("truncated event");
        }
        let _event = i32::from_be_bytes(payload[0..4].try_into().unwrap());
        payload = &payload[4..];
    }

    let body: &[u8] = match message_type {
        MSG_SERVER_FULL => {
            if payload.len() < 4 {
                anyhow::bail!("truncated full resp size");
            }
            let size = u32::from_be_bytes(payload[0..4].try_into().unwrap()) as usize;
            payload = &payload[4..];
            &payload[..payload.len().min(size)]
        }
        MSG_SERVER_ERROR => {
            if payload.len() < 8 {
                anyhow::bail!("truncated error resp");
            }
            resp.code = i32::from_be_bytes(payload[0..4].try_into().unwrap());
            let _size = u32::from_be_bytes(payload[4..8].try_into().unwrap());
            &payload[8..]
        }
        _ => &[][..],
    };

    if body.is_empty() {
        log::info!(
            "volc resp: type=0x{:x} flags=0b{:04b} is_last={} code={} (no body)",
            message_type, flags, resp.is_last, resp.code
        );
        return Ok(resp);
    }

    let raw = if compression == COMP_GZIP {
        gzip_decompress(body)?
    } else {
        body.to_vec()
    };
    let preview: String = String::from_utf8_lossy(&raw).chars().take(400).collect();
    if serialization == SER_JSON {
        match serde_json::from_slice::<Value>(&raw) {
            Ok(v) => resp.payload_msg = Some(v),
            Err(e) => log::warn!("volc payload json parse: {e}; raw={}", preview),
        }
    }
    log::info!(
        "volc resp: type=0x{:x} flags=0b{:04b} is_last={} code={} body={}",
        message_type, flags, resp.is_last, resp.code, preview
    );
    Ok(resp)
}

fn extract_text(payload: &Value) -> String {
    if let Some(t) = payload
        .get("result")
        .and_then(|r| r.get("text"))
        .and_then(|t| t.as_str())
    {
        return t.to_string();
    }
    if let Some(utts) = payload
        .get("result")
        .and_then(|r| r.get("utterances"))
        .and_then(|u| u.as_array())
    {
        let mut s = String::new();
        for u in utts {
            if let Some(t) = u.get("text").and_then(|t| t.as_str()) {
                s.push_str(t);
            }
        }
        return s;
    }
    String::new()
}

// ---- 流式会话 ----

enum VolcCmd {
    Audio(Vec<u8>),
    Finish { end_ts: i64 },
}

/// 火山 SAUC 流式会话：开口即建连，边上传音频边收 partial，断句时 finish 收 final。
///
/// 用法：`start` 建连 → 循环 `send_audio` 喂 PCM 段（partial 会异步从事件接收器流出）→
/// `finish` 发 last 包，事件接收器随后收到 `Final`。
pub struct VolcStream {
    cmd_tx: mpsc::Sender<VolcCmd>,
}

impl VolcStream {
    /// 建立 WS 连接、发送 full request，返回 (handle, AsrEvent 接收器)。
    pub async fn start(
        api_key: &str,
        resource_id: &str,
        url: &str,
        start_ts: i64,
    ) -> Result<(Self, mpsc::Receiver<AsrEvent>)> {
        let req_id = Uuid::new_v4().to_string();
        log::info!("volc connecting: url={} resource_id={}", url, resource_id);
        let mut request = url.into_client_request().context("ws request")?;
        let headers = request.headers_mut();
        headers.insert("X-Api-Key", api_key.parse().context("api key header")?);
        headers.insert(
            "X-Api-Resource-Id",
            resource_id.parse().context("resource id header")?,
        );
        headers.insert(
            "X-Api-Request-Id",
            req_id.parse().context("request id header")?,
        );
        headers.insert(
            "X-Api-Connect-Id",
            req_id.parse().context("connect id header")?,
        );
        headers.insert("X-Api-Sequence", "-1".parse().unwrap());

        let mut ws = match connect_async(request).await {
            Ok((ws, _)) => {
                log::info!("volc ws connected");
                ws
            }
            Err(e) => return Err(anyhow::anyhow!("volc ws connect: {e}")),
        };

        // 发送 full client request（seq=1）
        let full = build_full_request(1)?;
        ws.send(Message::Binary(full))
            .await
            .context("send full request")?;

        let (cmd_tx, mut cmd_rx) = mpsc::channel::<VolcCmd>(32);
        let (evt_tx, evt_rx) = mpsc::channel::<AsrEvent>(32);

        // split 成 sink + stream，select! 并发：边发音频边收响应。
        let (mut sink, mut stream) = ws.split();

        tokio::spawn(async move {
            let mut seq: i32 = 2; // 音频 seq 从 2 起（1 是 full request）
            let mut accumulated = String::new();
            let mut finished = false;
            let mut end_ts: i64 = start_ts;

            loop {
                tokio::select! {
                    // 发送侧：未 finish 前处理音频/finish 命令
                    cmd = cmd_rx.recv(), if !finished => {
                        match cmd {
                            Some(VolcCmd::Audio(chunk)) => {
                                match build_audio_request(seq, &chunk, false) {
                                    Ok(req) => {
                                        if sink.send(Message::Binary(req)).await.is_err() {
                                            let _ = evt_tx.send(AsrEvent::Error {
                                                message: "ws send failed".into(),
                                            }).await;
                                            return;
                                        }
                                        seq += 1;
                                    }
                                    Err(e) => {
                                        let _ = evt_tx.send(AsrEvent::Error {
                                            message: format!("build audio: {e}"),
                                        }).await;
                                        return;
                                    }
                                }
                            }
                            Some(VolcCmd::Finish { end_ts: et }) => {
                                end_ts = et;
                                // 发 last 包（空 payload + NEG_WITH_SEQ）
                                match build_audio_request(seq, &[], true) {
                                    Ok(req) => {
                                        if let Err(e) = sink.send(Message::Binary(req)).await {
                                            let _ = evt_tx.send(AsrEvent::Error {
                                                message: format!("ws send last: {e}"),
                                            }).await;
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        let _ = evt_tx.send(AsrEvent::Error {
                                            message: format!("build last: {e}"),
                                        }).await;
                                        return;
                                    }
                                }
                                finished = true;
                                log::info!("volc finish: last packet sent (seq={}), awaiting final", seq);
                            }
                            None => {
                                // drainer 提前退出（stop 等），中止
                                log::info!("volc stream cmd channel closed, aborting");
                                return;
                            }
                        }
                    }
                    // 接收侧：始终轮询服务端响应
                    msg = stream.next() => {
                        let data = match msg {
                            Some(Ok(Message::Binary(b))) => b,
                            Some(Ok(Message::Close(_))) => {
                                log::info!("volc ws closed by server");
                                return;
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(e)) => {
                                let _ = evt_tx.send(AsrEvent::Error {
                                    message: format!("ws: {e}"),
                                }).await;
                                return;
                            }
                            None => {
                                log::info!("volc ws stream ended");
                                return;
                            }
                        };
                        let parsed = match parse_response(&data) {
                            Ok(p) => p,
                            Err(e) => {
                                log::warn!("volc parse: {e}");
                                continue;
                            }
                        };
                        if parsed.code != 0 {
                            let _ = evt_tx.send(AsrEvent::Error {
                                message: format!("volc code {}", parsed.code),
                            }).await;
                            return;
                        }
                        if let Some(payload) = &parsed.payload_msg {
                            let text = extract_text(payload);
                            if !text.is_empty() {
                                accumulated = text.clone();
                            }
                        }
                        if parsed.is_last {
                            let _ = evt_tx.send(AsrEvent::Final {
                                text: accumulated,
                                start_ts,
                                end_ts,
                            }).await;
                            return;
                        } else if let Some(payload) = &parsed.payload_msg {
                            let text = extract_text(payload);
                            if !text.is_empty() {
                                if evt_tx.send(AsrEvent::Partial {
                                    text,
                                    start_ts,
                                    end_ts: start_ts,
                                }).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok((VolcStream { cmd_tx }, evt_rx))
    }

    /// 发送一段 PCM s16le 音频（非 last）。
    pub async fn send_audio(&mut self, pcm_s16le: &[u8]) -> Result<()> {
        self.cmd_tx
            .send(VolcCmd::Audio(pcm_s16le.to_vec()))
            .await
            .context("volc stream closed")
    }

    /// 发送 last 包并标记结束。后续 AsrEvent 接收器会收到 Final。
    pub async fn finish(self, end_ts: i64) -> Result<()> {
        self.cmd_tx
            .send(VolcCmd::Finish { end_ts })
            .await
            .context("volc stream closed before finish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_encodes_correctly() {
        let h = build_header(MSG_CLIENT_FULL, FLAG_POS_SEQ);
        assert_eq!(h[0], (PROTOCOL_VERSION_V1 << 4) | HEADER_SIZE);
        assert_eq!(h[1], (MSG_CLIENT_FULL << 4) | FLAG_POS_SEQ);
        assert_eq!(h[2], (SER_JSON << 4) | COMP_GZIP);
        assert_eq!(h[3], 0x00);
    }

    #[test]
    fn gzip_roundtrip() {
        let data = b"hello world";
        let comp = gzip_compress(data).unwrap();
        assert_eq!(gzip_decompress(&comp).unwrap(), data);
    }

    #[test]
    fn parse_full_response() {
        let json = r#"{"result":{"text":"你好"}}"#.as_bytes();
        let comp = gzip_compress(json).unwrap();
        let mut msg = Vec::new();
        msg.push((PROTOCOL_VERSION_V1 << 4) | HEADER_SIZE);
        msg.push((MSG_SERVER_FULL << 4) | FLAG_POS_SEQ);
        msg.push((SER_JSON << 4) | COMP_GZIP);
        msg.push(0x00);
        msg.extend_from_slice(&1i32.to_be_bytes());
        msg.extend_from_slice(&(comp.len() as u32).to_be_bytes());
        msg.extend_from_slice(&comp);

        let resp = parse_response(&msg).unwrap();
        assert_eq!(resp.code, 0);
        assert!(!resp.is_last);
        assert_eq!(extract_text(resp.payload_msg.as_ref().unwrap()), "你好");
    }

    #[test]
    fn parse_last_response() {
        let json = r#"{"result":{"text":"你好世界"}}"#.as_bytes();
        let comp = gzip_compress(json).unwrap();
        let mut msg = Vec::new();
        msg.push((PROTOCOL_VERSION_V1 << 4) | HEADER_SIZE);
        // flags: POS_SEQ (0x01) | is_last (0x02) = 0x03
        msg.push((MSG_SERVER_FULL << 4) | (FLAG_POS_SEQ | 0x02));
        msg.push((SER_JSON << 4) | COMP_GZIP);
        msg.push(0x00);
        msg.extend_from_slice(&1i32.to_be_bytes());
        msg.extend_from_slice(&(comp.len() as u32).to_be_bytes());
        msg.extend_from_slice(&comp);

        let resp = parse_response(&msg).unwrap();
        assert!(resp.is_last);
        assert_eq!(extract_text(resp.payload_msg.as_ref().unwrap()), "你好世界");
    }

    #[test]
    fn parse_error_response() {
        let json = r#"{"message":"bad request"}"#.as_bytes();
        let comp = gzip_compress(json).unwrap();
        let mut msg = Vec::new();
        msg.push((PROTOCOL_VERSION_V1 << 4) | HEADER_SIZE);
        msg.push((MSG_SERVER_ERROR << 4) | FLAG_POS_SEQ);
        msg.push((SER_JSON << 4) | COMP_GZIP);
        msg.push(0x00);
        msg.extend_from_slice(&1i32.to_be_bytes());
        msg.extend_from_slice(&1001i32.to_be_bytes()); // code
        msg.extend_from_slice(&(comp.len() as u32).to_be_bytes());
        msg.extend_from_slice(&comp);

        let resp = parse_response(&msg).unwrap();
        assert_eq!(resp.code, 1001);
    }
}
