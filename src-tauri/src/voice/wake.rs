//! v0.28.58 — audio wake word via openWakeWord (Hey Jarvis).
//!
//! Runs the three-stage openWakeWord model chain in-process using
//! tract-onnx (pure Rust, no native ONNX Runtime bundle). Pipeline:
//!
//!   raw f32 mono 16kHz audio (1280-sample chunks, 80ms)
//!     -> melspectrogram.onnx  -> (8, 32) mel frames
//!     -> embedding_model.onnx -> (1, 1, 1, 96) embedding
//!     -> hey_jarvis_v0.1.onnx -> (1, 1) wake confidence
//!
//! Rolling buffers keep the last 10 mel chunks + 16 embeddings so the
//! two downstream models always have the fixed-size windows they were
//! trained on. The detection loop uses a smoothed rolling average
//! with a 2-second refractory so a single utterance can't retrigger.
//!
//! This is a direct port of the pipeline in the MIT-licensed `oww-rs`
//! crate (https://github.com/skoky/oww_rs); we couldn't use that
//! crate directly because it owns its own cpal stream, and Travis's
//! `voice::capture` already owns the mic. Feeding samples from the
//! existing tick loop is the whole point of writing this inline.
//!
//! Wake phrase is "Hey Jarvis" — openWakeWord doesn't ship a
//! pre-trained "Hey Travis" model. A custom-trained "Hey Travis"
//! model is a separate follow-up (needs ~500 audio samples + a
//! training run) but the runtime here is identical, only the wake
//! model file swaps.

use anyhow::{anyhow, Context, Result};
use circular_buffer::CircularBuffer;
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};
use tract_onnx::prelude::*;

/// Chunk size — the entire pipeline is aligned to 80ms frames at 16kHz.
pub const CHUNK_SAMPLES: usize = 1280;

/// Raw-audio carryover appended before each chunk so mel windows stay
/// continuous across chunk boundaries. Three mel hops of 160 samples.
const MEL_LOOKBACK: usize = 160 * 3;
/// Mel model input length = lookback + one chunk.
const MEL_INPUT_SIZE: usize = MEL_LOOKBACK + CHUNK_SAMPLES; // 1760
/// Mel frames produced per chunk = MEL_INPUT_SIZE / 160 - 3.
const MELS_PER_CHUNK: usize = MEL_INPUT_SIZE / 160 - 3; // 8

/// Number of chunks worth of mel frames to buffer. Embedding model
/// takes a 76-frame window, and we produce 8 frames per chunk, so we
/// keep 80 frames (10 chunks) and slice the newest 76.
const MEL_CIRC_SIZE: usize = 80 / MELS_PER_CHUNK; // 10

/// Wake model input is a 16-embedding window.
const FEATURE_BUFFER_SIZE: usize = 16;

/// Rolling detection buffer for smoothing the raw wake output.
const DETECTION_BUFFER_SIZE: usize = 12;
/// Minimum count of buffered detections above threshold before we
/// average them — one loud burst shouldn't fire.
const MIN_POSITIVE_DETECTIONS: f32 = 2.0;
/// Refractory period so one detection doesn't retrigger for 2s.
const NO_DETECTION_MS: u128 = 2000;

/// Default probability threshold for a wake decision. openWakeWord's
/// reference implementations run around 0.5–0.7; 0.5 is the safe
/// baseline for the hey_jarvis_v0.1 model.
pub const DEFAULT_THRESHOLD: f32 = 0.5;

type Runnable = TypedRunnableModel<TypedModel>;

pub struct WakeDetector {
    mel: Runnable,
    emb: Runnable,
    wake: Runnable,
    raw_lookback: Vec<f32>,
    mel_buffer: Box<CircularBuffer<MEL_CIRC_SIZE, Tensor>>,
    feature_buffer: Box<CircularBuffer<FEATURE_BUFFER_SIZE, Tensor>>,
    detections: Box<CircularBuffer<DETECTION_BUFFER_SIZE, f32>>,
    threshold: f32,
    last_detection_ms: u128,
    /// Startup time reference for last-detection timestamps.
    boot: std::time::Instant,
}

impl WakeDetector {
    pub fn load_from_resources(app: &AppHandle, threshold: f32) -> Result<Self> {
        let dir = app
            .path()
            .resource_dir()
            .context("resolve resource dir")?
            .join("resources")
            .join("wake");
        let dir = if dir.exists() {
            dir
        } else {
            // Dev / non-bundled runs: resources/ isn't wrapped by
            // Tauri, so `resource_dir()` returns the app's own root.
            app.path()
                .resource_dir()
                .context("resolve resource dir")?
                .join("wake")
        };
        Self::load(
            &dir.join("melspectrogram.onnx"),
            &dir.join("embedding_model.onnx"),
            &dir.join("hey_jarvis_v0.1.onnx"),
            threshold,
        )
    }

