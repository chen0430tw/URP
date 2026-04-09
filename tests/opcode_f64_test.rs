//! F64 opcode correctness tests
//!
//! Covers all 17 F64 opcodes on the CPU executor (eval_opcode):
//!   FConst, FAdd, FSub, FMul, FDiv, FPow,
//!   FSqrt, FAbs, FNeg, FFloor, FCeil, FRound,
//!   FCmpEq, FCmpLt, FCmpLe,
//!   F64ToI64, I64ToF64

use std::collections::HashMap;
use urx_runtime_v08::{eval_opcode, IRBlock, Opcode};
use urx_runtime_v08::packet::PayloadValue;

const EPS: f64 = 1e-9;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn fconst(v: f64) -> f64 {
    let block = IRBlock::new("op", Opcode::FConst(v));
    let ctx: HashMap<String, PayloadValue> = HashMap::new();
    match eval_opcode(&block, &ctx) {
        PayloadValue::F64(r) => r,
        other => panic!("expected F64, got {other:?}"),
    }
}

fn fbin(opcode: Opcode, a: f64, b: f64) -> f64 {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["a".to_string(), "b".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("a".to_string(), PayloadValue::F64(a));
    ctx.insert("b".to_string(), PayloadValue::F64(b));
    match eval_opcode(&block, &ctx) {
        PayloadValue::F64(r) => r,
        other => panic!("expected F64, got {other:?}"),
    }
}

fn fun1(opcode: Opcode, a: f64) -> f64 {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["a".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("a".to_string(), PayloadValue::F64(a));
    match eval_opcode(&block, &ctx) {
        PayloadValue::F64(r) => r,
        other => panic!("expected F64, got {other:?}"),
    }
}

fn fcmp(opcode: Opcode, a: f64, b: f64) -> i64 {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["a".to_string(), "b".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("a".to_string(), PayloadValue::F64(a));
    ctx.insert("b".to_string(), PayloadValue::F64(b));
    match eval_opcode(&block, &ctx) {
        PayloadValue::I64(r) => r,
        other => panic!("expected I64, got {other:?}"),
    }
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

// ─────────────────────────────────────────────────────────────────────────────
// FConst
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_fconst() {
    assert!(approx(fconst(3.14), 3.14));
    assert!(approx(fconst(0.0), 0.0));
    assert!(approx(fconst(-1.5), -1.5));
    assert!(fconst(f64::INFINITY).is_infinite());
}

// ─────────────────────────────────────────────────────────────────────────────
// Binary Arithmetic
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_fadd() {
    assert!(approx(fbin(Opcode::FAdd, 1.0, 2.0), 3.0));
    assert!(approx(fbin(Opcode::FAdd, -1.5, 1.5), 0.0));
    assert!(approx(fbin(Opcode::FAdd, 0.1, 0.2), 0.1 + 0.2));
}

#[test]
fn test_fsub() {
    assert!(approx(fbin(Opcode::FSub, 5.0, 3.0), 2.0));
    assert!(approx(fbin(Opcode::FSub, 0.0, 1.0), -1.0));
    assert!(approx(fbin(Opcode::FSub, -2.0, -2.0), 0.0));
}

#[test]
fn test_fmul() {
    assert!(approx(fbin(Opcode::FMul, 3.0, 4.0), 12.0));
    assert!(approx(fbin(Opcode::FMul, -2.0, 5.0), -10.0));
    assert!(approx(fbin(Opcode::FMul, 0.0, 999.0), 0.0));
}

#[test]
fn test_fdiv() {
    assert!(approx(fbin(Opcode::FDiv, 10.0, 4.0), 2.5));
    assert!(approx(fbin(Opcode::FDiv, -9.0, 3.0), -3.0));
    assert!(approx(fbin(Opcode::FDiv, 1.0, 3.0), 1.0 / 3.0));
}

#[test]
fn test_fpow() {
    assert!(approx(fbin(Opcode::FPow, 2.0, 10.0), 1024.0));
    assert!(approx(fbin(Opcode::FPow, 9.0, 0.5), 3.0));
    assert!(approx(fbin(Opcode::FPow, 1.0, 999.0), 1.0));
}

// ─────────────────────────────────────────────────────────────────────────────
// Unary Arithmetic
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_fsqrt() {
    assert!(approx(fun1(Opcode::FSqrt, 4.0), 2.0));
    assert!(approx(fun1(Opcode::FSqrt, 9.0), 3.0));
    assert!(approx(fun1(Opcode::FSqrt, 2.0), 2.0_f64.sqrt()));
}

#[test]
fn test_fabs() {
    assert!(approx(fun1(Opcode::FAbs, -3.5), 3.5));
    assert!(approx(fun1(Opcode::FAbs, 3.5), 3.5));
    assert!(approx(fun1(Opcode::FAbs, 0.0), 0.0));
}

#[test]
fn test_fneg() {
    assert!(approx(fun1(Opcode::FNeg, 2.5), -2.5));
    assert!(approx(fun1(Opcode::FNeg, -2.5), 2.5));
    assert!(approx(fun1(Opcode::FNeg, 0.0), 0.0));
}

#[test]
fn test_ffloor() {
    assert!(approx(fun1(Opcode::FFloor, 2.9), 2.0));
    assert!(approx(fun1(Opcode::FFloor, -2.1), -3.0));
    assert!(approx(fun1(Opcode::FFloor, 3.0), 3.0));
}

#[test]
fn test_fceil() {
    assert!(approx(fun1(Opcode::FCeil, 2.1), 3.0));
    assert!(approx(fun1(Opcode::FCeil, -2.9), -2.0));
    assert!(approx(fun1(Opcode::FCeil, 3.0), 3.0));
}

#[test]
fn test_fround() {
    assert!(approx(fun1(Opcode::FRound, 2.5), 3.0));
    assert!(approx(fun1(Opcode::FRound, 2.4), 2.0));
    assert!(approx(fun1(Opcode::FRound, -2.5), -3.0));
}

// ─────────────────────────────────────────────────────────────────────────────
// Comparison (returns i64 0 or 1)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_fcmpeq() {
    assert_eq!(fcmp(Opcode::FCmpEq, 1.0, 1.0), 1);
    assert_eq!(fcmp(Opcode::FCmpEq, 1.0, 2.0), 0);
    assert_eq!(fcmp(Opcode::FCmpEq, -0.0, 0.0), 1);
}

#[test]
fn test_fcmplt() {
    assert_eq!(fcmp(Opcode::FCmpLt, 1.0, 2.0), 1);
    assert_eq!(fcmp(Opcode::FCmpLt, 2.0, 1.0), 0);
    assert_eq!(fcmp(Opcode::FCmpLt, 1.0, 1.0), 0);
}

#[test]
fn test_fcmple() {
    assert_eq!(fcmp(Opcode::FCmpLe, 1.0, 2.0), 1);
    assert_eq!(fcmp(Opcode::FCmpLe, 1.0, 1.0), 1);
    assert_eq!(fcmp(Opcode::FCmpLe, 2.0, 1.0), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Type Conversion
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_f64_to_i64() {
    let mut block = IRBlock::new("op", Opcode::F64ToI64);
    block.inputs = vec!["a".to_string()];
    let mut ctx = HashMap::new();

    ctx.insert("a".to_string(), PayloadValue::F64(3.9));
    assert_eq!(eval_opcode(&block, &ctx), PayloadValue::I64(3));

    ctx.insert("a".to_string(), PayloadValue::F64(-3.9));
    assert_eq!(eval_opcode(&block, &ctx), PayloadValue::I64(-3));

    ctx.insert("a".to_string(), PayloadValue::F64(0.0));
    assert_eq!(eval_opcode(&block, &ctx), PayloadValue::I64(0));
}

#[test]
fn test_i64_to_f64() {
    let mut block = IRBlock::new("op", Opcode::I64ToF64);
    block.inputs = vec!["a".to_string()];
    let mut ctx = HashMap::new();

    ctx.insert("a".to_string(), PayloadValue::I64(42));
    match eval_opcode(&block, &ctx) {
        PayloadValue::F64(v) => assert!(approx(v, 42.0)),
        other => panic!("expected F64, got {other:?}"),
    }

    ctx.insert("a".to_string(), PayloadValue::I64(-7));
    match eval_opcode(&block, &ctx) {
        PayloadValue::F64(v) => assert!(approx(v, -7.0)),
        other => panic!("expected F64, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PayloadValue::F64 codec round-trip
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_f64_payload_codec() {
    use urx_runtime_v08::packet::PayloadCodec;

    let cases = [0.0_f64, 1.0, -1.0, 3.14159265358979, f64::MAX, f64::MIN_POSITIVE];
    for v in cases {
        let encoded = PayloadCodec::encode(&PayloadValue::F64(v));
        let decoded = PayloadCodec::decode(&encoded);
        match decoded {
            PayloadValue::F64(r) => {
                if v.is_finite() {
                    assert!(approx(r, v), "codec round-trip failed for {v}: got {r}");
                } else {
                    assert_eq!(r.to_bits(), v.to_bits());
                }
            }
            other => panic!("expected F64, got {other:?}"),
        }
    }
}
