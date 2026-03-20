#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MergeMode {
    List = 1,
    Sum = 2,
    Concat = 3,
    ReduceMax = 4,
}

#[derive(Debug, Clone)]
pub enum Opcode {
    UConstI64(i64),
    UConstStr(String),
    UAdd,
    UConcat,
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
    pub fn get_block(&self, id: &str) -> Option<&IRBlock> {
        self.blocks.iter().find(|b| b.block_id == id)
    }
}
