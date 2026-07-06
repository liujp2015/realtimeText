/// VAD 事件流：语音段的开始 / 每帧音频 / 结束边界。
/// 检测逻辑（静音 800ms 断句、10s 强制断、阈值 0.5）与事件化前完全一致，
/// 仅把输出形态从"整段 Utterance"改为增量事件，供流式 ASR 边采集边上传。
#[derive(Debug)]
pub enum VadEvent {
    /// 语音态开始（Silence→Speech 转换的那一帧）。
    SpeechStart { start_ts: i64 },
    /// 语音态期间的每一帧（含尾部静音帧，与事件化前 Utterance.samples 一致）。
    SpeechFrame { samples: Vec<f32> },
    /// 语音段结束边界（静音≥800ms 或 语音≥10s 强制断）。
    SpeechEnd { start_ts: i64, end_ts: i64 },
}

const SILENCE_DURATION_MS: i64 = 800;
const MAX_SPEECH_MS: i64 = 10_000;
const SPEECH_THRESHOLD: f32 = 0.5;

enum State {
    Silence,
    Speech {
        start_ts: i64,
        last_speech_ts: i64,
    },
}

pub struct VadState {
    state: State,
}

impl VadState {
    pub fn new() -> Self {
        Self {
            state: State::Silence,
        }
    }

    /// Push a 512-sample (32ms) frame with its speech probability and wall-clock ts.
    /// 返回本帧产生的事件（通常 0 或 1 个；状态转换帧可能 2 个：Start+Frame 或 Frame+End）。
    pub fn push_frame(&mut self, frame: &[f32], prob: f32, now_ms: i64) -> Vec<VadEvent> {
        match &mut self.state {
            State::Silence => {
                if prob > SPEECH_THRESHOLD {
                    self.state = State::Speech {
                        start_ts: now_ms,
                        last_speech_ts: now_ms,
                    };
                    vec![
                        VadEvent::SpeechStart { start_ts: now_ms },
                        VadEvent::SpeechFrame {
                            samples: frame.to_vec(),
                        },
                    ]
                } else {
                    Vec::new()
                }
            }
            State::Speech {
                start_ts,
                last_speech_ts,
            } => {
                if prob > SPEECH_THRESHOLD {
                    *last_speech_ts = now_ms;
                }
                let mut events = vec![VadEvent::SpeechFrame {
                    samples: frame.to_vec(),
                }];

                // Force-commit 优先，保证在 10s 整切断而非 10.3s。
                if now_ms - *start_ts >= MAX_SPEECH_MS {
                    events.push(VadEvent::SpeechEnd {
                        start_ts: *start_ts,
                        end_ts: now_ms,
                    });
                    self.state = State::Silence;
                } else if now_ms - *last_speech_ts >= SILENCE_DURATION_MS {
                    events.push(VadEvent::SpeechEnd {
                        start_ts: *start_ts,
                        end_ts: *last_speech_ts,
                    });
                    self.state = State::Silence;
                }

                events
            }
        }
    }

    /// Reset state (e.g. on stop). Discards any in-progress speech without committing.
    pub fn reset(&mut self) {
        self.state = State::Silence;
    }
}

impl Default for VadState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(m: i64) -> i64 {
        m
    }

    /// 收集一个事件流里的 (start, end, frame_count)。
    fn collect_segment(events: &[VadEvent]) -> Option<(i64, i64, usize)> {
        let mut end = None;
        let mut frames = 0usize;
        for e in events {
            match e {
                VadEvent::SpeechStart { .. } => {}
                VadEvent::SpeechFrame { .. } => frames += 1,
                VadEvent::SpeechEnd {
                    start_ts,
                    end_ts,
                } => {
                    end = Some((*start_ts, *end_ts));
                }
            }
        }
        end.map(|(s, e)| (s, e, frames))
    }

    #[test]
    fn silence_only_never_commits() {
        let mut v = VadState::new();
        let samples = vec![0.0; 512];
        for i in 0..1000 {
            assert!(v.push_frame(&samples, 0.1, ms(i * 32)).is_empty());
        }
    }

    #[test]
    fn speech_then_silence_commits() {
        let mut v = VadState::new();
        let samples = vec![0.5; 512];
        // 10 frames of speech = 320ms
        let mut all_events = Vec::new();
        for i in 0..10 {
            all_events.extend(v.push_frame(&samples, 0.9, ms(i * 32)));
        }
        // 25 frames of silence = 800ms exactly triggers commit at last_speech_ts+800
        // last_speech_ts = 9*32 = 288ms; commit when now - 288 >= 800 → now >= 1088
        // 1088 / 32 = 34, so frame at i=34 should trigger (now=34*32=1088)
        for i in 10..50 {
            all_events.extend(v.push_frame(&samples, 0.1, ms(i * 32)));
            if all_events.iter().any(|e| matches!(e, VadEvent::SpeechEnd { .. })) {
                break;
            }
        }
        let (start, end, frames) = collect_segment(&all_events).expect("should commit");
        assert_eq!(start, 0);
        assert_eq!(end, 288);
        // 10 speech frames + 25 silence frames (i=10..34) before commit at i=34 → 35 frames total
        assert_eq!(frames, 35);
    }

    #[test]
    fn force_commit_at_10s() {
        let mut v = VadState::new();
        let samples = vec![0.5; 512];
        let mut all_events = Vec::new();
        // Continuous speech for >10s. 10000/32 ≈ 313 frames.
        for i in 0..400 {
            all_events.extend(v.push_frame(&samples, 0.9, ms(i * 32)));
            if all_events.iter().any(|e| matches!(e, VadEvent::SpeechEnd { .. })) {
                break;
            }
        }
        let (start, end, _) = collect_segment(&all_events).expect("should force-commit at 10s");
        // Force commit triggers when now - start >= 10000; start=0, so at i where i*32 >= 10000
        // i = 313 → now=10016ms
        assert!(end - start >= 10_000);
        assert!(end - start < 10_100);
    }

    #[test]
    fn force_commit_wins_over_silence() {
        // Speech for 9.7s, then silence. At 9.7s + 800ms = 10.5s, force-commit
        // should fire first at 10s.
        let mut v = VadState::new();
        let samples = vec![0.5; 512];
        let mut all_events = Vec::new();
        for i in 0..400 {
            let prob = if i < 304 { 0.9 } else { 0.1 }; // 304*32 = 9728ms last speech
            all_events.extend(v.push_frame(&samples, prob, ms(i * 32)));
            if all_events.iter().any(|e| matches!(e, VadEvent::SpeechEnd { .. })) {
                break;
            }
        }
        let (_start, end, _) = collect_segment(&all_events).expect("should commit");
        // Force commit at 10000ms (i=313, now=10016), not 10528ms (silence).
        assert!(end < 10_100, "force commit should win, end_ts={}", end);
    }
}
