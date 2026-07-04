/// One committed speech segment ready for ASR submission.
#[derive(Debug)]
pub struct Utterance {
    /// Mono f32 samples at 16kHz, range [-1, 1].
    pub samples: Vec<f32>,
    /// Wall-clock capture start time in ms.
    pub start_ts: i64,
    /// Wall-clock capture end time in ms.
    pub end_ts: i64,
}

const SILENCE_DURATION_MS: i64 = 800;
const MAX_SPEECH_MS: i64 = 10_000;
const SPEECH_THRESHOLD: f32 = 0.5;

enum State {
    Silence,
    Speech {
        samples: Vec<f32>,
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
    /// Returns Some(Utterance) when a segment is committed (silence ≥800ms or speech ≥10s).
    pub fn push_frame(&mut self, frame: &[f32], prob: f32, now_ms: i64) -> Option<Utterance> {
        match &mut self.state {
            State::Silence => {
                if prob > SPEECH_THRESHOLD {
                    let mut buf = Vec::with_capacity(16_000 * 12);
                    buf.extend_from_slice(frame);
                    self.state = State::Speech {
                        samples: buf,
                        start_ts: now_ms,
                        last_speech_ts: now_ms,
                    };
                }
                None
            }
            State::Speech {
                samples,
                start_ts,
                last_speech_ts,
            } => {
                samples.extend_from_slice(frame);
                if prob > SPEECH_THRESHOLD {
                    *last_speech_ts = now_ms;
                }

                // Force-commit takes priority so we cut at exactly 10s, not 10.3s.
                if now_ms - *start_ts >= MAX_SPEECH_MS {
                    let samples_owned = std::mem::take(samples);
                    let utt = Utterance {
                        samples: samples_owned,
                        start_ts: *start_ts,
                        end_ts: now_ms,
                    };
                    self.state = State::Silence;
                    return Some(utt);
                }

                if now_ms - *last_speech_ts >= SILENCE_DURATION_MS {
                    let samples_owned = std::mem::take(samples);
                    let utt = Utterance {
                        samples: samples_owned,
                        start_ts: *start_ts,
                        end_ts: *last_speech_ts,
                    };
                    self.state = State::Silence;
                    return Some(utt);
                }

                None
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

    #[test]
    fn silence_only_never_commits() {
        let mut v = VadState::new();
        let samples = vec![0.0; 512];
        for i in 0..1000 {
            assert!(v.push_frame(&samples, 0.1, ms(i * 32)).is_none());
        }
    }

    #[test]
    fn speech_then_silence_commits() {
        let mut v = VadState::new();
        let samples = vec![0.5; 512];
        // 10 frames of speech = 320ms
        for i in 0..10 {
            v.push_frame(&samples, 0.9, ms(i * 32));
        }
        // 25 frames of silence = 800ms exactly triggers commit at last_speech_ts+800
        // last_speech_ts = 9*32 = 288ms; commit when now - 288 >= 800 → now >= 1088
        // 1088 / 32 = 34, so frame at i=34 should trigger (now=34*32=1088)
        let mut committed = None;
        for i in 10..50 {
            if let Some(utt) = v.push_frame(&samples, 0.1, ms(i * 32)) {
                committed = Some(utt);
                break;
            }
        }
        let utt = committed.expect("should commit after 800ms silence");
        assert_eq!(utt.start_ts, 0);
        assert_eq!(utt.end_ts, 288);
        // 10 speech frames + 25 silence frames (i=10..34) before commit at i=34
        assert_eq!(utt.samples.len(), 512 * 35);
    }

    #[test]
    fn force_commit_at_10s() {
        let mut v = VadState::new();
        let samples = vec![0.5; 512];
        let mut committed = None;
        // Continuous speech for >10s. 10000/32 ≈ 313 frames.
        for i in 0..400 {
            if let Some(utt) = v.push_frame(&samples, 0.9, ms(i * 32)) {
                committed = Some(utt);
                break;
            }
        }
        let utt = committed.expect("should force-commit at 10s");
        // Force commit triggers when now - start >= 10000; start=0, so at i where i*32 >= 10000
        // i = 313 → now=10016ms
        assert!(utt.end_ts - utt.start_ts >= 10_000);
        assert!(utt.end_ts - utt.start_ts < 10_100);
    }

    #[test]
    fn force_commit_wins_over_silence() {
        // Speech for 9.7s, then silence. At 9.7s + 800ms = 10.5s, force-commit
        // should fire first at exactly 10s.
        let mut v = VadState::new();
        let samples = vec![0.5; 512];
        let mut committed_at = None;
        for i in 0..400 {
            let prob = if i < 304 { 0.9 } else { 0.1 }; // 304*32 = 9728ms last speech
            if let Some(utt) = v.push_frame(&samples, prob, ms(i * 32)) {
                committed_at = Some(utt.end_ts);
                break;
            }
        }
        let end_ts = committed_at.expect("should commit");
        // Force commit at 10000ms (i=313, now=10016), not 10528ms (silence).
        assert!(end_ts < 10_100, "force commit should win, end_ts={}", end_ts);
    }
}
