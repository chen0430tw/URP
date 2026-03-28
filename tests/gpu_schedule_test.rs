//! GPU scheduling integration tests
//!
//! Verifies WgpuExecutor wired into URXRuntime end-to-end:
//! - blocks routed to GPU executor, executor_name == "wgpu"
//! - multi-wave DAG computes correct results
//! - mixed CPU+GPU node graphs
//! - all batch-1+2 arithmetic/logic/shift opcodes via scheduler

#[cfg(feature = "gpu")]
mod tests {
    use std::sync::Arc;
    use urx_runtime_v08::{
        IRBlock, IREdge, IRGraph, MultifactorPolicy, Node, NodeType, Opcode, URXRuntime,
        WgpuExecutor,
    };

    async fn try_gpu() -> Option<WgpuExecutor> {
        match WgpuExecutor::new().await {
            Ok(ex) => { println!("[gpu-sched] adapter: {}", ex.adapter_info); Some(ex) }
            Err(e) => { println!("[gpu-sched] no adapter, skip — {e}"); None }
        }
    }

    fn gpu_node(id: &str) -> Node {
        let mut n = Node::new(id, NodeType::Cpu, 200.0);
        n.tags.push("gpu".to_string());
        n
    }

    fn const_block(id: &str, val: i64, tag: &str) -> IRBlock {
        let mut b = IRBlock::new(id, Opcode::UConstI64(val));
        b.required_tag = tag.to_string();
        b.resource_shape = "leaf".to_string();
        b
    }

    /// Binary op block: inputs must be named "a" and "b"
    fn binop(id: &str, opcode: Opcode, tag: &str) -> IRBlock {
        let mut b = IRBlock::new(id, opcode);
        b.inputs = vec!["a".to_string(), "b".to_string()];
        b.required_tag = tag.to_string();
        b.resource_shape = "compute".to_string();
        b
    }

    /// Edge: src_block → dst_block, value stored under `input_key` in dst's context
    fn edge(src: &str, dst: &str, input_key: &str) -> IREdge {
        IREdge {
            src_block: src.to_string(),
            dst_block: dst.to_string(),
            output_key: src.to_string(),
            input_key: input_key.to_string(),
        }
    }

