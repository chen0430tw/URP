//! Opcode correctness tests — Batch 1 (Arithmetic + Comparison) and Batch 2 (Logic + Shift)
//!
//! Tests run on the CPU executor (eval_opcode).
//! A separate #[cfg(feature = "gpu")] section repeats key cases on WgpuExecutor.

use std::collections::HashMap;
use urx_runtime_v08::{eval_opcode, IRBlock, Opcode};
use urx_runtime_v08::packet::PayloadValue;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn run(opcode: Opcode, a: i64, b: i64) -> i64 {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["a".to_string(), "b".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("a".to_string(), PayloadValue::I64(a));
    ctx.insert("b".to_string(), PayloadValue::I64(b));
    match eval_opcode(&block, &ctx) {
        PayloadValue::I64(v) => v,
        other => panic!("expected I64, got {other:?}"),
    }
}

fn run1(opcode: Opcode, a: i64) -> i64 {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["a".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("a".to_string(), PayloadValue::I64(a));
    match eval_opcode(&block, &ctx) {
        PayloadValue::I64(v) => v,
        other => panic!("expected I64, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 1 — Arithmetic
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_usub() {
    assert_eq!(run(Opcode::USub, 10, 3), 7);
    assert_eq!(run(Opcode::USub, 0, 5), -5);
    assert_eq!(run(Opcode::USub, -3, -3), 0);
}

#[test]
fn test_umul() {
    assert_eq!(run(Opcode::UMul, 6, 7), 42);
    assert_eq!(run(Opcode::UMul, -4, 3), -12);
    assert_eq!(run(Opcode::UMul, 0, 999), 0);
}

#[test]
fn test_udiv() {
    assert_eq!(run(Opcode::UDiv, 10, 3), 3);   // truncates toward zero
    assert_eq!(run(Opcode::UDiv, -10, 3), -3);
    assert_eq!(run(Opcode::UDiv, 100, 10), 10);
}

#[test]
fn test_urem() {
    assert_eq!(run(Opcode::URem, 10, 3), 1);
    assert_eq!(run(Opcode::URem, -10, 3), -1);
    assert_eq!(run(Opcode::URem, 100, 10), 0);
}

#[test]
#[should_panic(expected = "division by zero")]
fn test_udiv_by_zero() {
    run(Opcode::UDiv, 5, 0);
}

#[test]
#[should_panic(expected = "division by zero")]
fn test_urem_by_zero() {
    run(Opcode::URem, 5, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 1 — Comparison  (returns 1 or 0)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ucmpeq() {
    assert_eq!(run(Opcode::UCmpEq, 5, 5), 1);
    assert_eq!(run(Opcode::UCmpEq, 5, 6), 0);
    assert_eq!(run(Opcode::UCmpEq, -1, -1), 1);
}

#[test]
fn test_ucmplt() {
    assert_eq!(run(Opcode::UCmpLt, 3, 5), 1);
    assert_eq!(run(Opcode::UCmpLt, 5, 5), 0);
    assert_eq!(run(Opcode::UCmpLt, 6, 5), 0);
    assert_eq!(run(Opcode::UCmpLt, -1, 0), 1);
}

#[test]
fn test_ucmple() {
    assert_eq!(run(Opcode::UCmpLe, 3, 5), 1);
    assert_eq!(run(Opcode::UCmpLe, 5, 5), 1);
    assert_eq!(run(Opcode::UCmpLe, 6, 5), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 2 — Logic
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_uand() {
    assert_eq!(run(Opcode::UAnd, 0b1010, 0b1100), 0b1000);
    assert_eq!(run(Opcode::UAnd, 0xFF, 0x0F), 0x0F);
    assert_eq!(run(Opcode::UAnd, 0, 0xFFFF), 0);
}

#[test]
fn test_uor() {
    assert_eq!(run(Opcode::UOr, 0b1010, 0b0101), 0b1111);
    assert_eq!(run(Opcode::UOr, 0, 42), 42);
}

#[test]
fn test_uxor() {
    assert_eq!(run(Opcode::UXor, 0b1010, 0b1100), 0b0110);
    assert_eq!(run(Opcode::UXor, 0xFF, 0xFF), 0);
}

#[test]
fn test_unot() {
    assert_eq!(run1(Opcode::UNot, 0), -1);      // !0 = all 1s = -1 in i64
    assert_eq!(run1(Opcode::UNot, -1), 0);
    assert_eq!(run1(Opcode::UNot, 1), -2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 2 — Shift
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ushl() {
    assert_eq!(run(Opcode::UShl, 1, 3), 8);
    assert_eq!(run(Opcode::UShl, 0xFF, 4), 0xFF0);
    assert_eq!(run(Opcode::UShl, 1, 63), i64::MIN); // 1 << 63 = sign bit
}

#[test]
fn test_ushr() {
    // logical: zero-fills from the left
    assert_eq!(run(Opcode::UShr, 8, 3), 1);
    assert_eq!(run(Opcode::UShr, -1_i64, 1), i64::MAX); // 0x7FFF…FFF
}

#[test]
fn test_ushra() {
    // arithmetic: sign-fills from the left
    assert_eq!(run(Opcode::UShra, 8, 3), 1);
    assert_eq!(run(Opcode::UShra, -8, 3), -1); // sign-extended
    assert_eq!(run(Opcode::UShra, -1_i64, 1), -1);
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU — repeat key cases on WgpuExecutor
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "gpu")]
mod gpu_opcodes {
    use super::*;
    use urx_runtime_v08::{HardwareExecutor, WgpuExecutor};

    async fn try_init() -> Option<WgpuExecutor> {
        match WgpuExecutor::new().await {
            Ok(ex) => { println!("[gpu] {}", ex.adapter_info); Some(ex) }
            Err(e) => { println!("[gpu] skip — {e}"); None }
        }
    }

    fn grun(ex: &WgpuExecutor, opcode: Opcode, a: i64, b: i64) -> i64 {
        let mut block = IRBlock::new("op", opcode);
        block.inputs = vec!["a".to_string(), "b".to_string()];
        let mut ctx = HashMap::new();
        ctx.insert("a".to_string(), PayloadValue::I64(a));
        ctx.insert("b".to_string(), PayloadValue::I64(b));
        match ex.exec(&block, &ctx) {
            PayloadValue::I64(v) => v,
            other => panic!("expected I64, got {other:?}"),
        }
    }

    fn grun1(ex: &WgpuExecutor, opcode: Opcode, a: i64) -> i64 {
        let mut block = IRBlock::new("op", opcode);
        block.inputs = vec!["a".to_string()];
        let mut ctx = HashMap::new();
        ctx.insert("a".to_string(), PayloadValue::I64(a));
        match ex.exec(&block, &ctx) {
            PayloadValue::I64(v) => v,
            other => panic!("expected I64, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_gpu_batch1_arithmetic() {
        let Some(ex) = try_init().await else { return };
        assert_eq!(grun(&ex, Opcode::UAdd, 10, 5),  15);
        assert_eq!(grun(&ex, Opcode::USub, 10, 3),   7);
        assert_eq!(grun(&ex, Opcode::UMul,  6, 7),  42);
        assert_eq!(grun(&ex, Opcode::UDiv, 10, 3),   3);
        assert_eq!(grun(&ex, Opcode::URem, 10, 3),   1);
        println!("[gpu-batch1] arithmetic ✓");
    }

    #[tokio::test]
    async fn test_gpu_batch1_comparison() {
        let Some(ex) = try_init().await else { return };
        assert_eq!(grun(&ex, Opcode::UCmpEq, 5, 5), 1);
        assert_eq!(grun(&ex, Opcode::UCmpEq, 5, 6), 0);
        assert_eq!(grun(&ex, Opcode::UCmpLt, 3, 5), 1);
        assert_eq!(grun(&ex, Opcode::UCmpLt, 5, 5), 0);
        assert_eq!(grun(&ex, Opcode::UCmpLe, 5, 5), 1);
        assert_eq!(grun(&ex, Opcode::UCmpLe, 6, 5), 0);
        println!("[gpu-batch1] comparison ✓");
    }

    #[tokio::test]
    async fn test_gpu_batch2_logic() {
        let Some(ex) = try_init().await else { return };
        assert_eq!(grun(&ex, Opcode::UAnd, 0b1010, 0b1100), 0b1000);
        assert_eq!(grun(&ex, Opcode::UOr,  0b1010, 0b0101), 0b1111);
        assert_eq!(grun(&ex, Opcode::UXor, 0b1010, 0b1100), 0b0110);
        assert_eq!(grun1(&ex, Opcode::UNot, 0), -1);
        assert_eq!(grun1(&ex, Opcode::UNot, -1), 0);
        println!("[gpu-batch2] logic ✓");
    }

    #[tokio::test]
    async fn test_gpu_batch2_shift() {
        let Some(ex) = try_init().await else { return };
        assert_eq!(grun(&ex, Opcode::UShl,  1, 3),  8);
        assert_eq!(grun(&ex, Opcode::UShr,  8, 3),  1);
        assert_eq!(grun(&ex, Opcode::UShra, -8, 3), -1);
        println!("[gpu-batch2] shift ✓");
    }
}
