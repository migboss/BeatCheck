use anyhow::{anyhow, Result};
use crate::dsp::N_MELS;

/// Raw 50 fps beat & downbeat probability activations (values in `0.0..=1.0`).
#[derive(Debug, Clone)]
pub struct NeuralActivations {
    pub beat_probs: Vec<f32>,
    pub downbeat_probs: Vec<f32>,
    pub engine_name: String,
}

#[cfg(feature = "neural-onnx")]
const EMBEDDED_BEAT_THIS_SMALL0: &[u8] =
    include_bytes!("../../../assets/models/beat_this_small0.onnx");

#[cfg(feature = "neural-onnx")]
const CHUNK_FRAMES: usize = 1500;

#[cfg(feature = "neural-onnx")]
type CachedPlan = tract_onnx::prelude::SimplePlan<
    tract_onnx::prelude::TypedFact,
    Box<dyn tract_onnx::prelude::TypedOp>,
    tract_onnx::prelude::Graph<
        tract_onnx::prelude::TypedFact,
        Box<dyn tract_onnx::prelude::TypedOp>,
    >,
>;

#[cfg(feature = "neural-onnx")]
fn get_cached_model() -> Result<&'static CachedPlan> {
    use std::io::Cursor;
    use std::sync::OnceLock;
    use tract_onnx::prelude::*;

    static MODEL: OnceLock<std::result::Result<CachedPlan, String>> = OnceLock::new();
    let res = MODEL.get_or_init(|| {
        let mut cursor = Cursor::new(EMBEDDED_BEAT_THIS_SMALL0);
        tract_onnx::onnx()
            .model_for_read(&mut cursor)
            .map_err(|e| format!("Failed to parse embedded ONNX model: {e}"))?
            .with_input_fact(0, f32::fact([1, CHUNK_FRAMES, N_MELS]).into())
            .map_err(|e| format!("Failed to set ONNX input shape: {e}"))?
            .into_optimized()
            .map_err(|e| format!("Failed to optimize ONNX graph: {e}"))?
            .into_runnable()
            .map_err(|e| format!("Failed to compile runnable ONNX plan: {e}"))
    });
    res.as_ref().map_err(|e| anyhow!("{e}"))
}

/// Run the embedded Beat This! (`small0`) ONNX model via pure-Rust `tract-onnx`
/// in overlapping 30-second (1500-frame) chunks, returning 50 fps beat & downbeat probabilities.
#[cfg(feature = "neural-onnx")]
pub fn infer_beat_this_onnx(
    log_mel: &[f32],
    num_frames: usize,
    mut on_progress: impl FnMut(f32),
) -> Result<NeuralActivations> {
    use tract_onnx::prelude::*;

    if num_frames == 0 || log_mel.len() < num_frames * N_MELS {
        return Err(anyhow!("Empty spectrogram passed to neural tracker"));
    }

    const BORDER: usize = 75; // 1.5s overlap trimmed on interior chunk boundaries
    const STEP: usize = CHUNK_FRAMES - 2 * BORDER; // 1350 frames (27s) advance

    on_progress(0.18);
    let model = get_cached_model()?;
    on_progress(0.30);

    let mut beat_logits = vec![0.0f32; num_frames];
    let mut downbeat_logits = vec![0.0f32; num_frames];
    let mut weights = vec![0.0f32; num_frames];

    let total_chunks = if num_frames <= CHUNK_FRAMES {
        1
    } else {
        ((num_frames - CHUNK_FRAMES) + STEP - 1) / STEP + 1
    };

    let mut chunk_buf = vec![0.0f32; CHUNK_FRAMES * N_MELS];

    for chunk_idx in 0..total_chunks {
        let start = if total_chunks == 1 {
            0
        } else {
            (chunk_idx * STEP).min(num_frames.saturating_sub(CHUNK_FRAMES))
        };
        let valid_len = (num_frames - start).min(CHUNK_FRAMES);

        chunk_buf.fill(0.0);
        chunk_buf[..valid_len * N_MELS]
            .copy_from_slice(&log_mel[start * N_MELS..(start + valid_len) * N_MELS]);

        let input_tensor: Tensor =
            tract_ndarray::Array3::from_shape_vec((1, CHUNK_FRAMES, N_MELS), chunk_buf.clone())
                .map_err(|e| anyhow!("Tensor shape error: {e}"))?
                .into();

        let outputs = model
            .run(tvec!(input_tensor.into()))
            .map_err(|e| anyhow!("ONNX inference error on chunk {chunk_idx}: {e}"))?;

        if outputs.len() < 2 {
            return Err(anyhow!(
                "Expected 2 ONNX outputs (beat, downbeat), got {}",
                outputs.len()
            ));
        }

        let beat_view = outputs[0]
            .to_array_view::<f32>()
            .map_err(|e| anyhow!("Invalid beat output tensor: {e}"))?;
        let downbeat_view = outputs[1]
            .to_array_view::<f32>()
            .map_err(|e| anyhow!("Invalid downbeat output tensor: {e}"))?;

        let beat_slice = beat_view
            .as_slice()
            .ok_or_else(|| anyhow!("Non-contiguous beat output"))?;
        let downbeat_slice = downbeat_view
            .as_slice()
            .ok_or_else(|| anyhow!("Non-contiguous downbeat output"))?;

        let trim_left = if chunk_idx == 0 { 0 } else { BORDER };
        let trim_right = if chunk_idx + 1 == total_chunks {
            valid_len
        } else {
            valid_len.saturating_sub(BORDER)
        };

        for local_t in trim_left..trim_right {
            let global_t = start + local_t;
            if global_t < num_frames && local_t < beat_slice.len() && local_t < downbeat_slice.len()
            {
                beat_logits[global_t] += beat_slice[local_t];
                downbeat_logits[global_t] += downbeat_slice[local_t];
                weights[global_t] += 1.0;
            }
        }

        let frac = (chunk_idx + 1) as f32 / total_chunks as f32;
        on_progress(0.30 + 0.55 * frac);
    }

    let mut beat_probs = vec![0.0f32; num_frames];
    let mut downbeat_probs = vec![0.0f32; num_frames];
    for i in 0..num_frames {
        let w = weights[i].max(1.0);
        beat_probs[i] = sigmoid(beat_logits[i] / w);
        downbeat_probs[i] = sigmoid(downbeat_logits[i] / w);
    }

    Ok(NeuralActivations {
        beat_probs,
        downbeat_probs,
        engine_name: "Hybrid (Beat This! ONNX + DSP)".to_string(),
    })
}

#[cfg(not(feature = "neural-onnx"))]
pub fn infer_beat_this_onnx(
    _log_mel: &[f32],
    _num_frames: usize,
    _on_progress: impl FnMut(f32),
) -> Result<NeuralActivations> {
    Err(anyhow!("Built without neural-onnx feature"))
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
