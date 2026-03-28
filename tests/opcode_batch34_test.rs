//! Opcode correctness tests — Batch 3 (String / Type Conversion) and Batch 4 (Select + Aggregation)

use std::collections::HashMap;
use urx_runtime_v08::{eval_opcode, IRBlock, Opcode};
use urx_runtime_v08::packet::PayloadValue;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn ctx1s(key: &str, val: &str) -> HashMap<String, PayloadValue> {
    let mut m = HashMap::new();
    m.insert(key.to_string(), PayloadValue::Str(val.to_string()));
    m
}

fn ctx1i(key: &str, val: i64) -> HashMap<String, PayloadValue> {
    let mut m = HashMap::new();
    m.insert(key.to_string(), PayloadValue::I64(val));
    m
}

fn run_s1(opcode: Opcode, a: &str) -> PayloadValue {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["a".to_string()];
    eval_opcode(&block, &ctx1s("a", a))
}

fn run_i1(opcode: Opcode, a: i64) -> PayloadValue {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["a".to_string()];
    eval_opcode(&block, &ctx1i("a", a))
}

fn run_si2(opcode: Opcode, s: &str, start: i64, end: i64) -> PayloadValue {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["s".to_string(), "start".to_string(), "end".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("s".to_string(),     PayloadValue::Str(s.to_string()));
    ctx.insert("start".to_string(), PayloadValue::I64(start));
    ctx.insert("end".to_string(),   PayloadValue::I64(end));
    eval_opcode(&block, &ctx)
}

fn run_ss2(opcode: Opcode, s: &str, delim: &str) -> PayloadValue {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["s".to_string(), "d".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("s".to_string(), PayloadValue::Str(s.to_string()));
    ctx.insert("d".to_string(), PayloadValue::Str(delim.to_string()));
    eval_opcode(&block, &ctx)
}

fn run_b4(opcode: Opcode, cond: i64, a: i64, b: i64) -> PayloadValue {
    let mut block = IRBlock::new("op", opcode);
    block.inputs = vec!["cond".to_string(), "a".to_string(), "b".to_string()];
    let mut ctx = HashMap::new();
    ctx.insert("cond".to_string(), PayloadValue::I64(cond));
    ctx.insert("a".to_string(),    PayloadValue::I64(a));
    ctx.insert("b".to_string(),    PayloadValue::I64(b));
    eval_opcode(&block, &ctx)
}

fn run_i2(opcode: Opcode, a: i64, b: i64) -> i64 {
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

// ─────────────────────────────────────────────────────────────────────────────
// Batch 3 — UI64ToStr
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ui64tostr() {
    assert_eq!(run_i1(Opcode::UI64ToStr, 42),   PayloadValue::Str("42".to_string()));
    assert_eq!(run_i1(Opcode::UI64ToStr, -7),   PayloadValue::Str("-7".to_string()));
    assert_eq!(run_i1(Opcode::UI64ToStr, 0),    PayloadValue::Str("0".to_string()));
    assert_eq!(run_i1(Opcode::UI64ToStr, 1000), PayloadValue::Str("1000".to_string()));
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 3 — UStrToI64
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ustrtoi64() {
    assert_eq!(run_s1(Opcode::UStrToI64, "42"),   PayloadValue::I64(42));
    assert_eq!(run_s1(Opcode::UStrToI64, "-7"),   PayloadValue::I64(-7));
    assert_eq!(run_s1(Opcode::UStrToI64, "  0 "), PayloadValue::I64(0)); // trims whitespace
}

#[test]
#[should_panic(expected = "cannot parse")]
fn test_ustrtoi64_invalid() {
    run_s1(Opcode::UStrToI64, "hello");
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 3 — UStrLen
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ustrlen() {
    assert_eq!(run_s1(Opcode::UStrLen, "hello"), PayloadValue::I64(5));
    assert_eq!(run_s1(Opcode::UStrLen, ""),       PayloadValue::I64(0));
    assert_eq!(run_s1(Opcode::UStrLen, "你好"),    PayloadValue::I64(2)); // unicode chars
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 3 — UStrSlice
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ustrslice() {
    assert_eq!(run_si2(Opcode::UStrSlice, "hello", 1, 4),
               PayloadValue::Str("ell".to_string()));
    assert_eq!(run_si2(Opcode::UStrSlice, "hello", 0, 5),
               PayloadValue::Str("hello".to_string()));
    assert_eq!(run_si2(Opcode::UStrSlice, "hello", 2, 2),
               PayloadValue::Str("".to_string()));
    // clamp end beyond length
    assert_eq!(run_si2(Opcode::UStrSlice, "hi", 0, 100),
               PayloadValue::Str("hi".to_string()));
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 3 — UStrSplit
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_ustrsplit() {
    let result = run_ss2(Opcode::UStrSplit, "a,b,c", ",");
    assert_eq!(result, PayloadValue::List(vec![
        PayloadValue::Str("a".to_string()),
        PayloadValue::Str("b".to_string()),
        PayloadValue::Str("c".to_string()),
    ]));
}

#[test]
fn test_ustrsplit_no_match() {
    // delimiter not present → single-element list
    let result = run_ss2(Opcode::UStrSplit, "hello", ",");
    assert_eq!(result, PayloadValue::List(vec![
        PayloadValue::Str("hello".to_string()),
    ]));
}

#[test]
fn test_ustrsplit_empty_parts() {
    let result = run_ss2(Opcode::UStrSplit, "a,,b", ",");
    assert_eq!(result, PayloadValue::List(vec![
        PayloadValue::Str("a".to_string()),
        PayloadValue::Str("".to_string()),
        PayloadValue::Str("b".to_string()),
    ]));
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 4 — USelect
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_uselect() {
    // cond != 0 → picks input[1] (a = 10)
    assert_eq!(run_b4(Opcode::USelect, 1,  10, 20), PayloadValue::I64(10));
    assert_eq!(run_b4(Opcode::USelect, 42, 10, 20), PayloadValue::I64(10));
    // cond == 0 → picks input[2] (b = 20)
    assert_eq!(run_b4(Opcode::USelect, 0,  10, 20), PayloadValue::I64(20));
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 4 — UMin / UMax
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_umin() {
    assert_eq!(run_i2(Opcode::UMin, 3, 7),   3);
    assert_eq!(run_i2(Opcode::UMin, -5, 2),  -5);
    assert_eq!(run_i2(Opcode::UMin, 0, 0),   0);
}

#[test]
fn test_umax() {
    assert_eq!(run_i2(Opcode::UMax, 3, 7),   7);
    assert_eq!(run_i2(Opcode::UMax, -5, 2),  2);
    assert_eq!(run_i2(Opcode::UMax, 0, 0),   0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 4 — UAbs
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_uabs() {
    assert_eq!(run_i1(Opcode::UAbs, -42), PayloadValue::I64(42));
    assert_eq!(run_i1(Opcode::UAbs,  42), PayloadValue::I64(42));
    assert_eq!(run_i1(Opcode::UAbs,   0), PayloadValue::I64(0));
}

// ─────────────────────────────────────────────────────────────────────────────
// Batch 4 — UAssert
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_uassert_passes() {
    // non-zero cond → returns the cond value as pass-through
    assert_eq!(run_i1(Opcode::UAssert, 1),  PayloadValue::I64(1));
    assert_eq!(run_i1(Opcode::UAssert, 99), PayloadValue::I64(99));
    assert_eq!(run_i1(Opcode::UAssert, -1), PayloadValue::I64(-1));
}

#[test]
#[should_panic(expected = "condition is 0")]
fn test_uassert_fails() {
    run_i1(Opcode::UAssert, 0);
}
