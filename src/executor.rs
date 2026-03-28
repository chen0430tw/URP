use std::collections::HashMap;
use std::sync::Arc;

use crate::ir::{IRBlock, Opcode};
use crate::packet::PayloadValue;

// ─────────────────────────────────────────────────────────────────────────────
// HardwareExecutor trait
//
// Any hardware backend (CPU, GPU, FPGA, remote…) implements this trait.
// exec() is sync; parallelism across independent blocks is handled by the
// runtime's wave-based scheduler using tokio::task::spawn_blocking.
// ─────────────────────────────────────────────────────────────────────────────

pub trait HardwareExecutor: Send + Sync {
    /// Execute a single block given its resolved input context.
    fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue;

    /// Human-readable name for logging and diagnostics.
    fn name(&self) -> &'static str;
}

// ─────────────────────────────────────────────────────────────────────────────
// CpuExecutor — sequential single-core (original behavior)
// ─────────────────────────────────────────────────────────────────────────────

pub struct CpuExecutor;

impl HardwareExecutor for CpuExecutor {
    fn name(&self) -> &'static str { "cpu" }

    fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
        eval_opcode(block, ctx)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ThreadPoolExecutor — multi-core parallel
//
// Individual block logic is identical to CpuExecutor.
// The parallelism benefit comes from the runtime executing all blocks in the
// same topo wave concurrently via tokio::task::spawn_blocking when a node
// is bound to this executor type.
// ─────────────────────────────────────────────────────────────────────────────

pub struct ThreadPoolExecutor {
    pub parallelism: usize,
}

impl ThreadPoolExecutor {
    pub fn new(parallelism: usize) -> Self {
        Self { parallelism }
    }
}

impl HardwareExecutor for ThreadPoolExecutor {
    fn name(&self) -> &'static str { "thread-pool" }

    fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
        eval_opcode(block, ctx)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ExecutorRegistry — maps node_id → Arc<dyn HardwareExecutor>
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ExecutorRegistry {
    /// Per-node overrides.
    by_node: HashMap<String, Arc<dyn HardwareExecutor>>,
    /// Fallback when no per-node entry exists.
    default: Arc<dyn HardwareExecutor>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            by_node: HashMap::new(),
            default: Arc::new(CpuExecutor),
        }
    }

    /// Register an executor for a specific node ID.
    pub fn register(&mut self, node_id: impl Into<String>, executor: Arc<dyn HardwareExecutor>) {
        self.by_node.insert(node_id.into(), executor);
    }

    /// Set the fallback executor used when no per-node registration exists.
    pub fn set_default(&mut self, executor: Arc<dyn HardwareExecutor>) {
        self.default = executor;
    }

    /// Get the executor for a node (falls back to default if not registered).
    pub fn get(&self, node_id: &str) -> Arc<dyn HardwareExecutor> {
        self.by_node
            .get(node_id)
            .cloned()
            .unwrap_or_else(|| self.default.clone())
    }

    pub fn is_parallel(&self, node_id: &str) -> bool {
        self.get(node_id).name() == "thread-pool"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BlockExecutor — backward-compatible shim
// ─────────────────────────────────────────────────────────────────────────────

pub struct BlockExecutor;

impl BlockExecutor {
    pub fn exec(block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
        CpuExecutor.exec(block, ctx)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared opcode evaluation (used by all executor types)
// ─────────────────────────────────────────────────────────────────────────────

pub fn eval_opcode(block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
    match &block.opcode {
        Opcode::UConstI64(v) => PayloadValue::I64(*v),
        Opcode::UConstStr(s) => PayloadValue::Str(s.clone()),
        Opcode::UAdd => {
            let a = ctx.get(&block.inputs[0]).expect("missing input a");
            let b = ctx.get(&block.inputs[1]).expect("missing input b");
            match (a, b) {
                (PayloadValue::I64(x), PayloadValue::I64(y)) => PayloadValue::I64(x + y),
                _ => panic!("UAdd expects i64 inputs"),
            }
        }
        Opcode::UConcat => {
            let a = ctx.get(&block.inputs[0]).expect("missing input left");
            let b = ctx.get(&block.inputs[1]).expect("missing input right");
            let left = match a {
                PayloadValue::I64(v) => v.to_string(),
                PayloadValue::Str(s) => s.clone(),
            };
            let right = match b {
                PayloadValue::I64(v) => v.to_string(),
                PayloadValue::Str(s) => s.clone(),
            };
            PayloadValue::Str(format!("{left}{right}"))
        }
    }
}
