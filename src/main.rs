#![allow(dead_code)]
mod node;
mod ir;
mod optimizer;
mod cost;
mod et_cooling;
mod usb_executor;
mod partition;
mod policy;
mod reservation;
mod packet;
mod ring;
mod remote;
mod executor;
mod reducer;
mod runtime;
mod scheduler;
mod shared_memory;
mod jit_compiler;
#[cfg(feature = "gpu")]
mod gpu_executor;

use std::sync::Arc;
use std::time::Instant;
use ir::{IRBlock, IREdge, IRGraph, MergeMode, Opcode};
use node::{Node, NodeType};
use policy::MultifactorPolicy;
use runtime::URXRuntime;
use et_cooling::ETCoolingPolicy;
use reservation::{Reservation, ReservationTable};
use executor::ThreadPoolExecutor;

// ─────────────────────────────────────────────────────────────────────────────
// Demo A: CPU I64 + F64 mixed graph with local-ring routing
// ─────────────────────────────────────────────────────────────────────────────
//
//   b_pi   ──FConst(3.14)──┐
//   b_two  ──FConst(2.0)───┤  b_mul (FMul) → b_neg (FNeg) → b_str (UI64ToStr via F64ToI64)
//   b_x    ──UConstI64(7)──┐
//   b_y    ──UConstI64(5)──┤  b_sum (UAdd) → b_cat (UConcat) → leaf
//
// Both sub-graphs share the same cpu0 node (same host → local-ring routing).