    pub fn load(mel: &Path, emb: &Path, wake: &Path, threshold: f32) -> Result<Self> {
        let mel = tract_onnx::onnx()
            .model_for_path(mel)
            .with_context(|| format!("load melspec {}", mel.display()))?
            .with_input_fact(0, f32::fact([1, MEL_INPUT_SIZE]).into())
            .context("mel input fact")?
            .into_optimized()
            .context("mel optimize")?
            .into_runnable()
            .context("mel runnable")?;
        let emb = tract_onnx::onnx()
            .model_for_path(emb)
            .with_context(|| format!("load embed {}", emb.display()))?
            .with_input_fact(0, f32::fact([1, 76, 32, 1]).into())
            .context("emb input fact")?
            .into_optimized()
            .context("emb optimize")?
            .into_runnable()
            .context("emb runnable")?;
        let wake = tract_onnx::onnx()
            .model_for_path(wake)
            .with_context(|| format!("load wake {}", wake.display()))?
            .into_optimized()
            .context("wake optimize")?
            .into_runnable()
            .context("wake runnable")?;

        let mut feature_buffer =
            CircularBuffer::<FEATURE_BUFFER_SIZE, Tensor>::boxed();
        for _ in 0..FEATURE_BUFFER_SIZE {
            feature_buffer.push_back(
                Tensor::from_shape(&[1, 1, 1, 96], &[0f32; 96])
                    .expect("zero-init embedding tensor"),
            );
        }
        let mut mel_buffer = CircularBuffer::<MEL_CIRC_SIZE, Tensor>::boxed();
        for _ in 0..MEL_CIRC_SIZE {
            mel_buffer.push_back(
                Tensor::from_shape(
                    &[MELS_PER_CHUNK, 32],
                    &[0f32; MELS_PER_CHUNK * 32],
                )
                .expect("zero-init mel tensor"),
            );
        }
        Ok(WakeDetector {
            mel,
            emb,
            wake,
            raw_lookback: vec![0f32; MEL_LOOKBACK],
            mel_buffer,
            feature_buffer,
            detections: CircularBuffer::<DETECTION_BUFFER_SIZE, f32>::boxed(),
            threshold,
            last_detection_ms: 0,
            boot: std::time::Instant::now(),
        })
    }

    /// Feed one exactly-`CHUNK_SAMPLES` (1280) f32 sample chunk.
    /// Returns Some((probability, detected)) on every chunk; `detected`
    /// is only true when the smoothed average crosses the threshold
    /// and we're outside the refractory window.
    pub fn feed_chunk(&mut self, chunk: &[f32]) -> Result<(f32, bool)> {
        if chunk.len() != CHUNK_SAMPLES {
            return Err(anyhow!(
                "wake: chunk must be exactly {CHUNK_SAMPLES} samples, got {}",
                chunk.len()
            ));
        }
        let mel = self.compute_mel(chunk)?;
        self.mel_buffer.push_back(mel);

        // Stack the mel chunks -> (80, 32), slice the newest 76 rows,
        // reshape to embedding model input.
        let stacked_mels =
            Tensor::stack_tensors(0, &self.mel_buffer.to_vec()).context("stack mels")?;
        let slice = stacked_mels.slice(0, 4, 80).context("slice mels")?;
        let mel_window = slice
            .into_shape(&[1, 76, 32, 1])
            .context("reshape mels for emb")?;
        let emb_out = self
            .emb
            .run(tvec!(mel_window.into()))
            .context("emb run")?;
        self.feature_buffer
            .push_back(emb_out[0].clone().into_tensor());

        // Stack 16 embeddings -> (16, 1, 1, 96) then reshape to
        // (1, 16, 96) for the wake model.
        let stacked = Tensor::stack_tensors(0, &self.feature_buffer.to_vec())
            .context("stack embeddings")?;
        let wake_in = stacked
            .into_shape(&[1, FEATURE_BUFFER_SIZE, 96])
            .context("reshape for wake")?;
        let wake_out = self
            .wake
            .run(tvec!(wake_in.into()))
            .context("wake run")?;
        let out_tensor = wake_out[0]
            .clone()
            .into_tensor()
            .cast_to::<f32>()
            .context("wake -> f32")?
            .into_owned();
        let probability = out_tensor
            .into_array::<f32>()
            .context("wake to ndarray")?
            .as_slice()
            .and_then(|s| s.first().copied())
            .unwrap_or(0.0);

        self.detections.push_back(probability);
        let avg = self.smoothed_average();
        let now = self.boot.elapsed().as_millis();
        let past_refractory = now.saturating_sub(self.last_detection_ms) > NO_DETECTION_MS;
        if avg > self.threshold && past_refractory {
            self.last_detection_ms = now;
            self.detections.clear();
            return Ok((avg, true));
        }
        Ok((avg, false))
    }

    fn compute_mel(&mut self, chunk: &[f32]) -> Result<Tensor> {
        // Concatenate lookback + chunk into MEL_INPUT_SIZE samples.
        let mut input = Vec::with_capacity(MEL_INPUT_SIZE);
        input.extend_from_slice(&self.raw_lookback);
        input.extend_from_slice(chunk);
        self.raw_lookback
            .copy_from_slice(&chunk[chunk.len() - MEL_LOOKBACK..]);

        let tensor =
            Tensor::from_shape(&[1, MEL_INPUT_SIZE], &input).context("mel input tensor")?;
        let out = self.mel.run(tvec!(tensor.into())).context("mel run")?;
        let out = out[0]
            .clone()
            .into_tensor()
            .into_shape(&[MELS_PER_CHUNK, 32])
            .context("reshape mel out")?;
        // openWakeWord scales mel output by (v/10)+2 to fit the
        // embedding model's expected input distribution.
        let arr = out
            .into_array::<f32>()
            .context("mel to ndarray")?
            .mapv(|v| v / 10.0 + 2.0);
        Ok(arr.into_tensor())
    }

    fn smoothed_average(&self) -> f32 {
        let mut positive_count = 0.0f32;
        let mut sum = 0.0f32;
        for &d in self.detections.iter() {
            if d > self.threshold {
                positive_count += 1.0;
                sum += d;
            }
        }
        if positive_count < MIN_POSITIVE_DETECTIONS {
            return 0.0;
        }
        let avg = sum / positive_count;
        if avg > self.threshold {
            avg
        } else {
            0.0
        }
    }
}

/// Fire the wake event to the frontend. useNativeVoice listens for
/// `voice://wake-detected` and dispatches `travis:arm-voice`.
pub fn emit_wake(app: &AppHandle, probability: f32) {
    let _ = app.emit("voice://wake-detected", probability);
}
