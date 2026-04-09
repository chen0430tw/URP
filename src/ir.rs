use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MergeMode {
    List = 1,
    Sum = 2,
    Concat = 3,
    ReduceMax = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Opcode {
    // ── Constants ────────────────────────────────────────────────
    UConstI64(i64),
    UConstStr(String),

    // ── Batch 1: Arithmetic + Comparison ─────────────────────────
    UAdd,   // i64 + i64
    USub,   // i64 - i64
    UMul,   // i64 * i64  (low 64 bits)
    UDiv,   // i64 / i64  (quotient)
    URem,   // i64 % i64  (remainder)
    UCmpEq, // i64 == i64  → 1 or 0
    UCmpLt, // i64 <  i64  → 1 or 0
    UCmpLe, // i64 <= i64  → 1 or 0

    // ── Batch 2: Logic + Shift ────────────────────────────────────
    UAnd,   // i64 & i64
    UOr,    // i64 | i64
    UXor,   // i64 ^ i64
    UNot,   // !i64  (bitwise NOT, 1 input)
    UShl,   // i64 << i64
    UShr,   // i64 >> i64  (logical)
    UShra,  // i64 >> i64  (arithmetic)

    // ── Batch 3: Type Conversion + String ────────────────────────
    UConcat,     // any ++ any  → Str
    UI64ToStr,   // i64         → Str  (decimal)
    UStrToI64,   // Str         → i64  (parse, panic on error)
    UStrLen,     // Str         → i64  (character count)
    UStrSlice,   // Str, i64, i64 → Str  ([start, end))
    UStrSplit,   // Str, Str    → List<Str>  (split by delimiter)

    // ── Batch 4: Conditional Select + Aggregation ─────────────────
    USelect,   // i64(cond), any, any → any   (cond≠0 → input[1], else input[2])
    UMin,      // i64, i64 → i64
    UMax,      // i64, i64 → i64
    UAbs,      // i64      → i64
    UAssert,   // i64(cond)  → i64   (pass-through; panics when cond == 0)

    // ── F64: Constants ────────────────────────────────────────────
    FConst(f64),   // → f64

    // ── F64: Binary Arithmetic ────────────────────────────────────
    FAdd,   // f64 + f64
    FSub,   // f64 - f64
    FMul,   // f64 * f64
    FDiv,   // f64 / f64
    FPow,   // f64 ^ f64

    // ── F64: Unary Arithmetic ─────────────────────────────────────
    FSqrt,   // sqrt(f64)
    FAbs,    // |f64|
    FNeg,    // -f64
    FFloor,  // floor(f64)
    FCeil,   // ceil(f64)
    FRound,  // round(f64)

    // ── F64: Comparison (returns 1 or 0 as i64) ──────────────────
    FCmpEq,  // f64 == f64 → i64
    FCmpLt,  // f64 <  f64 → i64
    FCmpLe,  // f64 <= f64 → i64

    // ── F64: Type Conversion ──────────────────────────────────────
    F64ToI64,  // f64 → i64  (truncate)
    I64ToF64,  // i64 → f64

    // ── ONNX Model Inference ──────────────────────────────────────
    /// Run an ONNX model.  The String is the path to the .onnx file.
    /// Inputs come from ctx keyed by ONNX input name (PayloadValue::Tensor).
    /// Returns PayloadValue::Tensor (first model output).
    OnnxInfer(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IRBlock {
    pub block_id: String,
    pub opcode: Opcode,
    pub inputs: Vec<String>,
    pub output: String,
    pub required_tag: String,
    pub merge_mode: MergeMode,
    pub resource_shape: String,
    pub preferred_zone: String,
    pub inertia_key: Option<String>,
    pub estimated_duration: u32,
}

impl IRBlock {
    pub fn new(id: &str, opcode: Opcode) -> Self {
        Self {
            block_id: id.to_string(),
            opcode,
            inputs: Vec::new(),
            output: String::new(),
            required_tag: String::new(),
            merge_mode: MergeMode::List,
            resource_shape: String::new(),
            preferred_zone: String::new(),
            inertia_key: None,
            estimated_duration: 1,
        }
    }

    pub fn set_tag(&mut self, tag: &str) {
        self.required_tag = tag.to_string();
    }

    pub fn set_merge_mode(&mut self, mode: MergeMode) {
        self.merge_mode = mode;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IREdge {
    pub src_block: String,
    pub dst_block: String,
    pub output_key: String,
    pub input_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IRGraph {
    pub graph_id: String,
    pub blocks: Vec<IRBlock>,
    pub edges: Vec<IREdge>,
}

impl IRGraph {
    pub fn new() -> Self {
        Self {
            graph_id: "default".to_string(),
            blocks: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn with_id(id: String) -> Self {
        Self {
            graph_id: id,
            blocks: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn get_block(&self, id: &str) -> Option<&IRBlock> {
        self.blocks.iter().find(|b| b.block_id == id)
    }

    /// Deserialize an `IRGraph` from a JSON string.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Serialize this `IRGraph` to a pretty-printed JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Load an `IRGraph` from a JSON file.
    pub fn load_json(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let s = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&s)?)
    }

    /// Save this `IRGraph` to a JSON file.
    pub fn save_json(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }

    /// Build an `IRGraph` from an ONNX model file.
    ///
    /// # Graph structure
    /// ```text
    /// input_0 ──┐
    /// input_1 ──┼──▶ onnx_infer(path) ──▶ (leaf, returns Tensor)
    /// ...      ─┘
    /// ```
    ///
    /// The `onnx_infer` block holds `Opcode::OnnxInfer(path)`.  At execution
    /// time the `OnnxExecutor` (feature="onnx") picks it up and runs the model.
    ///
    /// Requires the `onnx` feature and a valid ONNX Runtime shared library at
    /// runtime.  Without the feature this always returns `Err`.
    pub fn from_onnx(model_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "onnx")]
        {
            use ort::Session;

            // Open the session just long enough to introspect input/output names.
            let session = Session::builder()?.commit_from_file(model_path)?;

            let mut graph = IRGraph::with_id(
                std::path::Path::new(model_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("onnx_graph")
                    .to_string(),
            );

            // Create one placeholder block per model input.
            let mut input_ids: Vec<String> = Vec::new();
            for (i, inp) in session.inputs.iter().enumerate() {
                let bid = format!("input_{}", i);
                let mut b = IRBlock::new(&bid, Opcode::UConstI64(0));
                b.output = inp.name.clone();
                graph.blocks.push(b);
                input_ids.push(bid);
            }

            // Create the single inference block.
            let mut infer = IRBlock::new("onnx_infer", Opcode::OnnxInfer(model_path.to_string()));
            infer.inputs = input_ids.clone();
            infer.output = "onnx_output".to_string();
            graph.blocks.push(infer);

            // Wire edges: each input placeholder → inference block.
            for (i, inp) in session.inputs.iter().enumerate() {
                graph.edges.push(IREdge {
                    src_block:   input_ids[i].clone(),
                    dst_block:   "onnx_infer".to_string(),
                    output_key:  "out".to_string(),
                    input_key:   inp.name.clone(),
                });
            }

            Ok(graph)
        }
        #[cfg(not(feature = "onnx"))]
        {
            let _ = model_path;
            Err("URX was built without the `onnx` feature. Rebuild with `--features onnx` and ensure onnxruntime is installed.".into())
        }
    }
}