    fn val(result: &urx_runtime_v08::RuntimeResult, id: &str) -> i64 {
        match &result.results.iter().find(|r| r.block_id == id).unwrap().value {
            urx_runtime_v08::packet::PayloadValue::I64(v) => *v,
            other => panic!("{id}: expected I64, got {other:?}"),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 1: single UAdd through runtime
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_schedule_uadd() {
        let Some(gpu) = try_gpu().await else { return };
        let mut rt = URXRuntime::new(vec![gpu_node("gpu0")], MultifactorPolicy::new());
        rt.executors.register("gpu0", Arc::new(gpu));

        let mut g = IRGraph::with_id("gpu-uadd".to_string());
        g.blocks.push(const_block("ca", 30, "gpu"));
        g.blocks.push(const_block("cb", 12, "gpu"));
        g.blocks.push(binop("add", Opcode::UAdd, "gpu"));
        g.edges.push(edge("ca", "add", "a"));
        g.edges.push(edge("cb", "add", "b"));

        let result = rt.execute_graph(&g).await;

        let r = result.results.iter().find(|r| r.block_id == "add").unwrap();
        assert_eq!(val(&result, "add"), 42);
        assert_eq!(r.executor_name, "wgpu");
        println!("[gpu-sched] UAdd 30+12={:?} on {} ✓", r.value, r.executor_name);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 2: chained DAG  (a+b) * (c-d)  = (3+7)*(20-10) = 100
    //   Wave 1: ca=3, cb=7, cc=20, cd=10
    //   Wave 2: sum=a+b=10,  diff=c-d=10
    //   Wave 3: prod=sum*diff=100
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_schedule_chain() {
        let Some(gpu) = try_gpu().await else { return };
        let mut rt = URXRuntime::new(vec![gpu_node("gpu0")], MultifactorPolicy::new());
        rt.executors.register("gpu0", Arc::new(gpu));

        let mut g = IRGraph::with_id("gpu-chain".to_string());
        g.blocks.push(const_block("ca",  3, "gpu"));
        g.blocks.push(const_block("cb",  7, "gpu"));
        g.blocks.push(const_block("cc", 20, "gpu"));
        g.blocks.push(const_block("cd", 10, "gpu"));
        // give each compute block a unique zone to prevent fuse_linear_blocks merging them
        let mut sum_b = binop("sum",  Opcode::UAdd, "gpu");
        sum_b.preferred_zone = "z-sum".to_string();
        let mut diff_b = binop("diff", Opcode::USub, "gpu");
        diff_b.preferred_zone = "z-diff".to_string();
        let mut prod_b = binop("prod", Opcode::UMul, "gpu");
        prod_b.preferred_zone = "z-prod".to_string();
        g.blocks.push(sum_b);
        g.blocks.push(diff_b);
        g.blocks.push(prod_b);

        g.edges.push(edge("ca", "sum",  "a"));
        g.edges.push(edge("cb", "sum",  "b"));
        g.edges.push(edge("cc", "diff", "a"));
        g.edges.push(edge("cd", "diff", "b"));
        g.edges.push(edge("sum",  "prod", "a"));
        g.edges.push(edge("diff", "prod", "b"));

        let result = rt.execute_graph(&g).await;

        assert_eq!(val(&result, "sum"),  10);
        assert_eq!(val(&result, "diff"), 10);
        assert_eq!(val(&result, "prod"), 100);
        println!("[gpu-sched] (3+7)*(20-10) = {} ✓", val(&result, "prod"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 3: comparison pipeline  5 < 8 → 1 (true)
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_schedule_cmp() {
        let Some(gpu) = try_gpu().await else { return };
        let mut rt = URXRuntime::new(vec![gpu_node("gpu0")], MultifactorPolicy::new());
        rt.executors.register("gpu0", Arc::new(gpu));

        let mut g = IRGraph::with_id("gpu-cmp".to_string());
        g.blocks.push(const_block("cx", 5, "gpu"));
        g.blocks.push(const_block("cy", 8, "gpu"));
        g.blocks.push(binop("lt",  Opcode::UCmpLt, "gpu")); // 5 < 8 = 1
        g.blocks.push(binop("eq",  Opcode::UCmpEq, "gpu")); // 5 == 8 = 0
        g.blocks.push(binop("le",  Opcode::UCmpLe, "gpu")); // 5 <= 8 = 1

        for op in ["lt","eq","le"] {
            g.edges.push(edge("cx", op, "a"));
            g.edges.push(edge("cy", op, "b"));
        }

        let result = rt.execute_graph(&g).await;
        assert_eq!(val(&result, "lt"), 1);
        assert_eq!(val(&result, "eq"), 0);
        assert_eq!(val(&result, "le"), 1);
        println!("[gpu-sched] cmp: lt={} eq={} le={} ✓",
            val(&result,"lt"), val(&result,"eq"), val(&result,"le"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 4: USelect  — cond = (x < y), result picks x when true
    //   x=5, y=8 → cond=1 → result=5
    //   Uses a 3-input block: inputs = ["cond","a","b"]
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_schedule_select() {
        let Some(gpu) = try_gpu().await else { return };
        let mut rt = URXRuntime::new(vec![gpu_node("gpu0")], MultifactorPolicy::new());
        rt.executors.register("gpu0", Arc::new(gpu));

        let mut g = IRGraph::with_id("gpu-select".to_string());
        g.blocks.push(const_block("cx", 5, "gpu"));
        g.blocks.push(const_block("cy", 8, "gpu"));

        // cond = cx < cy
        let mut cond_b = binop("cond", Opcode::UCmpLt, "gpu");
        cond_b.preferred_zone = "z-cond".to_string();
        g.edges.push(edge("cx", "cond", "a"));
        g.edges.push(edge("cy", "cond", "b"));
        g.blocks.push(cond_b);

        // select(cond, cx, cy) → picks cx when cond != 0
        let mut sel = IRBlock::new("sel", Opcode::USelect);
        sel.inputs = vec!["cond".to_string(), "a".to_string(), "b".to_string()];
        sel.required_tag = "gpu".to_string();
        sel.resource_shape = "compute".to_string();
        sel.preferred_zone = "z-sel".to_string();
        g.edges.push(edge("cond", "sel", "cond"));
        g.edges.push(edge("cx",   "sel", "a"));
        g.edges.push(edge("cy",   "sel", "b"));
        g.blocks.push(sel);

        let result = rt.execute_graph(&g).await;
        assert_eq!(val(&result, "sel"), 5); // picked x=5 because 5 < 8
        println!("[gpu-sched] select: min(5,8) = {} ✓", val(&result,"sel"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 5: mixed CPU+GPU nodes — consts on cpu0, compute on gpu0
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_schedule_mixed_nodes() {
        let Some(gpu) = try_gpu().await else { return };

        let mut cpu_node = Node::new("cpu0", NodeType::Cpu, 100.0);
        cpu_node.tags.push("cpu".to_string());

        let mut rt = URXRuntime::new(vec![gpu_node("gpu0"), cpu_node], MultifactorPolicy::new());
        rt.executors.register("gpu0", Arc::new(gpu));

        let mut g = IRGraph::with_id("mixed".to_string());
        let mut ca = const_block("ca", 100, "cpu");
        ca.preferred_zone = "zone-a".to_string();
        let mut cb = const_block("cb", 55, "cpu");
        cb.preferred_zone = "zone-a".to_string();
        let mut sub = binop("sub", Opcode::USub, "gpu");
        sub.preferred_zone = "zone-b".to_string();

        g.blocks.push(ca);
        g.blocks.push(cb);
        g.blocks.push(sub);
        g.edges.push(edge("ca", "sub", "a"));
        g.edges.push(edge("cb", "sub", "b"));

        let result = rt.execute_graph(&g).await;
        assert_eq!(val(&result, "sub"), 45); // 100 - 55
        let r = result.results.iter().find(|r| r.block_id == "sub").unwrap();
        println!("[gpu-sched] mixed: 100-55={} on {} ✓", val(&result,"sub"), r.executor_name);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 6: all batch-1+2 GPU opcodes in one graph
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_schedule_all_opcodes() {
        let Some(gpu) = try_gpu().await else { return };
        let mut rt = URXRuntime::new(vec![gpu_node("gpu0")], MultifactorPolicy::new());
        rt.executors.register("gpu0", Arc::new(gpu));

        // inputs: n12=12, n4=4, n3=3
        // all compute blocks use inputs "a" and "b" → edges map to those keys
        let ops: Vec<(&str, Opcode, &str, &str, i64)> = vec![
            ("add",  Opcode::UAdd,  "n12","n4",  16),  // 12+4
            ("sub",  Opcode::USub,  "n12","n4",   8),  // 12-4
            ("mul",  Opcode::UMul,  "n12","n4",  48),  // 12*4
            ("div",  Opcode::UDiv,  "n12","n3",   4),  // 12/3
            ("rem",  Opcode::URem,  "n12","n4",   0),  // 12%4
            ("eq",   Opcode::UCmpEq,"n4","n4",    1),  // 4==4
            ("lt",   Opcode::UCmpLt,"n4","n12",   1),  // 4<12
            ("le",   Opcode::UCmpLe,"n12","n12",  1),  // 12<=12
            ("and",  Opcode::UAnd,  "n12","n4",   4),  // 0b1100 & 0b0100
            ("or",   Opcode::UOr,   "n12","n4",  12),  // 12|4
            ("xor",  Opcode::UXor,  "n12","n4",   8),  // 12^4
            ("shl",  Opcode::UShl,  "n3","n3",   24),  // 3<<3
            ("shr",  Opcode::UShr,  "n12","n3",   1),  // 12>>3
            ("shra", Opcode::UShra, "n12","n3",   1),  // 12>>3
        ];

        let mut g = IRGraph::with_id("all-ops".to_string());
        g.blocks.push(const_block("n12", 12, "gpu"));
        g.blocks.push(const_block("n4",   4, "gpu"));
        g.blocks.push(const_block("n3",   3, "gpu"));

        for (id, opcode, src_a, src_b, _) in &ops {
            g.blocks.push(binop(id, opcode.clone(), "gpu"));
            g.edges.push(edge(src_a, id, "a"));
            g.edges.push(edge(src_b, id, "b"));
        }

        let result = rt.execute_graph(&g).await;

        for (id, _, _, _, expected) in &ops {
            let got = val(&result, id);
            assert_eq!(got, *expected, "{id}: expected {expected}, got {got}");
        }
        println!("[gpu-sched] all batch-1+2 opcodes via scheduler ✓");
    }
}