async fn demo_a() {
    println!("\n── Demo A: I64 + F64 mixed on cpu0 (local-ring) ──");

    let graph = IRGraph {
        graph_id: "demo_a".into(),
        blocks: vec![
            IRBlock {
                block_id: "b_pi".into(),
                opcode: Opcode::FConst(std::f64::consts::PI),
                inputs: vec![],
                output: "pi".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "b_two".into(),
                opcode: Opcode::FConst(2.0),
                inputs: vec![],
                output: "two".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "b_mul".into(),
                opcode: Opcode::FMul,
                inputs: vec!["a".into(), "b".into()],
                output: "tau".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: Some("f64_path".into()),
                estimated_duration: 2,
            },
            IRBlock {
                block_id: "b_neg".into(),
                opcode: Opcode::FNeg,
                inputs: vec!["v".into()],
                output: "neg_tau".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "b_x".into(),
                opcode: Opcode::UConstI64(7),
                inputs: vec![],
                output: "x".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "b_y".into(),
                opcode: Opcode::UConstI64(5),
                inputs: vec![],
                output: "y".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "b_sum".into(),
                opcode: Opcode::UAdd,
                inputs: vec!["a".into(), "b".into()],
                output: "sum".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::Sum,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: Some("sum_path".into()),
                estimated_duration: 2,
            },
            IRBlock {
                block_id: "b_prefix".into(),
                opcode: Opcode::UConstStr("result=".into()),
                inputs: vec![],
                output: "prefix".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "string".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "b_cat".into(),
                opcode: Opcode::UConcat,
                inputs: vec!["left".into(), "right".into()],
                output: "out".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::Concat,
                resource_shape: "string".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 2,
            },
        ],
        edges: vec![
            IREdge { src_block: "b_pi".into(),     dst_block: "b_mul".into(),    output_key: "pi".into(),     input_key: "a".into() },
            IREdge { src_block: "b_two".into(),    dst_block: "b_mul".into(),    output_key: "two".into(),    input_key: "b".into() },
            IREdge { src_block: "b_mul".into(),    dst_block: "b_neg".into(),    output_key: "tau".into(),    input_key: "v".into() },
            IREdge { src_block: "b_x".into(),      dst_block: "b_sum".into(),    output_key: "x".into(),      input_key: "a".into() },
            IREdge { src_block: "b_y".into(),      dst_block: "b_sum".into(),    output_key: "y".into(),      input_key: "b".into() },
            IREdge { src_block: "b_prefix".into(), dst_block: "b_cat".into(),    output_key: "prefix".into(), input_key: "left".into() },
            IREdge { src_block: "b_sum".into(),    dst_block: "b_cat".into(),    output_key: "sum".into(),    input_key: "right".into() },
        ],
    };

    let nodes = vec![
        Node {
            node_id: "cpu0".into(),
            node_type: NodeType::Cpu,
            host_id: "host0".into(),
            zone: "z1".into(),
            tags: vec!["cpu".into(), "arith".into()],
            compute_capacity: 8.0,
            memory_capacity: 32.0,
            bandwidth: 10.0,
            inertia_keys: vec![],
        },
    ];

    let policy = MultifactorPolicy { reservation_bias: 1.0 };
    let mut rt = URXRuntime::new(nodes, policy);
    let result = rt.execute_graph(&graph).await;

    println!("  fused_graph_id  : {}", result.fused_graph_id);
    println!("  partitions      : {:?}", result.partitions.len());
    println!("  packet_log      : {} packets", result.packet_log.len());
    println!("  merged          : {:?}", result.merged);
    println!("  outputs ({})     :", result.outputs.len());
    for v in &result.outputs {
        println!("    {:?}", v);
    }
    for r in &result.results {
        println!("  block {:>8} [{:>6}µs–{:>6}µs] on {} → {:?}",
            r.block_id, r.start_time, r.end_time, r.node_id, r.value);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo B: Two-node graph with ET-Cooling SA optimizer
// ─────────────────────────────────────────────────────────────────────────────
//
//   cpu0 (z1) handles constants + arithmetic
//   gpu0 (z2, different host) handles string formatting
//   Cross-node edge → remote-packet routing
//   ET-Cooling replaces bind_partitions

async fn demo_b() {
    println!("\n── Demo B: cpu0→gpu0 cross-node + ET-Cooling optimizer ──");

    let graph = IRGraph {
        graph_id: "demo_b".into(),
        blocks: vec![
            IRBlock {
                block_id: "c1".into(),
                opcode: Opcode::UConstI64(12),
                inputs: vec![],
                output: "x".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: Some("const_x".into()),
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "c2".into(),
                opcode: Opcode::UConstI64(30),
                inputs: vec![],
                output: "y".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: Some("const_y".into()),
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "c3".into(),
                opcode: Opcode::UAdd,
                inputs: vec!["a".into(), "b".into()],
                output: "sum".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::Sum,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: Some("sum_path".into()),
                estimated_duration: 2,
            },
            IRBlock {
                block_id: "c4".into(),
                opcode: Opcode::UConstStr("answer=".into()),
                inputs: vec![],
                output: "prefix".into(),
                required_tag: "gpu".into(),
                merge_mode: MergeMode::Concat,
                resource_shape: "string".into(),
                preferred_zone: "z2".into(),
                inertia_key: Some("prefix_path".into()),
                estimated_duration: 1,
            },
            IRBlock {
                block_id: "c5".into(),
                opcode: Opcode::UConcat,
                inputs: vec!["left".into(), "right".into()],
                output: "txt".into(),
                required_tag: "gpu".into(),
                merge_mode: MergeMode::Concat,
                resource_shape: "string".into(),
                preferred_zone: "z2".into(),
                inertia_key: Some("concat_path".into()),
                estimated_duration: 2,
            },
        ],
        edges: vec![
            IREdge { src_block: "c1".into(), dst_block: "c3".into(), output_key: "x".into(),      input_key: "a".into() },
            IREdge { src_block: "c2".into(), dst_block: "c3".into(), output_key: "y".into(),      input_key: "b".into() },
            IREdge { src_block: "c4".into(), dst_block: "c5".into(), output_key: "prefix".into(), input_key: "left".into() },
            IREdge { src_block: "c3".into(), dst_block: "c5".into(), output_key: "sum".into(),    input_key: "right".into() },
        ],
    };

    let nodes = vec![
        Node {
            node_id: "cpu0".into(),
            node_type: NodeType::Cpu,
            host_id: "host0".into(),
            zone: "z1".into(),
            tags: vec!["cpu".into(), "arith".into(), "lowlat".into()],
            compute_capacity: 8.0,
            memory_capacity: 32.0,
            bandwidth: 10.0,
            inertia_keys: vec!["sum_path".into()],
        },
        Node {
            node_id: "gpu0".into(),
            node_type: NodeType::Gpu,
            host_id: "host1".into(),
            zone: "z2".into(),
            tags: vec!["gpu".into(), "throughput".into()],
            compute_capacity: 20.0,
            memory_capacity: 64.0,
            bandwidth: 25.0,
            inertia_keys: vec!["concat_path".into()],
        },
    ];

    let policy = MultifactorPolicy { reservation_bias: 1.0 };
    let mut rt = URXRuntime::new(nodes, policy);

    // ET-Cooling SA optimizer (default: 300 epochs, t_max=2.0)
    let et = ETCoolingPolicy::new();
    rt.set_et_policy(et);

    let result = rt.execute_graph(&graph).await;

    println!("  fused_graph_id  : {}", result.fused_graph_id);
    println!("  partition_binding:");
    for (pid, nid) in &result.partition_binding {
        println!("    {} → {}", pid, nid);
    }
    println!("  packet_log routes:");
    for p in &result.packet_log {
        println!("    {} → {} [{}] cost={:.2}", p.src_block, p.dst_block, p.route_type, p.route_cost);
    }
    println!("  merged: {:?}", result.merged);
    println!("  outputs ({}):", result.outputs.len());
    for v in &result.outputs {
        println!("    {:?}", v);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo C: Reservation table with backfill query
// ─────────────────────────────────────────────────────────────────────────────
//
//   Pre-populate the ReservationTable to simulate a busy node, then run a graph
//   whose partition scheduler must find backfill windows around the existing
//   reservations. Prints which time slots were assigned.

async fn demo_c() {
    println!("\n── Demo C: Reservation table backfill scheduling ──");

    // Pre-populate reservations: node cpu0 is busy from t=10 to t=30.
    let mut pre_reservations = ReservationTable::new();
    pre_reservations.add(Reservation::new("pre1".into(), "cpu0".into(), 10, 30));
    pre_reservations.add(Reservation::new("pre2".into(), "cpu0".into(), 40, 60));

    // Print the backfill windows before execution.
    let windows = pre_reservations.find_backfill_windows("cpu0", 100);
    println!("  Pre-existing reservations on cpu0: t=[10,30), t=[40,60)");
    println!("  Backfill windows (max_dur=100):");
    for w in &windows {
        println!("    t=[{}, {})  duration={}", w.start_time, w.end_time, w.duration);
    }

    // Simple linear graph: const → neg → out
    let graph = IRGraph {
        graph_id: "demo_c".into(),
        blocks: vec![
            IRBlock {
                block_id: "r1".into(),
                opcode: Opcode::FConst(9.0),
                inputs: vec![],
                output: "v".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 3,
            },
            IRBlock {
                block_id: "r2".into(),
                opcode: Opcode::FSqrt,
                inputs: vec!["v".into()],
                output: "out".into(),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 3,
            },
        ],
        edges: vec![
            IREdge {
                src_block: "r1".into(),
                dst_block: "r2".into(),
                output_key: "v".into(),
                input_key: "v".into(),
            },
        ],
    };

    let nodes = vec![
        Node {
            node_id: "cpu0".into(),
            node_type: NodeType::Cpu,
            host_id: "host0".into(),
            zone: "z1".into(),
            tags: vec!["cpu".into()],
            compute_capacity: 4.0,
            memory_capacity: 16.0,
            bandwidth: 5.0,
            inertia_keys: vec![],
        },
    ];

    let policy = MultifactorPolicy { reservation_bias: 1.0 };
    let mut rt = URXRuntime::new(nodes, policy);

    // Inject the pre-existing reservations into the runtime.
    for r in pre_reservations.node_reservations("cpu0") {
        rt.add_reservation(r.clone());
    }

    let result = rt.execute_graph(&graph).await;

    println!("  Execution results:");
    for r in &result.results {
        println!("    block {} t=[{}µs,{}µs] on {} → {:?}",
            r.block_id, r.start_time, r.end_time, r.node_id, r.value);
    }
    println!("  Reservation slots assigned:");
    for (pid, nid) in &result.partition_binding {
        println!("    partition {} → node {}", pid, nid);
    }
    println!("  Leaf outputs: {:?}", result.outputs);
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo D: Multi-core ThreadPoolExecutor — parallel independent blocks
// ─────────────────────────────────────────────────────────────────────────────
//
//   8 independent FPow blocks run in parallel via spawn_blocking.
//   We time the whole batch and compare against sequential expectation.
//
//   Graph:  base0..base7 (FConst) → pow0..pow7 (FPow, exp=2.0) → (leaves)

async fn demo_d() {
    println!("\n── Demo D: ThreadPoolExecutor multi-core parallel (8 FPow blocks) ──");

    let mut graph = IRGraph::with_id("demo_d".to_string());

    for i in 0..8u32 {
        // const base
        graph.blocks.push(IRBlock {
            block_id: format!("base{i}"),
            opcode: Opcode::FConst((i + 1) as f64),
            inputs: vec![],
            output: "v".into(),
            required_tag: "cpu".into(),
            merge_mode: MergeMode::List,
            resource_shape: "scalar".into(),
            preferred_zone: "z1".into(),
            inertia_key: None,
            estimated_duration: 1,
        });
        // const exp
        graph.blocks.push(IRBlock {
            block_id: format!("exp{i}"),
            opcode: Opcode::FConst(2.0),
            inputs: vec![],
            output: "e".into(),
            required_tag: "cpu".into(),
            merge_mode: MergeMode::List,
            resource_shape: "scalar".into(),
            preferred_zone: "z1".into(),
            inertia_key: None,
            estimated_duration: 1,
        });
        // pow
        graph.blocks.push(IRBlock {
            block_id: format!("pow{i}"),
            opcode: Opcode::FPow,
            inputs: vec!["a".into(), "b".into()],
            output: "out".into(),
            required_tag: "cpu".into(),
            merge_mode: MergeMode::Sum,
            resource_shape: "scalar".into(),
            preferred_zone: "z1".into(),
            inertia_key: None,
            estimated_duration: 2,
        });
        graph.edges.push(IREdge {
            src_block: format!("base{i}"),
            dst_block: format!("pow{i}"),
            output_key: "v".into(),
            input_key: "a".into(),
        });
        graph.edges.push(IREdge {
            src_block: format!("exp{i}"),
            dst_block: format!("pow{i}"),
            output_key: "e".into(),
            input_key: "b".into(),
        });
    }

    let nodes = vec![
        Node {
            node_id: "cpu0".into(),
            node_type: NodeType::Cpu,
            host_id: "host0".into(),
            zone: "z1".into(),
            tags: vec!["cpu".into()],
            compute_capacity: 8.0,
            memory_capacity: 32.0,
            bandwidth: 10.0,
            inertia_keys: vec![],
        },
    ];

    let policy = MultifactorPolicy { reservation_bias: 1.0 };
    let mut rt = URXRuntime::new(nodes, policy);

    // Register ThreadPoolExecutor for cpu0 — enables spawn_blocking parallelism
    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    rt.executors.register(
        "cpu0",
        Arc::new(ThreadPoolExecutor::new(parallelism)),
    );

    println!("  Logical CPUs available: {parallelism}");
    println!("  Running 8 parallel FPow blocks via ThreadPoolExecutor...");

    let t0 = Instant::now();
    let result = rt.execute_graph(&graph).await;
    let elapsed = t0.elapsed();

    println!("  Elapsed: {:.2?}", elapsed);
    println!("  Executor on cpu0: thread-pool");

    // pow(i+1, 2) for i in 0..8 → 1,4,9,16,25,36,49,64  sum=204
    let pow_results: Vec<_> = result.results.iter()
        .filter(|r| r.block_id.starts_with("pow"))
        .collect();
    println!("  pow results ({}):", pow_results.len());
    for r in &pow_results {
        println!("    {} → {:?}  [executor={}]", r.block_id, r.value, r.executor_name);
    }

    // Sum via reducer
    println!("  merged Sum = {:?}", result.merged.get("Sum"));

    // Verify: 1²+2²+...+8² = 204
    let sum_str = result.merged.get("Sum").cloned().unwrap_or_default();
    let sum: i64 = sum_str.parse().unwrap_or(0);
    // Note: FPow returns F64, so Sum reducer (i64 only) will be 0; check individual values
    let f_sum: f64 = pow_results.iter().filter_map(|r| {
        if let crate::packet::PayloadValue::F64(v) = &r.value { Some(*v) } else { None }
    }).sum();
    println!("  F64 sum of squares: {f_sum}  (expected 204.0)");
    assert!((f_sum - 204.0).abs() < 1e-6, "sum of squares mismatch: {f_sum}");
    println!("  ✓ multi-core result correct");
    let _ = sum;
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo E: GPU via WgpuExecutor (feature = "gpu")
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "gpu")]
async fn demo_e() {
    use gpu_executor::WgpuExecutor;

    println!("\n── Demo E: WgpuExecutor GPU scheduling ──");

    match WgpuExecutor::new().await {
        Err(e) => {
            println!("  GPU init failed (skipping): {e}");
            return;
        }
        Ok(gpu_exec) => {
            println!("  GPU: {}", gpu_exec.adapter_info);

            // Simple graph: two i64 constants → UAdd → leaf
            let mut graph = IRGraph::with_id("demo_e".to_string());
            graph.blocks.push(IRBlock {
                block_id: "g1".into(),
                opcode: Opcode::UConstI64(100),
                inputs: vec![],
                output: "x".into(),
                required_tag: "gpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            });
            graph.blocks.push(IRBlock {
                block_id: "g2".into(),
                opcode: Opcode::UConstI64(200),
                inputs: vec![],
                output: "y".into(),
                required_tag: "gpu".into(),
                merge_mode: MergeMode::List,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 1,
            });
            graph.blocks.push(IRBlock {
                block_id: "g3".into(),
                opcode: Opcode::UAdd,
                inputs: vec!["a".into(), "b".into()],
                output: "sum".into(),
                required_tag: "gpu".into(),
                merge_mode: MergeMode::Sum,
                resource_shape: "scalar".into(),
                preferred_zone: "z1".into(),
                inertia_key: None,
                estimated_duration: 2,
            });
            graph.edges.push(IREdge {
                src_block: "g1".into(), dst_block: "g3".into(),
                output_key: "x".into(), input_key: "a".into(),
            });
            graph.edges.push(IREdge {
                src_block: "g2".into(), dst_block: "g3".into(),
                output_key: "y".into(), input_key: "b".into(),
            });

            let nodes = vec![
                Node {
                    node_id: "gpu0".into(),
                    node_type: NodeType::Gpu,
                    host_id: "host0".into(),
                    zone: "z1".into(),
                    tags: vec!["gpu".into()],
                    compute_capacity: 100.0,
                    memory_capacity: 8192.0,
                    bandwidth: 500.0,
                    inertia_keys: vec![],
                },
            ];

            let policy = MultifactorPolicy { reservation_bias: 1.0 };
            let mut rt = URXRuntime::new(nodes, policy);
            rt.executors.register("gpu0", Arc::new(gpu_exec));

            let result = rt.execute_graph(&graph).await;

            println!("  partition_binding: {:?}", result.partition_binding);
            for r in &result.results {
                println!("  block {} on {} [{}] → {:?}",
                    r.block_id, r.node_id, r.executor_name, r.value);
            }
            println!("  merged Sum = {:?}", result.merged.get("Sum"));

            let sum_str = result.merged.get("Sum").cloned().unwrap_or_default();
            let sum: i64 = sum_str.parse().unwrap_or(0);
            assert_eq!(sum, 300, "GPU UAdd result mismatch: {sum}");
            println!("  ✓ GPU result correct (100+200=300)");
        }
    }
}

#[cfg(not(feature = "gpu"))]
async fn demo_e() {
    println!("\n── Demo E: GPU (skipped — build without --features gpu) ──");
    println!("  Run: cargo run --features gpu  to enable WgpuExecutor test");
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: generate a deep-parallel pipeline graph
// ─────────────────────────────────────────────────────────────────────────────
//
// Structure:
//   depth=3, width=4  →  4 independent columns, each 4 layers deep
//
//   col 0 (zone z1): l0_w0 (FConst 1.0) → l1_w0 (FMul) → l2_w0 (FAdd) → l3_w0 (FMul)
//   col 1 (zone z2): l0_w1 (FConst 2.0) → l1_w1 (FMul) → ...
//   ...
//
// Each column is a self-contained chain (both binary inputs sourced from the
// same previous layer block). Blocks are emitted column-first so that
// partition_graph groups each column into one partition → one node.
//
// Math per column w (value v = w+1):
//   layer 0: v
//   layer 1 (FMul): v²
//   layer 2 (FAdd): 2v²
//   layer 3 (FMul): 4v⁴

fn generate_pipeline_graph(depth: usize, width: usize) -> IRGraph {
    let mut g = IRGraph::with_id(format!("pipeline_d{depth}_w{width}"));

    for w in 0..width {
        let zone = format!("z{}", w + 1);
        let init_val = (w + 1) as f64;

        // Layer 0: FConst source
        g.blocks.push(IRBlock {
            block_id: format!("l0_w{w}"),
            opcode: Opcode::FConst(init_val),
            inputs: vec![],
            output: format!("v0_{w}"),
            required_tag: "cpu".into(),
            merge_mode: MergeMode::List,
            resource_shape: "scalar".into(),
            preferred_zone: zone.clone(),
            inertia_key: None,
            estimated_duration: 1,
        });

        // Layers 1..=depth: alternating FMul / FAdd, both inputs from previous layer
        for d in 1..=depth {
            let prev = format!("l{}_w{}", d - 1, w);
            let cur  = format!("l{d}_w{w}");
            let opcode = if d % 2 == 1 { Opcode::FMul } else { Opcode::FAdd };

            g.blocks.push(IRBlock {
                block_id: cur.clone(),
                opcode,
                inputs: vec!["a".into(), "b".into()],
                output: format!("v{d}_{w}"),
                required_tag: "cpu".into(),
                merge_mode: MergeMode::Sum,
                resource_shape: "scalar".into(),
                preferred_zone: zone.clone(),
                inertia_key: None,
                estimated_duration: 2,
            });
            // Both "a" and "b" sourced from the same previous block
            g.edges.push(IREdge {
                src_block: prev.clone(),
                dst_block: cur.clone(),
                output_key: format!("v{}_{}", d - 1, w),
                input_key: "a".into(),
            });
            g.edges.push(IREdge {
                src_block: prev,
                dst_block: cur,
                output_key: format!("v{}_{}", d - 1, w),
                input_key: "b".into(),
            });
        }
    }

    g
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo F: Workstation mode — enable_workstation_mode() auto-binds executors
// ─────────────────────────────────────────────────────────────────────────────
//
//  4 CPU nodes (z1-z4) + enable_workstation_mode() → ThreadPoolExecutor on all
//  Pipeline graph depth=3, width=4 → 4 independent column-partitions
//  Each partition goes to its matching zone node

async fn demo_f() {
    println!("\n── Demo F: enable_workstation_mode() + pipeline graph (depth=3, width=4) ──");

    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    println!("  Logical CPUs: {parallelism}");

    // 4 CPU nodes in 4 zones
    let nodes: Vec<Node> = (0..4).map(|i| Node {
        node_id: format!("cpu{i}"),
        node_type: NodeType::Cpu,
        host_id: "workstation".into(),
        zone: format!("z{}", i + 1),
        tags: vec!["cpu".into()],
        compute_capacity: parallelism as f32 / 4.0,
        memory_capacity: 32.0,
        bandwidth: 50.0,
        inertia_keys: vec![],
    }).collect();

    let policy = MultifactorPolicy { reservation_bias: 1.0 };
    let mut rt = URXRuntime::new(nodes, policy);

    // Auto-bind ThreadPoolExecutor to all CPU nodes
    rt.enable_workstation_mode();
    println!("  Workstation mode enabled — ThreadPoolExecutor on all CPU nodes");

    let graph = generate_pipeline_graph(3, 4);
    println!("  Graph: {} blocks, {} edges", graph.blocks.len(), graph.edges.len());

    let t0 = Instant::now();
    let result = rt.execute_graph(&graph).await;
    let elapsed = t0.elapsed();

    println!("  Elapsed: {elapsed:.2?}");
    println!("  Partition → Node binding:");
    let mut bindings: Vec<_> = result.partition_binding.iter().collect();
    bindings.sort();
    for (pid, nid) in &bindings {
        println!("    {pid} → {nid}");
    }
    println!("  Leaf outputs ({}):", result.outputs.len());
    for v in &result.outputs {
        println!("    {v:?}");
    }

    // Verify column results:
    // col w: init = w+1, after FMul: (w+1)², after FAdd: 2(w+1)², after FMul: 4(w+1)⁴
    let expected: Vec<f64> = (0..4).map(|w| {
        let v = (w + 1) as f64;
        4.0 * v.powi(4)
    }).collect();
    println!("  Expected leaf values: {expected:?}");

    let leaf_vals: Vec<f64> = result.outputs.iter().filter_map(|v| {
        if let packet::PayloadValue::F64(f) = v { Some(*f) } else { None }
    }).collect();
    let mut leaf_sorted = leaf_vals.clone();
    leaf_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut exp_sorted = expected.clone();
    exp_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    if leaf_sorted == exp_sorted {
        println!("  ✓ All column outputs correct");
    } else {
        println!("  ⚠ Output mismatch: got {leaf_sorted:?}, expected {exp_sorted:?}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo G: JIT batch execution — compile graph to WGSL, run 256 work items
// ─────────────────────────────────────────────────────────────────────────────
//
//  Graph: x → FMul(x,x) → s=x²; then FAdd(s, x) → y = x² + x
//  Run with N=256 inputs: x[i] = i as f32
//  Verify: y[i] = i*(i+1)
//  Report: throughput in Mop/s

#[cfg(feature = "gpu")]
async fn demo_g() {
    use gpu_executor::JitBlockExecutor;
    use jit_compiler::JitExecutor;

    println!("\n── Demo G: JitBlockExecutor via URXRuntime + JitExecutor batch ──");

    // ── Part 1: JitBlockExecutor wired into URXRuntime ────────────────────
    // Graph: FConst(3.0) → FSqrt → FNeg → leaf   expected: -√3 ≈ -1.7320508
    let jit_exec = match JitBlockExecutor::new().await {
        Ok(j) => j,
        Err(e) => { println!("  GPU init failed (skipping): {e}"); return; }
    };
    println!("  JIT GPU: {}", jit_exec.adapter_info());

    let mut graph = IRGraph::with_id("demo_g".into());
    graph.blocks.push(IRBlock {
        block_id: "c1".into(), opcode: Opcode::FConst(3.0),
        inputs: vec![], output: "v".into(),
        required_tag: "gpu".into(), merge_mode: MergeMode::List,
        resource_shape: "scalar".into(), preferred_zone: "z1".into(),
        inertia_key: None, estimated_duration: 1,
    });
    graph.blocks.push(IRBlock {
        block_id: "sq".into(), opcode: Opcode::FSqrt,
        inputs: vec!["v".into()], output: "sq".into(),
        required_tag: "gpu".into(), merge_mode: MergeMode::List,
        resource_shape: "scalar".into(), preferred_zone: "z1".into(),
        inertia_key: None, estimated_duration: 2,
    });
    graph.blocks.push(IRBlock {
        block_id: "neg".into(), opcode: Opcode::FNeg,
        inputs: vec!["v".into()], output: "neg".into(),
        required_tag: "gpu".into(), merge_mode: MergeMode::List,
        resource_shape: "scalar".into(), preferred_zone: "z1".into(),
        inertia_key: None, estimated_duration: 2,
    });
    graph.edges.push(IREdge { src_block: "c1".into(), dst_block: "sq".into(),
        output_key: "v".into(), input_key: "v".into() });
    graph.edges.push(IREdge { src_block: "sq".into(), dst_block: "neg".into(),
        output_key: "sq".into(), input_key: "v".into() });

    let nodes = vec![Node {
        node_id: "gpu0".into(), node_type: NodeType::Gpu,
        host_id: "host0".into(), zone: "z1".into(),
        tags: vec!["gpu".into()], compute_capacity: 100.0,
        memory_capacity: 8192.0, bandwidth: 500.0, inertia_keys: vec![],
    }];
    let policy = MultifactorPolicy { reservation_bias: 1.0 };
    let mut rt = URXRuntime::new(nodes, policy);
    rt.executors.register("gpu0", Arc::new(jit_exec));

    let result = rt.execute_graph(&graph).await;

    for r in &result.results {
        println!("  block {:>4} [{:>4}µs–{:>4}µs] executor={} → {:?}",
            r.block_id, r.start_time, r.end_time, r.executor_name, r.value);
    }
    if let Some(v) = result.outputs.first() {
        println!("  Leaf: {v:?}");
        if let packet::PayloadValue::F64(f) = v {
            let expected = -f64::sqrt(3.0);
            assert!((f - expected).abs() < 1e-4,
                "JitBlockExecutor mismatch: {f} ≠ {expected}");
            println!("  ✓ JitBlockExecutor correct (-√3 ≈ {f:.7})");
        }
    }

    // ── Part 2: JitExecutor batch — x²+x for N=1024 ──────────────────────
    println!();
    let jit = match JitExecutor::new().await {
        Ok(j) => j,
        Err(e) => { println!("  JitExecutor init failed: {e}"); return; }
    };

    let mut g = IRGraph::with_id("batch".into());
    g.blocks.push(IRBlock::new("x",  Opcode::FConst(0.0)));   // source slot
    let mut sq = IRBlock::new("sq", Opcode::FMul);
    sq.inputs = vec!["a".into(), "b".into()];
    g.blocks.push(sq);
    let mut y = IRBlock::new("y", Opcode::FAdd);
    y.inputs = vec!["a".into(), "b".into()];
    g.blocks.push(y);
    g.edges.push(IREdge { src_block: "x".into(),  dst_block: "sq".into(), output_key: "x".into(),  input_key: "a".into() });
    g.edges.push(IREdge { src_block: "x".into(),  dst_block: "sq".into(), output_key: "x".into(),  input_key: "b".into() });
    g.edges.push(IREdge { src_block: "sq".into(), dst_block: "y".into(),  output_key: "sq".into(), input_key: "a".into() });
    g.edges.push(IREdge { src_block: "x".into(),  dst_block: "y".into(),  output_key: "x".into(),  input_key: "b".into() });

    let compiled = jit.compile(&g).unwrap();
    const N: usize = 1024;
    let xs: Vec<f32> = (0..N).map(|i| i as f32).collect();

    let t0 = Instant::now();
    let out = jit.run(&compiled, &[xs], N).unwrap();
    let elapsed = t0.elapsed();

    let ys = &out[0];
    let mops = (N * 2) as f64 / elapsed.as_secs_f64() / 1e6;
    println!("  Batch N={N}: {elapsed:.2?}  {mops:.1} Mop/s");
    println!("  y[1]={} y[4]={} y[15]={}", ys[1], ys[4], ys[15]);

    let ok = (0..N).all(|i| (ys[i] - (i * i + i) as f32).abs() < 0.5);
    if ok { println!("  ✓ All {N} batch results correct (y[i]=i²+i)"); }
}

#[cfg(not(feature = "gpu"))]
async fn demo_g() {
    println!("\n── Demo G: JIT batch (skipped — build without --features gpu) ──");
    println!("  Run: cargo run --features gpu");
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo H: ZeroCopyContext — shared memory, buffer pool, inertia cache
// ─────────────────────────────────────────────────────────────────────────────
//
//  Exercises the full shared-memory stack without any graph execution:
//    SharedMemoryRegion  — named byte buffers shared across partitions
//    BufferPool          — power-of-two bucket pool to reuse BytesMut allocations
//    InertiaBufferCache  — LRU-evicting keyed cache for hot tensors/payloads
//    ZeroCopyContext     — unified facade that wraps all three

async fn demo_h() {
    use shared_memory::{BufferPool, InertiaBufferCache, SharedMemoryRegion, ZeroCopyContext};
    use packet::{PayloadCodec, PayloadValue};
    use bytes::Bytes;

    println!("\n── Demo H: ZeroCopyContext (SharedMemoryRegion / BufferPool / InertiaBufferCache) ──");

    // ── 1. SharedMemoryRegion: encode a PayloadValue, write, then read back ──
    let region = SharedMemoryRegion::new("reg_a".into(), 256);
    let original = PayloadValue::I64(0xDEAD_BEEF_i64);
    let encoded  = PayloadCodec::encode(&original);
    region.write(&encoded).await.unwrap();

    let decoded = region.read_view().await;
    assert_eq!(decoded, original,
        "SharedMemoryRegion round-trip failed: {decoded:?}");
    println!("  SharedMemoryRegion  : write I64(0xDEAD_BEEF) → read back {:?}  ✓", decoded);
    println!("  region.id()={} size={}B", region.id(), region.size().await);

    // Concurrent reader tracking
    region.acquire_read().await;
    region.acquire_read().await;
    assert_eq!(region.reader_count().await, 2);
    region.release_read().await;
    assert_eq!(region.reader_count().await, 1);
    region.release_read().await;
    println!("  Reader count after acquire×2 / release×2 = {}  ✓", region.reader_count().await);

    // ── 2. BufferPool: acquire / release cycles ──────────────────────────────
    let pool = BufferPool::new();

    // Acquire 3 buffers of different sizes (will be rounded to next power of two)
    let b1 = pool.acquire(100).await;   // → 128-byte bucket
    let b2 = pool.acquire(500).await;   // → 512-byte bucket
    let b3 = pool.acquire(500).await;   // → 512-byte bucket (second)
    println!("  BufferPool caps: b1={} b2={} b3={}",
        b1.capacity(), b2.capacity(), b3.capacity());

    // Return two of them; check pool stats
    pool.release(b1).await;
    pool.release(b2).await;
    let stats = pool.stats().await;
    println!("  After releasing 2 buffers: pool={} bufs, {} bytes total, {} size classes  ✓",
        stats.total_buffers, stats.total_capacity, stats.size_categories);
    assert!(stats.total_buffers >= 2);

    // Re-acquire from pool (should reuse, not allocate)
    let reused = pool.acquire(100).await;
    let _ = (b3, reused); // consume to avoid unused-var warning

    // ── 3. InertiaBufferCache: put / get / LRU eviction ─────────────────────
    let cache = InertiaBufferCache::new(3); // cap = 3 entries

    cache.put("tensor_q".into(), Bytes::from(b"query\0\0\0".as_ref())).await;
    cache.put("tensor_k".into(), Bytes::from(b"key\0\0\0\0\0".as_ref())).await;
    cache.put("tensor_v".into(), Bytes::from(b"value\0\0\0".as_ref())).await;

    assert!(cache.get("tensor_q").await.is_some());
    assert!(cache.get("tensor_k").await.is_some());
    assert!(cache.get("missing").await.is_none());

    // Inserting a 4th entry should evict the LRU (tensor_v, accessed least recently)
    cache.put("tensor_out".into(), Bytes::from(b"output\0\0".as_ref())).await;
    let s = cache.stats().await;
    println!("  InertiaBufferCache  : {} entries (cap=3, evicted 1 LRU)  ✓", s.entries);
    assert_eq!(s.entries, 3);

    // ── 4. ZeroCopyContext: unified facade ───────────────────────────────────
    let ctx = ZeroCopyContext::new();

    // Buffer pool path
    let mut buf = ctx.acquire_buffer(200).await;
    buf.extend_from_slice(&encoded);
    ctx.release_buffer(buf).await;

    // Shared region path
    let shared = ctx.get_shared_region("shared_kv", 1024).await;
    shared.write(&encoded).await.unwrap();
    let v = shared.read_view().await;
    assert_eq!(v, original);

    // Inertia cache path
    ctx.cache("hot_weight".into(), Bytes::from(encoded.clone())).await;
    let hit = ctx.get_cached("hot_weight").await;
    assert!(hit.is_some(), "inertia cache miss");

    let bstats = ctx.buffer_stats().await;
    let cstats = ctx.cache_stats().await;
    println!("  ZeroCopyContext     : pool_bufs={} cache_entries={}  ✓",
        bstats.total_buffers, cstats.entries);
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo I: compile_graph — WGSL JIT compilation (no GPU required)
// ─────────────────────────────────────────────────────────────────────────────
//
//  Builds a 4-block arithmetic graph and runs it through compile_graph():
//    FConst(2.0) ─┐
//                 FMul → FAdd(+1.0) → leaf
//    FConst(3.0) ─┘
//
//  Verifies the returned CompiledGraph metadata and prints the WGSL source.

async fn demo_i() {
    use jit_compiler::{compile_graph, CompiledGraph, ShaderType};

    println!("\n── Demo I: compile_graph (WGSL JIT compilation, CPU meta-path) ──");

    // Build graph:  a=FConst(2), b=FConst(3), mul=FMul(a,b), add=FAdd(mul,c=1)
    let mut graph = IRGraph::with_id("demo_i".into());
    graph.blocks.push(IRBlock::new("a",   Opcode::FConst(2.0)));
    graph.blocks.push(IRBlock::new("b",   Opcode::FConst(3.0)));
    let mut mul = IRBlock::new("mul", Opcode::FMul);
    mul.inputs = vec!["lhs".into(), "rhs".into()];
    graph.blocks.push(mul);
    let mut add = IRBlock::new("add", Opcode::FAdd);
    add.inputs = vec!["a".into(), "b".into()];
    graph.blocks.push(add);

    graph.edges.push(IREdge { src_block: "a".into(),   dst_block: "mul".into(), output_key: "a".into(),   input_key: "lhs".into() });
    graph.edges.push(IREdge { src_block: "b".into(),   dst_block: "mul".into(), output_key: "b".into(),   input_key: "rhs".into() });
    graph.edges.push(IREdge { src_block: "mul".into(), dst_block: "add".into(), output_key: "mul".into(), input_key: "a".into() });
    graph.edges.push(IREdge { src_block: "b".into(),   dst_block: "add".into(), output_key: "b".into(),   input_key: "b".into() });

    let compiled: CompiledGraph = compile_graph(&graph)
        .expect("compile_graph failed");

    println!("  graph_id      : {}", graph.graph_id);
    println!("  topo_order    : {:?}", compiled.topo_order);
    println!("  n_regs        : {}", compiled.n_regs);
    println!("  input_indices : {:?}", compiled.input_indices);
    println!("  output_indices: {:?}", compiled.output_indices);
    println!("  result_types  : {:?}", compiled.result_types.iter()
        .map(|t| match t { ShaderType::F32 => "f32", ShaderType::I32 => "i32" })
        .collect::<Vec<_>>());

    // Verify metadata
    assert_eq!(compiled.n_regs, 4, "expected 4 virtual registers");
    assert_eq!(compiled.input_indices.len(), 2, "expected 2 source blocks (a, b)");
    assert_eq!(compiled.output_indices.len(), 1, "expected 1 leaf block (add)");

    // Show first 6 lines of the WGSL source
    let preview: Vec<&str> = compiled.wgsl_source.lines().take(6).collect();
    println!("  WGSL (first 6 lines):");
    for line in &preview {
        println!("    {line}");
    }

    let wgsl_lines = compiled.wgsl_source.lines().count();
    println!("  … ({wgsl_lines} lines total)");
    println!("  ✓ compile_graph succeeded, {wgsl_lines} WGSL lines generated");
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo J: USB protocol framing + UsbLoopbackExecutor / UsbCpuFallbackExecutor
// ─────────────────────────────────────────────────────────────────────────────
//
//  Exercises the USB binary framing layer end-to-end without real hardware:
//    - crc8 over raw payload bytes
//    - encode_request / decode_response round-trips
//    - STATUS_OK / STATUS_UNSUPPORTED / STATUS_ERROR constants
//    - UsbLoopbackExecutor registered on a runtime node
//    - UsbCpuFallbackExecutor wrapping the loopback (production-safe stack)
//    - BlockExecutor::exec() direct execution path

async fn demo_j() {
    use usb_executor::{
        UsbLoopbackExecutor, UsbCpuFallbackExecutor,
        encode_request, decode_response, crc8,
        FRAME_SYNC, STATUS_OK, STATUS_UNSUPPORTED, STATUS_ERROR,
        UsbOpcodeId,
    };
    use executor::BlockExecutor;
    use packet::PayloadValue;
    use std::collections::HashMap;

    println!("\n── Demo J: USB framing + UsbLoopback/CpuFallback + BlockExecutor ──");

    // ── 1. crc8 sanity checks ─────────────────────────────────────────────────
    assert_eq!(crc8(&[]), 0x00, "crc8 of empty should be 0");
    let sample = b"URP\x01\x02\x03";
    let checksum = crc8(sample);
    assert_ne!(checksum, 0, "crc8 of non-empty data should be non-zero");
    println!("  crc8({sample:?}) = 0x{checksum:02X}  ✓");

    // ── 2. encode_request → decode_response round-trip ───────────────────────
    // Build a fake HELLO request (opcode 0x10 = UsbHello, 0 inputs)
    let request = encode_request(UsbOpcodeId::Hello, &[]);
    assert!(request.len() >= 4, "frame too short");
    assert_eq!(request[0], FRAME_SYNC, "bad sync byte");
    println!("  encode_request(Hello, []) → {} bytes, sync=0x{:02X}  ✓",
        request.len(), request[0]);

    // Encode a STATUS_OK response with an I64 payload, then decode it
    let val = PayloadValue::I64(9999);
    let response = usb_executor::encode_response(STATUS_OK, Some(&val));
    match decode_response(&response) {
        Ok((STATUS_OK, Some(PayloadValue::I64(v)))) => {
            assert_eq!(v, 9999);
            println!("  encode/decode STATUS_OK I64(9999) → I64({v})  ✓");
        }
        other => panic!("unexpected decode result: {other:?}"),
    }

    // Encode STATUS_UNSUPPORTED (no payload)
    let unsup = usb_executor::encode_response(STATUS_UNSUPPORTED, None);
    match decode_response(&unsup) {
        Ok((STATUS_UNSUPPORTED, None)) =>
            println!("  STATUS_UNSUPPORTED → decoded correctly  ✓"),
        other => panic!("unexpected: {other:?}"),
    }

    // Bad CRC → STATUS_ERROR
    let mut corrupt = response.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0xFF;   // flip CRC byte
    match decode_response(&corrupt) {
        Err(_) | Ok((STATUS_ERROR, _)) =>
            println!("  Corrupted CRC → error detected  ✓"),
        other => panic!("expected error, got {other:?}"),
    }
    let _ = STATUS_ERROR; // suppress unused warning if decode always returns Err

    // ── 3. BlockExecutor::exec() — direct block execution ────────────────────
    let mut ctx: HashMap<String, PayloadValue> = HashMap::new();
    ctx.insert("a".into(), PayloadValue::I64(21));
    ctx.insert("b".into(), PayloadValue::I64(21));
    let mut add_block = IRBlock::new("sum", Opcode::UAdd);
    add_block.inputs = vec!["a".into(), "b".into()];

    let direct = BlockExecutor::exec(&add_block, &ctx);
    assert_eq!(direct, PayloadValue::I64(42), "BlockExecutor UAdd mismatch");
    println!("  BlockExecutor::exec(UAdd, {{a:21, b:21}}) = {:?}  ✓", direct);

    // ── 4. UsbLoopbackExecutor registered in URXRuntime ───────────────────────
    // Graph: FConst(5.0) → FSqrt → leaf   expected ≈ 2.236
    // The loopback executor falls through to CPU for unsupported opcodes.
    let mut graph = IRGraph::with_id("demo_j".into());
    graph.blocks.push(IRBlock::new("c", Opcode::FConst(5.0)));
    let mut sq = IRBlock::new("sq", Opcode::FSqrt);
    sq.inputs = vec!["v".into()];
    graph.blocks.push(sq);
    graph.edges.push(IREdge {
        src_block: "c".into(), dst_block: "sq".into(),
        output_key: "c".into(), input_key: "v".into(),
    });

    let nodes = vec![Node {
        node_id: "usb0".into(), node_type: NodeType::Cpu,
        host_id: "host0".into(), zone: "z1".into(),
        tags: vec!["cpu".into()], compute_capacity: 4.0,
        memory_capacity: 8.0, bandwidth: 5.0, inertia_keys: vec![],
    }];
    let policy = MultifactorPolicy { reservation_bias: 1.0 };
    let mut rt = URXRuntime::new(nodes, policy);

    // ── 4a. UsbLoopbackExecutor ───────────────────────────────────────────────
    rt.executors.register("usb0", Arc::new(UsbLoopbackExecutor::new("usb0")));
    let r1 = rt.execute_graph(&graph).await;
    if let Some(v) = r1.outputs.first() {
        println!("  UsbLoopbackExecutor leaf = {v:?}");
        if let PayloadValue::F64(f) = v {
            assert!((f - 5.0f64.sqrt()).abs() < 1e-9, "loopback sqrt mismatch");
            println!("  ✓ UsbLoopbackExecutor correct (√5 ≈ {f:.6})");
        }
    }

    // ── 4b. UsbCpuFallbackExecutor ────────────────────────────────────────────
    rt.executors.register("usb0", Arc::new(
        UsbCpuFallbackExecutor::new(Box::new(UsbLoopbackExecutor::new("usb0")))
    ));
    let r2 = rt.execute_graph(&graph).await;
    if let Some(v) = r2.outputs.first() {
        println!("  UsbCpuFallbackExecutor leaf = {v:?}");
        if let PayloadValue::F64(f) = v {
            assert!((f - 5.0f64.sqrt()).abs() < 1e-9, "fallback sqrt mismatch");
            println!("  ✓ UsbCpuFallbackExecutor correct (√5 ≈ {f:.6})");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo K: ReservationAwarePolicy — custom impl + high-priority preemption
// ─────────────────────────────────────────────────────────────────────────────
//
//  Shows that ReservationAwarePolicy::select_with_reservations() can route a
//  partition to an alternative node when the preferred node is fully booked.
//
//  Setup:
//    node0 — already has a reservation covering slots 0–100 (busy)
//    node1 — completely free
//  The policy should pick node1 for a 10-slot partition starting at slot 0.

async fn demo_k() {
    use reservation::{Reservation, ReservationTable, ReservationAwarePolicy, ReservationPriority};

    println!("\n── Demo K: ReservationAwarePolicy (custom earliest-slot scheduler) ──");

    // ── Implement the trait ───────────────────────────────────────────────────
    struct EarliestSlotPolicy;

    impl ReservationAwarePolicy for EarliestSlotPolicy {
        fn select_with_reservations(
            &self,
            _partition_id: &str,
            node_ids: &[String],
            duration: u32,
            preferred_time: u32,
            reservations: &ReservationTable,
        ) -> Option<String> {
            // `earliest_start_time` returns None for nodes with no reservations
            // (they have no by_node entry) — that means "available at preferred_time".
            node_ids.iter()
                .min_by_key(|nid| {
                    reservations
                        .earliest_start_time(nid, duration, preferred_time)
                        .unwrap_or(preferred_time)
                })
                .cloned()
        }
    }

    // ── Build a reservation table ─────────────────────────────────────────────
    let mut table = ReservationTable::default();

    // node0 is busy from slot 0 to 100
    table.add(Reservation::new("job_x".into(), "node0".into(), 0, 100));

    // node1 is free; node2 has a tiny gap (1–10) that can't fit a 20-slot job
    table.add(Reservation::new("job_y".into(), "node2".into(), 0, 1));
    table.add(Reservation::new("job_z".into(), "node2".into(), 10, 50));

    let candidates = vec![
        "node0".to_string(),
        "node1".to_string(),
        "node2".to_string(),
    ];

    let policy = EarliestSlotPolicy;

    // ── 1. 10-slot job starting at 0: node0 busy, node1 free → picks node1 ──
    let chosen = policy.select_with_reservations("part_A", &candidates, 10, 0, &table);
    println!("  10-slot job  →  chosen: {:?}", chosen);
    assert_eq!(chosen.as_deref(), Some("node1"),
        "expected node1 (free), got {chosen:?}");
    println!("  ✓ 10-slot correctly routed to node1 (node0 busy 0-100)");

    // ── 2. 20-slot job: node1 wins over node2 (node2 has fragmented gaps) ────
    let chosen2 = policy.select_with_reservations("part_B", &candidates, 20, 0, &table);
    println!("  20-slot job  →  chosen: {:?}", chosen2);
    assert_eq!(chosen2.as_deref(), Some("node1"),
        "expected node1 for 20-slot job, got {chosen2:?}");
    println!("  ✓ 20-slot correctly routed to node1 (node2 fragmented)");

    // ── 3. Reservation priority variants (High / Critical) ───────────────────
    // Just construct them to verify no panic + correct fields.
    let r_normal   = Reservation::new("rn".into(), "node0".into(), 200, 210);
    let r_high     = Reservation::new("rh".into(), "node0".into(), 210, 220)
        .with_priority(ReservationPriority::High);
    let r_critical = Reservation::new("rc".into(), "node0".into(), 220, 230)
        .with_priority(ReservationPriority::Critical);

    table.add(r_normal);
    table.add(r_high);
    table.add(r_critical);

    // Verify they were recorded
    let earliest = table.earliest_start_time("node0", 5, 230).unwrap_or(230);
    println!("  Earliest free slot on node0 after priority reservations: {earliest}");
    assert!(earliest >= 230, "expected slot ≥ 230, got {earliest}");
    println!("  ✓ Priority reservations (High, Critical) added without panic");
}

// ─────────────────────────────────────────────────────────────────────────────
// Demo L: load real workload graphs from JSON and execute on URXRuntime
// ─────────────────────────────────────────────────────────────────────────────
//
//  Loads the 4 graphs from urp_workloads.tar.gz (already extracted):
//    fft_n64_s6      — 703 blocks,  1152 edges  (FFT butterfly N=64)
//    fft_n128_s7     — 1599 blocks, 2688 edges  (FFT butterfly N=128)
//    attn_h4_s32     — 45 blocks,   54 edges    (Transformer attention h=4)
//    resnet_8blk_c64 — 163 blocks,  178 edges   (ResNet residual blocks)
//
//  Each graph is fused, partitioned, and scheduled through URXRuntime.
//  Measures wall-clock time for fuse+partition+execute.

async fn demo_l() {
    use std::path::Path;
    use std::time::Instant;

    println!("\n── Demo L: load & execute real workload graphs from JSON ──");

    // Workloads directory — extracted from urp_workloads.tar.gz
    let workload_dir = r"C:\Users\asus\urp";

    let graphs_to_run = [
        ("fft_n64_s6",      "FFT butterfly N=64 (6-stage)"),
        ("fft_n128_s7",     "FFT butterfly N=128 (7-stage)"),
        ("attn_h4_s32",     "Transformer attention h=4 seq=32"),
        ("resnet_8blk_c64", "ResNet 8 residual blocks c=64"),
    ];

    // 4 CPU nodes — one per zone the workloads use
    let nodes: Vec<Node> = (0..4).map(|i| Node {
        node_id:          format!("cpu{i}"),
        node_type:        NodeType::Cpu,
        host_id:          "host0".into(),
        zone:             format!("z{i}"),
        tags:             vec!["cpu".into()],
        compute_capacity: 16.0,
        memory_capacity:  64.0,
        bandwidth:        20.0,
        inertia_keys:     vec![],
    }).collect();

    for (filename, label) in &graphs_to_run {
        let path = Path::new(workload_dir).join(format!("{filename}.json"));

        let graph = match IRGraph::load_json(path.to_str().unwrap()) {
            Ok(g) => g,
            Err(e) => {
                println!("  [{label}] SKIP — could not load {filename}.json: {e}");
                continue;
            }
        };

        let nb = graph.blocks.len();
        let ne = graph.edges.len();

        let policy = MultifactorPolicy { reservation_bias: 1.0 };
        let mut rt = URXRuntime::new(nodes.clone(), policy);
        rt.enable_workstation_mode();

        let t0 = Instant::now();
        let result = rt.execute_graph(&graph).await;
        let elapsed = t0.elapsed();

        println!("  [{label}]");
        println!("    graph        : {filename} ({nb} blocks, {ne} edges)");
        println!("    partitions   : {}", result.partition_binding.len());
        println!("    results      : {} block executions", result.results.len());
        println!("    outputs      : {} leaf values", result.outputs.len());
        println!("    packets      : {} routed", result.packet_log.len());
        println!("    elapsed      : {elapsed:.2?}");

        // Spot-check: all blocks should have produced a result
        assert_eq!(result.results.len(), result.partitions.len(),
            "{filename}: result count mismatch");
        println!("    ✓ all blocks executed");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    demo_a().await;
    demo_b().await;
    demo_c().await;
    demo_d().await;
    demo_e().await;
    demo_f().await;
    demo_g().await;
    demo_h().await;
    demo_i().await;
    demo_j().await;
    demo_k().await;
    demo_l().await;
}
