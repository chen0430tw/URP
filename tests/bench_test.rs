//! Throughput benchmark: CPU sequential vs JIT GPU dispatch.
//!
//! Not a criterion bench — uses `std::time::Instant` and prints results.
//! Run with: `cargo test bench_ -- --nocapture`

use std::collections::HashMap;
use std::time::Instant;
use urx_runtime_v08::{compile_graph, eval_opcode, IRBlock, IREdge, IRGraph, Opcode};
use urx_runtime_v08::packet::PayloadValue;

fn edge(src: &str, dst: &str, key: &str) -> IREdge {
    IREdge {
        src_block: src.to_string(),
        dst_block: dst.to_string(),
        output_key: src.to_string(),
        input_key: key.to_string(),
    }
}

/// Build the benchmark graph: sqrt(a*a + b*b)
fn l2_graph() -> IRGraph {
    let mut g = IRGraph::with_id("bench_l2".into());
    g.blocks.push(IRBlock::new("a",   Opcode::FConst(0.0)));
    g.blocks.push(IRBlock::new("b",   Opcode::FConst(0.0)));
    let mut aa  = IRBlock::new("aa",  Opcode::FMul);  aa.inputs  = vec!["a".into(), "a".into()];
    let mut bb  = IRBlock::new("bb",  Opcode::FMul);  bb.inputs  = vec!["b".into(), "b".into()];
    let mut sum = IRBlock::new("sum", Opcode::FAdd);  sum.inputs = vec!["a".into(), "b".into()];
    let mut sq  = IRBlock::new("sq",  Opcode::FSqrt); sq.inputs  = vec!["a".into()];
    g.blocks.extend([aa, bb, sum, sq]);
    g.edges.push(edge("a",   "aa",  "a")); g.edges.push(edge("a",  "aa",  "b"));
    g.edges.push(edge("b",   "bb",  "a")); g.edges.push(edge("b",  "bb",  "b"));
    g.edges.push(edge("aa",  "sum", "a")); g.edges.push(edge("bb", "sum", "b"));
    g.edges.push(edge("sum", "sq",  "a"));
    g
}

fn cpu_eval_l2(a: f64, b: f64) -> f64 {
    let g = l2_graph();
    // Manually walk the eval in topo order using eval_opcode per block
    // This mirrors how the runtime would call each block sequentially.
    let mut ctx: HashMap<String, PayloadValue> = HashMap::new();
    ctx.insert("a".into(), PayloadValue::F64(a));
    ctx.insert("b".into(), PayloadValue::F64(b));

    // aa = a*a
    let mut aa_blk = IRBlock::new("aa", Opcode::FMul);
    aa_blk.inputs = vec!["a".into(), "a".into()];
    let aa_val = eval_opcode(&aa_blk, &ctx);
    ctx.insert("aa".into(), aa_val);

    // bb = b*b
    let mut bb_blk = IRBlock::new("bb", Opcode::FMul);
    bb_blk.inputs = vec!["b".into(), "b".into()];
    let bb_val = eval_opcode(&bb_blk, &ctx);
    ctx.insert("bb".into(), bb_val);

    // sum = aa + bb
    let mut sum_blk = IRBlock::new("sum", Opcode::FAdd);
    sum_blk.inputs = vec!["aa".into(), "bb".into()];
    let sum_val = eval_opcode(&sum_blk, &ctx);
    ctx.insert("sum".into(), sum_val);

    // sq = sqrt(sum)
    let mut sq_blk = IRBlock::new("sq", Opcode::FSqrt);
    sq_blk.inputs = vec!["sum".into()];
    match eval_opcode(&sq_blk, &ctx) {
        PayloadValue::F64(v) => v,
        _ => panic!("expected F64"),
    }
}

#[test]
fn bench_cpu_sequential() {
    const N: usize = 10_000;
    let t0 = Instant::now();
    let mut checksum = 0.0f64;
    for i in 0..N {
        let a = (i % 100) as f64;
        let b = ((i * 3) % 100) as f64;
        checksum += cpu_eval_l2(a, b);
    }
    let elapsed = t0.elapsed();
    println!(
        "[bench] CPU sequential  N={N}: {:>8.2}ms  ({:.0} items/s)  checksum={checksum:.1}",
        elapsed.as_secs_f64() * 1000.0,
        N as f64 / elapsed.as_secs_f64(),
    );
    assert!(checksum > 0.0);
}

#[test]
fn bench_compile_graph() {
    const ITERS: usize = 1_000;
    let g = l2_graph();
    let t0 = Instant::now();
    for _ in 0..ITERS {
        let _ = compile_graph(&g).unwrap();
    }
    let elapsed = t0.elapsed();
    println!(
        "[bench] compile_graph   iters={ITERS}: {:>8.2}ms  ({:.0} compiles/s)",
        elapsed.as_secs_f64() * 1000.0,
        ITERS as f64 / elapsed.as_secs_f64(),
    );
}

#[cfg(feature = "gpu")]
mod gpu_bench {
    use super::*;
    use urx_runtime_v08::JitExecutor;

    #[tokio::test]
    async fn bench_jit_gpu_batch() {
        let jit = match JitExecutor::new().await {
            Ok(j) => j,
            Err(e) => {
                println!("[bench] No GPU adapter ({e}), skipping JIT bench");
                return;
            }
        };
        println!("[bench] JIT adapter: {}", jit.adapter_info);

        let g = l2_graph();
        let compiled = jit.compile(&g).unwrap();

        for &n in &[1_000usize, 10_000, 100_000, 1_000_000] {
            let a_vals: Vec<f32> = (0..n).map(|i| (i % 100) as f32).collect();
            let b_vals: Vec<f32> = (0..n).map(|i| ((i * 3) % 100) as f32).collect();

            let t0 = Instant::now();
            let outputs = jit.run(&compiled, &[a_vals, b_vals], n).unwrap();
            let elapsed = t0.elapsed();

            let checksum: f32 = outputs[0].iter().sum();
            println!(
                "[bench] JIT GPU batch   N={n:>8}: {:>8.2}ms  ({:.0} items/s)  checksum={checksum:.1}",
                elapsed.as_secs_f64() * 1000.0,
                n as f64 / elapsed.as_secs_f64(),
            );
        }
    }
}
