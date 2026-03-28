//! JIT end-to-end integration tests
//!
//! These tests compile a full IRGraph to WGSL and run it on the GPU with N
//! independent work items.  If no GPU adapter is available the tests skip
//! gracefully (same pattern as gpu_test.rs).

use urx_runtime_v08::{compile_graph, IRBlock, IREdge, IRGraph, Opcode};

fn edge(src: &str, dst: &str, key: &str) -> IREdge {
    IREdge {
        src_block: src.to_string(),
        dst_block: dst.to_string(),
        output_key: src.to_string(),
        input_key: key.to_string(),
    }
}

// ── CPU compile_graph smoke tests ─────────────────────────────────────────────

#[test]
fn test_jit_compile_linear_chain() {
    // (a + b) * c
    let mut g = IRGraph::with_id("linear".into());
    g.blocks.push(IRBlock::new("a", Opcode::UConstI64(0)));
    g.blocks.push(IRBlock::new("b", Opcode::UConstI64(0)));
    g.blocks.push(IRBlock::new("c", Opcode::UConstI64(0)));
    let mut add = IRBlock::new("add", Opcode::UAdd);
    add.inputs = vec!["a".into(), "b".into()];
    let mut mul = IRBlock::new("mul", Opcode::UMul);
    mul.inputs = vec!["a".into(), "b".into()];
    g.blocks.extend([add, mul]);
    g.edges.push(edge("a", "add", "a"));
    g.edges.push(edge("b", "add", "b"));
    g.edges.push(edge("add", "mul", "a"));
    g.edges.push(edge("c",   "mul", "b"));

    let compiled = compile_graph(&g).unwrap();
    assert_eq!(compiled.n_regs, 5);
    assert_eq!(compiled.input_indices.len(), 3);  // a, b, c are inputs
    assert_eq!(compiled.output_indices.len(), 1);  // mul is the single output
    assert!(compiled.wgsl_source.contains("r3 = r0 + r1;"));  // add
    assert!(compiled.wgsl_source.contains("r4 = r3 * r2;"));  // mul
}

#[test]
fn test_jit_compile_float_chain() {
    // (a*a + b*b).sqrt()  — L2 norm of 2D vector
    let mut g = IRGraph::with_id("l2norm".into());
    g.blocks.push(IRBlock::new("a", Opcode::FConst(0.0)));
    g.blocks.push(IRBlock::new("b", Opcode::FConst(0.0)));
    let mut aa = IRBlock::new("aa", Opcode::FMul); aa.inputs = vec!["a".into(), "a".into()];
    let mut bb = IRBlock::new("bb", Opcode::FMul); bb.inputs = vec!["b".into(), "b".into()];
    let mut sum = IRBlock::new("s",  Opcode::FAdd); sum.inputs = vec!["a".into(), "b".into()];
    let mut sq  = IRBlock::new("h",  Opcode::FSqrt); sq.inputs = vec!["a".into()];
    g.blocks.extend([aa, bb, sum, sq]);
    g.edges.push(edge("a",  "aa", "a")); g.edges.push(edge("a", "aa", "b"));
    g.edges.push(edge("b",  "bb", "a")); g.edges.push(edge("b", "bb", "b"));
    g.edges.push(edge("aa", "s",  "a")); g.edges.push(edge("bb", "s", "b"));
    g.edges.push(edge("s",  "h",  "a"));

    let compiled = compile_graph(&g).unwrap();
    assert!(compiled.wgsl_source.contains("sqrt("));
    assert_eq!(compiled.output_indices.len(), 1);
}

#[test]
fn test_jit_compile_select() {
    // select(cond, a, b)
    let mut g = IRGraph::with_id("sel".into());
    g.blocks.push(IRBlock::new("cond", Opcode::UConstI64(0)));
    g.blocks.push(IRBlock::new("a",    Opcode::UConstI64(0)));
    g.blocks.push(IRBlock::new("b",    Opcode::UConstI64(0)));
    let mut sel = IRBlock::new("sel", Opcode::USelect);
    sel.inputs = vec!["cond".into(), "a".into(), "b".into()];
    g.blocks.push(sel);
    g.edges.push(edge("cond", "sel", "cond"));
    g.edges.push(edge("a",    "sel", "a"));
    g.edges.push(edge("b",    "sel", "b"));

    let compiled = compile_graph(&g).unwrap();
    assert!(compiled.wgsl_source.contains("select("));
}

// ── GPU tests (feature = "gpu") ───────────────────────────────────────────────

#[cfg(feature = "gpu")]
mod gpu {
    use super::*;
    use urx_runtime_v08::JitExecutor;

