//! ONNX Runtime-backed executor for `Opcode::OnnxInfer` blocks.
//!
//! Feature-gated: only available with `--features onnx`.
//! Without the feature the struct exists but `load()` always returns `Err` and
//! `exec()` panics if it encounters an `OnnxInfer` block.
//!
//! # Usage
//! ```no_run
//! use std::sync::Arc;
//! use urx_runtime_v08::onnx_executor::OnnxExecutor;
//!
//! let exec = OnnxExecutor::load("model.onnx").unwrap();
//! // runtime.executors.register("gpu_node", Arc::new(exec));
//! ```

use std::collections::HashMap;

use crate::executor::{eval_opcode, HardwareExecutor};
use crate::ir::{IRBlock, Opcode};
use crate::packet::PayloadValue;

// ─────────────────────────────────────────────────────────────────────────────

/// ONNX executor: pre-loads a model at construction time, runs it for every
/// `OnnxInfer` block, and delegates all other opcodes to `eval_opcode`.
pub struct OnnxExecutor {
    /// Path to the `.onnx` file (kept for diagnostics).
    pub model_path: String,
    /// Live session — only present when the `onnx` feature is enabled.
    #[cfg(feature = "onnx")]
    session: ort::Session,
}

impl OnnxExecutor {
    /// Load an ONNX model and return a ready-to-use executor.
    ///
    /// Requires the `onnx` feature **and** a valid ONNX Runtime shared library
    /// (set `ORT_DYLIB_PATH` or place `onnxruntime.dll` / `libonnxruntime.so`
    /// on `PATH` / `LD_LIBRARY_PATH`).
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "onnx")]
        {
            let session = ort::Session::builder()?.commit_from_file(path)?;
            Ok(Self {
                model_path: path.to_string(),
                session,
            })
        }
        #[cfg(not(feature = "onnx"))]
        {
            let _ = path;
            Err("URX was built without the `onnx` feature. \
                 Rebuild with `cargo build --features onnx` and ensure \
                 onnxruntime is installed."
                .into())
        }
    }
}

impl HardwareExecutor for OnnxExecutor {
    fn name(&self) -> &'static str {
        "onnx"
    }

    fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
        // Fast path: let the onnx-feature code handle OnnxInfer directly
        #[cfg(feature = "onnx")]
        if let Opcode::OnnxInfer(model_path) = &block.opcode {
            return run_onnx_inference(&self.session, model_path, ctx);
        }

        // All other opcodes (or OnnxInfer without feature) fall through
        match &block.opcode {
            Opcode::OnnxInfer(path) => {
                let _ = path;
                panic!(
                    "OnnxInfer requires the `onnx` feature — \
                     rebuild with `cargo build --features onnx`"
                );
            }
            _ => eval_opcode(block, ctx),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Real ONNX inference (only compiled with the `onnx` feature)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "onnx")]
fn run_onnx_inference(
    session: &ort::Session,
    _model_path: &str,
    ctx: &HashMap<String, PayloadValue>,
) -> PayloadValue {
    use ndarray::{Array, IxDyn};

    // ── 1. Build f32 ArrayD for every model input ────────────────────────
    let named_arrays: Vec<(String, ndarray::ArrayD<f32>)> = session
        .inputs
        .iter()
        .map(|inp| {
            let name = inp.name.as_str();
            let arr = match ctx.get(name) {
                Some(PayloadValue::Tensor(data, shape)) => {
                    Array::from_shape_vec(IxDyn(shape), data.clone()).unwrap_or_else(|e| {
                        panic!("OnnxInfer: shape mismatch for input '{name}': {e}")
                    })
                }
                Some(PayloadValue::F64(v)) => {
                    // Scalar promoted to a length-1 f32 tensor
                    Array::from_shape_vec(IxDyn(&[1usize]), vec![*v as f32]).unwrap()
                }
                Some(other) => {
                    panic!("OnnxInfer: input '{name}' must be Tensor or F64, got {other:?}")
                }
                None => panic!("OnnxInfer: missing input '{name}' in execution context"),
            };
            (inp.name.clone(), arr)
        })
        .collect();

    // ── 2. Convert to ort::Value and run ────────────────────────────────
    let ort_inputs: Vec<(String, ort::Value)> = named_arrays
        .into_iter()
        .map(|(name, arr)| {
            let val = ort::Value::from_array(arr).unwrap_or_else(|e| {
                panic!("OnnxInfer: failed to create ORT value for '{name}': {e}")
            });
            (name, val)
        })
        .collect();

    // session.run accepts Vec<(String, ort::Value)> via Into<SessionInputs>
    let outputs = session
        .run(ort_inputs)
        .unwrap_or_else(|e| panic!("OnnxInfer: session.run() failed: {e}"));

    // ── 3. Extract first output as f32 tensor ────────────────────────────
    let (_, first_val) = outputs
        .iter()
        .next()
        .unwrap_or_else(|| panic!("OnnxInfer: model returned no outputs"));

    let extracted = first_val
        .try_extract_tensor::<f32>()
        .unwrap_or_else(|e| panic!("OnnxInfer: failed to extract f32 tensor: {e}"));

    let view = extracted.view();
    let shape: Vec<usize> = view.shape().to_vec();
    let data: Vec<f32> = view.iter().cloned().collect();

    PayloadValue::Tensor(data, shape)
}
