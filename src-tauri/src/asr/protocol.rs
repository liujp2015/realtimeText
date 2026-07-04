use serde::{Deserialize, Serialize};

const ENDPOINT: &str = "https://api.stepfun.com/step_plan/v1/audio/asr/sse";
const MODEL: &str = "stepaudio-2.5-asr";

#[derive(Debug, Clone, Serialize)]
pub struct AsrRequest {
    pub audio: AsrAudio,
}

#[derive(Debug, Clone, Serialize)]
pub struct AsrAudio {
    pub data: String, // base64-encoded pcm_s16le
    pub input: AsrInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct AsrInput {
    pub transcription: Transcription,
    pub format: AudioFormat,
}

#[derive(Debug, Clone, Serialize)]
pub struct Transcription {
    pub language: String,
    pub model: String,
    pub enable_itn: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioFormat {
    #[serde(rename = "type")]
    pub kind: String,
    pub codec: String,
    pub rate: u32,
    pub bits: u32,
    pub channel: u32,
}

impl AsrRequest {
    pub fn new(base64_audio: String) -> Self {
        Self {
            audio: AsrAudio {
                data: base64_audio,
                input: AsrInput {
                    transcription: Transcription {
                        language: "zh".into(),
                        model: MODEL.into(),
                        enable_itn: true,
                    },
                    format: AudioFormat {
                        kind: "pcm".into(),
                        codec: "pcm_s16le".into(),
                        rate: 16_000,
                        bits: 16,
                        channel: 1,
                    },
                },
            },
        }
    }
}

pub fn endpoint() -> &'static str {
    ENDPOINT
}

/// SSE event from the server.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    #[serde(rename = "transcript.text.delta")]
    Delta {
        #[serde(default)]
        delta: String,
        #[serde(default)]
        start_time: i64,
        #[serde(default)]
        end_time: i64,
    },
    #[serde(rename = "transcript.text.done")]
    Done {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: String,
    },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_correctly() {
        let req = AsrRequest::new("AAA=".into());
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"stepaudio-2.5-asr\""));
        assert!(json.contains("\"language\":\"zh\""));
        assert!(json.contains("\"enable_itn\":true"));
        assert!(json.contains("\"codec\":\"pcm_s16le\""));
        assert!(json.contains("\"rate\":16000"));
        assert!(json.contains("\"data\":\"AAA=\""));
    }

    #[test]
    fn parses_delta() {
        let raw = r#"{"type":"transcript.text.delta","delta":"你好","start_time":0,"end_time":500}"#;
        let e: SseEvent = serde_json::from_str(raw).unwrap();
        match e {
            SseEvent::Delta { delta, start_time, end_time } => {
                assert_eq!(delta, "你好");
                assert_eq!(start_time, 0);
                assert_eq!(end_time, 500);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_done() {
        let raw = r#"{"type":"transcript.text.done","text":"你好世界"}"#;
        let e: SseEvent = serde_json::from_str(raw).unwrap();
        match e {
            SseEvent::Done { text } => assert_eq!(text, "你好世界"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_error() {
        let raw = r#"{"type":"error","message":"bad request"}"#;
        let e: SseEvent = serde_json::from_str(raw).unwrap();
        match e {
            SseEvent::Error { message } => assert_eq!(message, "bad request"),
            _ => panic!("wrong variant"),
        }
    }
}