    #[tokio::test]
    async fn test_jit_gpu_integer_add_batch() {
        let jit = match JitExecutor::new().await {
            Ok(j) => j,
            Err(_) => return,
        };
        println!("[jit-gpu] adapter: {}", jit.adapter_info);

        // Graph: a + b  (integer)
        let mut g = IRGraph::with_id("add".into());
        g.blocks.push(IRBlock::new("a", Opcode::UConstI64(0)));
        g.blocks.push(IRBlock::new("b", Opcode::UConstI64(0)));
        let mut add = IRBlock::new("add", Opcode::UAdd);
        add.inputs = vec!["a".into(), "b".into()];
        g.blocks.push(add);
        g.edges.push(edge("a", "add", "a"));
        g.edges.push(edge("b", "add", "b"));

        let compiled = jit.compile(&g).unwrap();
        let n = 1024;
        // inputs: slot 0 = a values (1..=n), slot 1 = b values (all 1.0)
        let a_vals: Vec<f32> = (1..=n as i32).map(|x| x as f32).collect();
        let b_vals: Vec<f32> = vec![1.0f32; n];
        let outputs = jit.run(&compiled, &[a_vals, b_vals], n).unwrap();

        // output[0][i] = (i+1) + 1 = i+2
        for i in 0..n {
            let expected = (i as f32 + 1.0) + 1.0;
            assert!((outputs[0][i] - expected).abs() < 0.5,
                "item {i}: expected {expected}, got {}", outputs[0][i]);
        }
        println!("[jit-gpu] integer add N={n} ✓");
    }

    #[tokio::test]
    async fn test_jit_gpu_float_sqrt_batch() {
        let jit = match JitExecutor::new().await {
            Ok(j) => j,
            Err(_) => return,
        };

        // Graph: sqrt(a)
        let mut g = IRGraph::with_id("sqrt".into());
        g.blocks.push(IRBlock::new("a", Opcode::FConst(0.0)));
        let mut sq = IRBlock::new("s", Opcode::FSqrt);
        sq.inputs = vec!["a".into()];
        g.blocks.push(sq);
        g.edges.push(edge("a", "s", "a"));

        let compiled = jit.compile(&g).unwrap();
        let n = 512;
        // sqrt(i*i) should equal i (for i = 0..n)
        let a_vals: Vec<f32> = (0..n as i32).map(|x| (x * x) as f32).collect();
        let outputs = jit.run(&compiled, &[a_vals], n).unwrap();

        for i in 0..n {
            let expected = i as f32;
            assert!((outputs[0][i] - expected).abs() < 1e-2,
                "sqrt({}^2): expected {expected}, got {}", i, outputs[0][i]);
        }
        println!("[jit-gpu] float sqrt N={n} ✓");
    }

    #[tokio::test]
    async fn test_jit_gpu_l2norm_batch() {
        let jit = match JitExecutor::new().await {
            Ok(j) => j,
            Err(_) => return,
        };

        // Graph: sqrt(a*a + b*b)
        let mut g = IRGraph::with_id("l2".into());
        g.blocks.push(IRBlock::new("a", Opcode::FConst(0.0)));
        g.blocks.push(IRBlock::new("b", Opcode::FConst(0.0)));
        let mut aa  = IRBlock::new("aa",  Opcode::FMul);  aa.inputs  = vec!["a".into(), "a".into()];
        let mut bb  = IRBlock::new("bb",  Opcode::FMul);  bb.inputs  = vec!["b".into(), "b".into()];
        let mut sum = IRBlock::new("sum", Opcode::FAdd);  sum.inputs = vec!["a".into(), "b".into()];
        let mut sq  = IRBlock::new("sq",  Opcode::FSqrt); sq.inputs  = vec!["a".into()];
        g.blocks.extend([aa, bb, sum, sq]);
        g.edges.push(edge("a",   "aa",  "a")); g.edges.push(edge("a",  "aa",  "b"));
        g.edges.push(edge("b",   "bb",  "a")); g.edges.push(edge("b",  "bb",  "b"));
        g.edges.push(edge("aa",  "sum", "a")); g.edges.push(edge("bb", "sum", "b"));
        g.edges.push(edge("sum", "sq",  "a"));

        let compiled = jit.compile(&g).unwrap();
        let n = 256;
        // 3-4-5 triangle: sqrt(3^2 + 4^2) = 5
        let a_vals = vec![3.0f32; n];
        let b_vals = vec![4.0f32; n];
        let outputs = jit.run(&compiled, &[a_vals, b_vals], n).unwrap();
        for i in 0..n {
            assert!((outputs[0][i] - 5.0).abs() < 1e-2,
                "item {i}: expected 5.0, got {}", outputs[0][i]);
        }
        println!("[jit-gpu] L2 norm (3,4)→5 N={n} ✓");
    }
}
