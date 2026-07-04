use anyhow::{anyhow, Result};
use include_dir::{include_dir, Dir};
use ndarray::{Array1, Array2, Array3};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::sync::Mutex;

static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");

const SAMPLES_PER_FRAME: usize = 512;
const CONTEXT_SIZE: usize = 64; // silero-vad v5: 64-sample context prepended to each 512-sample frame
const SR: i64 = 16_000;
const STATE_SHAPE: (usize, usize, usize) = (2, 1, 128);

pub struct SileroVad {
    session: Mutex<Session>,
    state: Mutex<Array3<f32>>,
    context: Mutex<Vec<f32>>,
}

impl SileroVad {
    pub fn new() -> Result<Self> {
        let model_bytes = ASSETS
            .get_file("silero_vad.onnx")
            .ok_or_else(|| anyhow!("silero_vad.onnx not found in src-tauri/assets/"))?
            .contents();
        let session = {
            let builder = Session::builder()
                .map_err(|e| anyhow!("ort builder: {e}"))?;
            builder
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| anyhow!("ort opt level: {e}"))?
                .with_intra_threads(1)
                .map_err(|e| anyhow!("ort threads: {e}"))?
                .commit_from_memory(model_bytes)
                .map_err(|e| anyhow!("ort commit: {e}"))?
        };
        Ok(Self {
            session: Mutex::new(session),
            state: Mutex::new(Array3::<f32>::zeros(STATE_SHAPE)),
            context: Mutex::new(vec![0.0; CONTEXT_SIZE]),
        })
    }

    pub fn reset(&self) {
        let mut s = self.state.lock().unwrap();
        s.fill(0.0);
        let mut ctx = self.context.lock().unwrap();
        ctx.fill(0.0);
    }

    /// Run inference on a 512-sample (32ms @16kHz) mono f32 frame.
    /// Returns speech probability in [0, 1].
    /// Silero-vad v5 requires prepending the previous 64 samples as context.
    pub fn predict(&self, samples: &[f32]) -> Result<f32> {
        if samples.len() != SAMPLES_PER_FRAME {
            return Err(anyhow!(
                "expected {} samples, got {}",
                SAMPLES_PER_FRAME,
                samples.len()
            ));
        }

        // Build input = [context(64) ; frame(512)] = 576 samples, shape [1, 576].
        let ctx = self.context.lock().unwrap().clone();
        let mut input_vec = Vec::with_capacity(CONTEXT_SIZE + SAMPLES_PER_FRAME);
        input_vec.extend_from_slice(&ctx);
        input_vec.extend_from_slice(samples);

        // Update context for next call: last 64 samples of this frame.
        {
            let mut c = self.context.lock().unwrap();
            c.copy_from_slice(&samples[SAMPLES_PER_FRAME - CONTEXT_SIZE..]);
        }

        let input = Array2::<f32>::from_shape_vec((1, CONTEXT_SIZE + SAMPLES_PER_FRAME), input_vec)
            .map_err(|e| anyhow!("input shape: {e}"))?;
        let sr = Array1::<i64>::from_vec(vec![SR]);
        let state = self.state.lock().unwrap().clone();

        let input_v = Tensor::from_array(input).map_err(|e| anyhow!("input tensor: {e}"))?;
        let state_v = Tensor::from_array(state).map_err(|e| anyhow!("state tensor: {e}"))?;
        let sr_v = Tensor::from_array(sr).map_err(|e| anyhow!("sr tensor: {e}"))?;

        let inputs = ort::inputs! {
            "input" => input_v,
            "state" => state_v,
            "sr" => sr_v,
        };

        let mut session = self.session.lock().unwrap();
        let outputs = session.run(inputs).map_err(|e| anyhow!("ort run: {e}"))?;

        let prob = {
            let out = outputs
                .get("output")
                .ok_or_else(|| anyhow!("model missing output 'output'"))?;
            let (_shape, data) = out
                .try_extract_tensor::<f32>()
                .map_err(|e| anyhow!("extract output: {e}"))?;
            data.first().copied().unwrap_or(0.0)
        };

        if let Some(new_state_val) = outputs.get("stateN") {
            let (_shape, data) = new_state_val
                .try_extract_tensor::<f32>()
                .map_err(|e| anyhow!("extract state: {e}"))?;
            let expected = STATE_SHAPE.0 * STATE_SHAPE.1 * STATE_SHAPE.2;
            if data.len() == expected {
                let owned = Array3::from_shape_vec(STATE_SHAPE, data.to_vec())
                    .map_err(|e| anyhow!("reshape state: {e}"))?;
                let mut s = self.state.lock().unwrap();
                s.assign(&owned);
            }
        }

        Ok(prob)
    }
}
