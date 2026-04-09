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

pub trait HardwareExecutor: Send + Sync + 'static {
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
    // Helper: resolve a named input as i64
    let i64_in = |name: &str| -> i64 {
        match ctx.get(name).unwrap_or_else(|| panic!("missing input '{name}'")) {
            PayloadValue::I64(v) => *v,
            other => panic!("input '{name}' expected i64, got {other:?}"),
        }
    };

    // Helper: resolve a named input as f64
    let f64_in = |name: &str| -> f64 {
        match ctx.get(name).unwrap_or_else(|| panic!("missing input '{name}'")) {
            PayloadValue::F64(v) => *v,
            PayloadValue::I64(v) => *v as f64,
            other => panic!("input '{name}' expected f64, got {other:?}"),
        }
    };

    match &block.opcode {
        // ── Constants ────────────────────────────────────────────────
        Opcode::UConstI64(v) => PayloadValue::I64(*v),
        Opcode::UConstStr(s) => PayloadValue::Str(s.clone()),

        // ── Batch 1: Arithmetic ───────────────────────────────────────
        Opcode::UAdd => {
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_add(i64_in(&block.inputs[1])))
        }
        Opcode::USub => {
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_sub(i64_in(&block.inputs[1])))
        }
        Opcode::UMul => {
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_mul(i64_in(&block.inputs[1])))
        }
        Opcode::UDiv => {
            let b = i64_in(&block.inputs[1]);
            assert!(b != 0, "UDiv: division by zero");
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_div(b))
        }
        Opcode::URem => {
            let b = i64_in(&block.inputs[1]);
            assert!(b != 0, "URem: division by zero");
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_rem(b))
        }

        // ── Batch 1: Comparison (returns 1 or 0) ─────────────────────
        Opcode::UCmpEq => {
            PayloadValue::I64((i64_in(&block.inputs[0]) == i64_in(&block.inputs[1])) as i64)
        }
        Opcode::UCmpLt => {
            PayloadValue::I64((i64_in(&block.inputs[0]) < i64_in(&block.inputs[1])) as i64)
        }
        Opcode::UCmpLe => {
            PayloadValue::I64((i64_in(&block.inputs[0]) <= i64_in(&block.inputs[1])) as i64)
        }

        // ── Batch 2: Logic ────────────────────────────────────────────
        Opcode::UAnd => {
            PayloadValue::I64(i64_in(&block.inputs[0]) & i64_in(&block.inputs[1]))
        }
        Opcode::UOr => {
            PayloadValue::I64(i64_in(&block.inputs[0]) | i64_in(&block.inputs[1]))
        }
        Opcode::UXor => {
            PayloadValue::I64(i64_in(&block.inputs[0]) ^ i64_in(&block.inputs[1]))
        }
        Opcode::UNot => {
            PayloadValue::I64(!i64_in(&block.inputs[0]))
        }

        // ── Batch 2: Shift ────────────────────────────────────────────
        Opcode::UShl => {
            let amt = (i64_in(&block.inputs[1]) & 63) as u32;
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_shl(amt))
        }
        Opcode::UShr => {
            let amt = (i64_in(&block.inputs[1]) & 63) as u32;
            PayloadValue::I64(((i64_in(&block.inputs[0]) as u64).wrapping_shr(amt)) as i64)
        }
        Opcode::UShra => {
            let amt = (i64_in(&block.inputs[1]) & 63) as u32;
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_shr(amt))
        }

        // ── Batch 3: String / Type Conversion ────────────────────────
        Opcode::UConcat => {
            let to_s = |v: &PayloadValue| match v {
                PayloadValue::I64(n) => n.to_string(),
                PayloadValue::F64(n) => n.to_string(),
                PayloadValue::Str(s) => s.clone(),
                PayloadValue::List(_) => panic!("UConcat: List input not supported"),
                PayloadValue::Tensor(_, _) => panic!("UConcat: Tensor input not supported"),
            };
            let a = ctx.get(&block.inputs[0]).expect("missing input left");
            let b = ctx.get(&block.inputs[1]).expect("missing input right");
            PayloadValue::Str(format!("{}{}", to_s(a), to_s(b)))
        }
        Opcode::UI64ToStr => {
            PayloadValue::Str(i64_in(&block.inputs[0]).to_string())
        }
        Opcode::UStrToI64 => {
            let s = match ctx.get(&block.inputs[0]).expect("missing input") {
                PayloadValue::Str(s) => s.clone(),
                other => panic!("UStrToI64 expects Str, got {other:?}"),
            };
            PayloadValue::I64(s.trim().parse::<i64>().unwrap_or_else(|e| {
                panic!("UStrToI64: cannot parse {:?}: {e}", s)
            }))
        }
        Opcode::UStrLen => {
            let s = match ctx.get(&block.inputs[0]).expect("missing input") {
                PayloadValue::Str(s) => s.clone(),
                other => panic!("UStrLen expects Str, got {other:?}"),
            };
            PayloadValue::I64(s.chars().count() as i64)
        }
        Opcode::UStrSlice => {
            let s = match ctx.get(&block.inputs[0]).expect("missing input str") {
                PayloadValue::Str(s) => s.clone(),
                other => panic!("UStrSlice input[0] expects Str, got {other:?}"),
            };
            let start = i64_in(&block.inputs[1]).max(0) as usize;
            let end   = i64_in(&block.inputs[2]).max(0) as usize;
            let chars: Vec<char> = s.chars().collect();
            let end = end.min(chars.len());
            let start = start.min(end);
            PayloadValue::Str(chars[start..end].iter().collect())
        }
        Opcode::UStrSplit => {
            let s = match ctx.get(&block.inputs[0]).expect("missing input str") {
                PayloadValue::Str(s) => s.clone(),
                other => panic!("UStrSplit input[0] expects Str, got {other:?}"),
            };
            let delim = match ctx.get(&block.inputs[1]).expect("missing input delim") {
                PayloadValue::Str(s) => s.clone(),
                other => panic!("UStrSplit input[1] expects Str, got {other:?}"),
            };
            let parts: Vec<PayloadValue> = s
                .split(delim.as_str())
                .map(|p| PayloadValue::Str(p.to_string()))
                .collect();
            PayloadValue::List(parts)
        }

        // ── Batch 4: Conditional Select + Aggregation ────────────────
        Opcode::USelect => {
            let cond = i64_in(&block.inputs[0]);
            let key  = if cond != 0 { &block.inputs[1] } else { &block.inputs[2] };
            ctx.get(key).unwrap_or_else(|| panic!("USelect: missing input '{key}'")).clone()
        }
        Opcode::UMin => {
            PayloadValue::I64(i64_in(&block.inputs[0]).min(i64_in(&block.inputs[1])))
        }
        Opcode::UMax => {
            PayloadValue::I64(i64_in(&block.inputs[0]).max(i64_in(&block.inputs[1])))
        }
        Opcode::UAbs => {
            PayloadValue::I64(i64_in(&block.inputs[0]).wrapping_abs())
        }
        Opcode::UAssert => {
            let cond = i64_in(&block.inputs[0]);
            assert!(cond != 0, "UAssert: condition is 0 (false)");
            PayloadValue::I64(cond)
        }

        // ── F64: Constants ────────────────────────────────────────────
        Opcode::FConst(v) => PayloadValue::F64(*v),

        // ── F64: Binary Arithmetic ────────────────────────────────────
        Opcode::FAdd => PayloadValue::F64(f64_in(&block.inputs[0]) + f64_in(&block.inputs[1])),
        Opcode::FSub => PayloadValue::F64(f64_in(&block.inputs[0]) - f64_in(&block.inputs[1])),
        Opcode::FMul => PayloadValue::F64(f64_in(&block.inputs[0]) * f64_in(&block.inputs[1])),
        Opcode::FDiv => {
            let b = f64_in(&block.inputs[1]);
            assert!(b != 0.0, "FDiv: division by zero");
            PayloadValue::F64(f64_in(&block.inputs[0]) / b)
        }
        Opcode::FPow => PayloadValue::F64(f64_in(&block.inputs[0]).powf(f64_in(&block.inputs[1]))),

        // ── F64: Unary Arithmetic ─────────────────────────────────────
        Opcode::FSqrt  => PayloadValue::F64(f64_in(&block.inputs[0]).sqrt()),
        Opcode::FAbs   => PayloadValue::F64(f64_in(&block.inputs[0]).abs()),
        Opcode::FNeg   => PayloadValue::F64(-f64_in(&block.inputs[0])),
        Opcode::FFloor => PayloadValue::F64(f64_in(&block.inputs[0]).floor()),
        Opcode::FCeil  => PayloadValue::F64(f64_in(&block.inputs[0]).ceil()),
        Opcode::FRound => PayloadValue::F64(f64_in(&block.inputs[0]).round()),

        // ── F64: Comparison ───────────────────────────────────────────
        Opcode::FCmpEq => PayloadValue::I64((f64_in(&block.inputs[0]) == f64_in(&block.inputs[1])) as i64),
        Opcode::FCmpLt => PayloadValue::I64((f64_in(&block.inputs[0]) <  f64_in(&block.inputs[1])) as i64),
        Opcode::FCmpLe => PayloadValue::I64((f64_in(&block.inputs[0]) <= f64_in(&block.inputs[1])) as i64),

        // ── F64: Type Conversion ──────────────────────────────────────
        Opcode::F64ToI64 => PayloadValue::I64(f64_in(&block.inputs[0]) as i64),
        Opcode::I64ToF64 => PayloadValue::F64(i64_in(&block.inputs[0]) as f64),

        // ── ONNX Model Inference ──────────────────────────────────────
        Opcode::OnnxInfer(_path) => {
            panic!(
                "OnnxInfer blocks require OnnxExecutor. \
                 Register it: runtime.executors.register(node_id, \
                 Arc::new(OnnxExecutor::load(path).unwrap()))"
            );
        }
    }
}
