#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MergeMode {
    List = 1,
    Sum = 2,
    Concat = 3,
    ReduceMax = 4,
}

#[derive(Debug, Clone)]
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

    // ── String ───────────────────────────────────────────────────
    UConcat, // any ++ any → Str
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct IREdge {
    pub src_block: String,
    pub dst_block: String,
    pub output_key: String,
    pub input_key: String,
}

#[derive(Debug, Clone)]
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
}
