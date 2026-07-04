use anyhow::Result;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

/// Target format required by StepAudio: 16kHz / mono / pcm_s16le.
pub const TARGET_RATE: usize = 16_000;
pub const FRAME_MS: usize = 40;
pub const FRAME_SAMPLES: usize = TARGET_RATE * FRAME_MS / 1000; // 640 samples
pub const FRAME_BYTES: usize = FRAME_SAMPLES * 2; // 1280 bytes

/// Downmix interleaved stereo f32 (L, R, L, R, ...) to mono f32.
pub fn downmix_stereo_to_mono(input: &[f32]) -> Vec<f32> {
    let n = input.len() / 2;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let l = input[i * 2];
        let r = input[i * 2 + 1];
        out.push((l + r) * 0.5);
    }
    out
}

/// Hard clip into [-1.0, 1.0].
#[inline]
pub fn hard_clip(x: f32) -> f32 {
    x.clamp(-1.0, 1.0)
}

/// Quantize f32 in [-1, 1] to pcm_s16le little-endian bytes.
pub fn quantize_to_pcm_s16le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = hard_clip(s);
        let v = (clamped * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Stateful DSP pipe: accumulates mono f32 at source rate, resamples to 16kHz,
/// emits 1280-byte (40ms) pcm_s16le frames.
pub struct DspPipe {
    resampler: SincFixedIn<f32>,
    source_rate: usize,
    mono_buffer: Vec<f32>,
    resampled_buffer: Vec<f32>,
    frame_buffer: Vec<u8>,
}

impl DspPipe {
    pub fn new(source_rate: usize) -> Result<Self> {
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        // Chunk size at source rate that yields ~40ms of source audio.
        let chunk = source_rate * 40 / 1000;
        let resampler = SincFixedIn::<f32>::new(
            TARGET_RATE as f64 / source_rate as f64,
            2.0,
            params,
            chunk,
            1,
        )?;
        Ok(Self {
            resampler,
            source_rate,
            mono_buffer: Vec::new(),
            resampled_buffer: Vec::new(),
            frame_buffer: Vec::new(),
        })
    }

    /// Push interleaved stereo f32 samples into the pipe.
    pub fn push_interleaved(&mut self, stereo: &[f32]) -> Result<()> {
        let mono = downmix_stereo_to_mono(stereo);
        self.mono_buffer.extend_from_slice(&mono);
        self.drain_resampler()?;
        Ok(())
    }

    fn drain_resampler(&mut self) -> Result<()> {
        let chunk = self.resampler.input_frames_next();
        while self.mono_buffer.len() >= chunk {
            let input: Vec<f32> = self.mono_buffer.drain(..chunk).collect();
            let mut waves_in = Vec::new();
            waves_in.push(input.clone());
            let out = self.resampler.process(&waves_in, None)?;
            if let Some(ch) = out.into_iter().next() {
                self.resampled_buffer.extend_from_slice(&ch);
            }
        }
        Ok(())
    }

    /// Returns next 640-sample (40ms) mono f32 frame at 16kHz if available.
    pub fn next_f32_frame(&mut self) -> Option<Vec<f32>> {
        if self.resampled_buffer.len() >= FRAME_SAMPLES {
            let chunk: Vec<f32> = self.resampled_buffer.drain(..FRAME_SAMPLES).collect();
            Some(chunk)
        } else {
            None
        }
    }

    /// Returns next 1280-byte pcm_s16le frame if available.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        while let Some(chunk) = self.next_f32_frame() {
            let bytes = quantize_to_pcm_s16le(&chunk);
            self.frame_buffer.extend_from_slice(&bytes);
        }
        if self.frame_buffer.len() >= FRAME_BYTES {
            let frame: Vec<u8> = self.frame_buffer.drain(..FRAME_BYTES).collect();
            Some(frame)
        } else {
            None
        }
    }

    pub fn source_rate(&self) -> usize {
        self.source_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_stereo() {
        let stereo = [1.0, -1.0, 0.5, 0.5];
        let mono = downmix_stereo_to_mono(&stereo);
        assert_eq!(mono, vec![0.0, 0.5]);
    }

    #[test]
    fn hard_clip_clamps() {
        assert_eq!(hard_clip(2.0), 1.0);
        assert_eq!(hard_clip(-2.0), -1.0);
        assert_eq!(hard_clip(0.3), 0.3);
    }

    #[test]
    fn quantize_produces_le_bytes() {
        let samples = [0.0, 1.0, -1.0];
        let bytes = quantize_to_pcm_s16le(&samples);
        assert_eq!(bytes.len(), 6);
        // 0.0 -> 0x00 0x00
        assert_eq!(&bytes[0..2], &[0x00, 0x00]);
        // 1.0 -> 0xFF 0x7F (32767 LE)
        assert_eq!(&bytes[2..4], &[0xFF, 0x7F]);
        // -1.0 -> 0x01 0x80 (-32767 LE; round(-32767.0) = -32767)
        assert_eq!(&bytes[4..6], &[0x01, 0x80]);
    }
}
