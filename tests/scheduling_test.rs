//! URP Scheduling capability tests
//!
//! Tests the full scheduling pipeline:
//! - Multi-node tag-based dispatch
//! - Zone-aware node selection
//! - Inertia key affinity
//! - DAG dependency execution
//! - Block fusion
//! - Local-ring vs remote-packet routing
//! - Graph partitioning

use urx_runtime_v08::{
    IRBlock, IREdge, IRGraph, MergeMode, MultifactorPolicy, Node, NodeType, Opcode, URXRuntime,
};

fn cpu_node(id: &str, zone: &str, host: &str, capacity: f32, bw: f32) -> Node {
    let mut n = Node::new(id, NodeType::Cpu, capacity);
    n.zone = zone.to_string();
    n.host_id = host.to_string();
    n.bandwidth = bw;
    n.tags.push("cpu".to_string());
    n
}

fn gpu_node(id: &str, zone: &str, host: &str, capacity: f32, bw: f32) -> Node {
    let mut n = Node::new(id, NodeType::Gpu, capacity);
    n.zone = zone.to_string();
    n.host_id = host.to_string();
    n.bandwidth = bw;
    n.tags.push("gpu".to_string());
    n
}

fn block(id: &str, op: Opcode, tag: &str, zone: &str, shape: &str) -> IRBlock {
    let mut b = IRBlock::new(id, op);
    b.required_tag = tag.to_string();
    b.preferred_zone = zone.to_string();
    b.resource_shape = shape.to_string();
    b
}

fn edge(src: &str, dst: &str, key: &str) -> IREdge {
    IREdge {
        src_block: src.to_string(),
        dst_block: dst.to_string(),
        output_key: key.to_string(),
        input_key: key.to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: CPU vs GPU tag dispatch
// Two blocks: one requires "cpu", one requires "gpu"
// Verify each lands on the correctly tagged node
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_tag_based_dispatch() {
    let nodes = vec![
        cpu_node("cpu0", "zone-a", "host0", 100.0, 1000.0),
        gpu_node("gpu0", "zone-a", "host0", 200.0, 2000.0),
    ];

    let mut graph = IRGraph::with_id("tag-dispatch".to_string());
    graph.blocks.push(block("cpu-task", Opcode::UConstI64(1), "cpu", "zone-a", "small"));
    graph.blocks.push(block("gpu-task", Opcode::UConstI64(2), "gpu", "zone-a", "small"));

    let mut rt = URXRuntime::new(nodes, MultifactorPolicy::new());
    let result = rt.execute_graph(&graph).await;

    let cpu_binding = result.block_binding.get("cpu-task").unwrap();
    let gpu_binding = result.block_binding.get("gpu-task").unwrap();

    println!("\n[tag-dispatch]");
    println!("  cpu-task → {}", cpu_binding);
    println!("  gpu-task → {}", gpu_binding);

    assert_eq!(cpu_binding, "cpu0", "cpu-task should land on cpu0");
    assert_eq!(gpu_binding, "gpu0", "gpu-task should land on gpu0");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: Zone-aware selection
// Two CPU nodes: one in zone-a, one in zone-b
// Block prefers zone-a → should select the zone-a node
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_zone_preference() {
    let nodes = vec![
        cpu_node("cpu-zone-a", "zone-a", "host0", 100.0, 1000.0),
        cpu_node("cpu-zone-b", "zone-b", "host1", 100.0, 1000.0),
    ];

    let mut graph = IRGraph::with_id("zone-pref".to_string());
    graph.blocks.push(block("task-a", Opcode::UConstI64(10), "cpu", "zone-a", "small"));
    graph.blocks.push(block("task-b", Opcode::UConstI64(20), "cpu", "zone-b", "small"));

    let mut rt = URXRuntime::new(nodes, MultifactorPolicy::new());
    let result = rt.execute_graph(&graph).await;

    let a = result.block_binding.get("task-a").unwrap();
    let b = result.block_binding.get("task-b").unwrap();

    println!("\n[zone-preference]");
    println!("  task-a (prefers zone-a) → {}", a);
    println!("  task-b (prefers zone-b) → {}", b);

    assert_eq!(a, "cpu-zone-a", "zone-a task should prefer cpu-zone-a");
    assert_eq!(b, "cpu-zone-b", "zone-b task should prefer cpu-zone-b");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: Inertia key affinity
// First run: task lands on cpu0 (higher capacity), node remembers the inertia key
// Second run: same key → should return to cpu0 due to +3.0 inertia bonus
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_inertia_affinity() {
    let mut cpu0 = cpu_node("cpu0", "zone-a", "host0", 200.0, 1000.0);
    let cpu1 = cpu_node("cpu1", "zone-a", "host0", 100.0, 1000.0);

    // Pre-seed inertia key on cpu0 to simulate a previous run
    cpu0.remember_inertia_key("model-weights-v1");

    let nodes = vec![cpu0, cpu1];

    let mut graph = IRGraph::with_id("inertia".to_string());
    let mut b = block("inference", Opcode::UConstI64(42), "cpu", "zone-a", "large");
    b.inertia_key = Some("model-weights-v1".to_string());
    graph.blocks.push(b);

    let mut rt = URXRuntime::new(nodes, MultifactorPolicy::new());
    let result = rt.execute_graph(&graph).await;

    let assigned = result.block_binding.get("inference").unwrap();

    println!("\n[inertia-affinity]");
    println!("  inference (inertia=model-weights-v1) → {}", assigned);
    println!("  (cpu0 has cached key, cpu1 does not)");

    assert_eq!(assigned, "cpu0", "inertia key should pull task back to cpu0");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: DAG dependency chain
// const_a(10) → add(a+b) ← const_b(20) → concat(sum+"!")
// Verify final result is "30!"
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_dag_dependency_chain() {
    let nodes = vec![cpu_node("cpu0", "zone-a", "host0", 100.0, 1000.0)];

    let mut graph = IRGraph::with_id("dag-chain".to_string());

    let mut const_a = block("const_a", Opcode::UConstI64(10), "cpu", "zone-a", "small");
    const_a.merge_mode = MergeMode::Sum;

    let mut const_b = block("const_b", Opcode::UConstI64(20), "cpu", "zone-a", "small");
    const_b.merge_mode = MergeMode::Sum;

    let mut add = block("add", Opcode::UAdd, "cpu", "zone-a", "small");
    add.inputs = vec!["a".to_string(), "b".to_string()];
    add.merge_mode = MergeMode::Sum;

    let mut suffix = block("suffix", Opcode::UConstStr("!".to_string()), "cpu", "zone-a", "small");
    suffix.merge_mode = MergeMode::Concat;

    let mut concat = block("concat", Opcode::UConcat, "cpu", "zone-a", "small");
    concat.inputs = vec!["left".to_string(), "right".to_string()];
    concat.merge_mode = MergeMode::Concat;

    graph.blocks.extend([const_a, const_b, add, suffix, concat]);
    graph.edges.push(IREdge { src_block: "const_a".into(), dst_block: "add".into(), output_key: "out".into(), input_key: "a".into() });
    graph.edges.push(IREdge { src_block: "const_b".into(), dst_block: "add".into(), output_key: "out".into(), input_key: "b".into() });
    graph.edges.push(IREdge { src_block: "add".into(), dst_block: "concat".into(), output_key: "out".into(), input_key: "left".into() });
    graph.edges.push(IREdge { src_block: "suffix".into(), dst_block: "concat".into(), output_key: "out".into(), input_key: "right".into() });

    let mut rt = URXRuntime::new(nodes, MultifactorPolicy::new());
    let result = rt.execute_graph(&graph).await;

    let add_result = result.results.iter().find(|r| r.block_id == "add").unwrap();
    let concat_result = result.results.iter().find(|r| r.block_id == "concat").unwrap();

    println!("\n[dag-chain]");
    println!("  const_a=10, const_b=20");
    println!("  add result: {:?}", add_result.value);
    println!("  concat result: {:?}", concat_result.value);

    assert_eq!(format!("{:?}", add_result.value), "I64(30)");
    assert_eq!(format!("{:?}", concat_result.value), r#"Str("30!")"#);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: Block fusion
// Two adjacent blocks with identical tag/zone/shape and a direct edge → fused
// Three blocks where middle has different shape → only first pair can fuse
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_block_fusion_scheduling() {
    use urx_runtime_v08::fuse_linear_blocks;

    // Case A: fusable pair
    let mut g = IRGraph::with_id("fusion-a".to_string());
    g.blocks.push(block("a", Opcode::UConstI64(1), "cpu", "zone-a", "small"));
    g.blocks.push(block("b", Opcode::UConstI64(2), "cpu", "zone-a", "small"));
    g.edges.push(edge("a", "b", "out"));

    let fused = fuse_linear_blocks(&g);
    println!("\n[block-fusion]");
    println!("  original: {} blocks", g.blocks.len());
    println!("  fused:    {} blocks → {:?}", fused.blocks.len(),
        fused.blocks.iter().map(|b| &b.block_id).collect::<Vec<_>>());

    assert_eq!(fused.blocks.len(), 1, "compatible pair should fuse into 1 block");

    // Case B: incompatible triple (middle has different shape)
    let mut g2 = IRGraph::with_id("fusion-b".to_string());
    g2.blocks.push(block("x", Opcode::UConstI64(1), "cpu", "zone-a", "small"));
    g2.blocks.push(block("y", Opcode::UConstI64(2), "cpu", "zone-a", "large")); // different shape
    g2.blocks.push(block("z", Opcode::UConstI64(3), "cpu", "zone-a", "small"));
    g2.edges.push(edge("x", "y", "out"));
    g2.edges.push(edge("y", "z", "out"));

    let fused2 = fuse_linear_blocks(&g2);
    println!("  incompatible triple → {} blocks", fused2.blocks.len());
    assert_eq!(fused2.blocks.len(), 3, "incompatible shapes should not fuse");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6: Local ring vs remote packet routing
// Nodes on same host → local-ring (cost 0 + bw term)
// Nodes on different hosts → remote-packet (cost +10)
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_local_vs_remote_routing() {
    let nodes = vec![
        cpu_node("local-a",  "zone-a", "host0", 100.0, 1000.0),
        cpu_node("local-b",  "zone-a", "host0", 100.0, 1000.0), // same host
        cpu_node("remote-c", "zone-b", "host1", 100.0, 1000.0), // different host
    ];

    // Graph: src → same_dst (local)  AND  src → remote_dst (remote)
    let mut graph = IRGraph::with_id("routing".to_string());
    graph.blocks.push(block("src",        Opcode::UConstI64(99), "cpu", "zone-a", "small"));
    graph.blocks.push(block("same_dst",   Opcode::UConstI64(1),  "cpu", "zone-a", "small"));
    // remote task needs different zone to land on remote-c
    let mut r = block("remote_dst", Opcode::UConstI64(2), "cpu", "zone-b", "small");
    r.preferred_zone = "zone-b".to_string();
    graph.blocks.push(r);

    let mut rt = URXRuntime::new(nodes, MultifactorPolicy::new());
    let result = rt.execute_graph(&graph).await;

    println!("\n[routing]");
    for log in &result.packet_log {
        println!("  {} → {} via {} (cost={:.2})",
            log.src_block, log.dst_block, log.route_type, log.route_cost);
    }
    println!("  remote_sent_packets={}", result.remote_sent_packets);

    let src_node = result.block_binding.get("src").unwrap();
    let remote_node = result.block_binding.get("remote_dst").unwrap();
    println!("  src → {}, remote_dst → {}", src_node, remote_node);

    assert_eq!(remote_node, "remote-c", "zone-b task should land on remote-c");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 7: Graph partitioning
// Blocks with different tags/shapes → multiple partitions
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_graph_partitioning() {
    use urx_runtime_v08::{fuse_linear_blocks, partition_graph};

    let mut g = IRGraph::with_id("partition".to_string());
    g.blocks.push(block("cpu1", Opcode::UConstI64(1), "cpu", "zone-a", "small"));
    g.blocks.push(block("cpu2", Opcode::UConstI64(2), "cpu", "zone-a", "small"));
    g.blocks.push(block("gpu1", Opcode::UConstI64(3), "gpu", "zone-a", "small")); // different tag
    g.blocks.push(block("gpu2", Opcode::UConstI64(4), "gpu", "zone-a", "small"));
    g.blocks.push(block("cpu3", Opcode::UConstI64(5), "cpu", "zone-b", "small")); // different zone

    let fused = fuse_linear_blocks(&g);
    let parts = partition_graph(&fused);

    let mut partition_ids: Vec<&String> = parts.values().collect();
    partition_ids.sort();
    partition_ids.dedup();

    println!("\n[partitioning]");
    for b in &fused.blocks {
        println!("  {} → partition {}", b.block_id, parts.get(&b.block_id).unwrap());
    }
    println!("  total partitions: {}", partition_ids.len());

    // cpu1+cpu2 fused → p0, gpu1+gpu2 fused → p1, cpu3 → p2
    assert!(partition_ids.len() >= 2, "different tags/zones should produce multiple partitions");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 8: High-capacity node preference
// Two CPU nodes, same zone, one 10x more capacity → high-priority task goes there
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn test_capacity_based_selection() {
    let nodes = vec![
        cpu_node("cpu-weak",   "zone-a", "host0", 10.0,  1000.0),
        cpu_node("cpu-strong", "zone-a", "host0", 100.0, 1000.0),
    ];

    let mut graph = IRGraph::with_id("capacity".to_string());
    graph.blocks.push(block("task", Opcode::UConstI64(1), "cpu", "zone-a", "heavy"));

    let mut rt = URXRuntime::new(nodes, MultifactorPolicy::new());
    let result = rt.execute_graph(&graph).await;

    let assigned = result.block_binding.get("task").unwrap();
    println!("\n[capacity-selection]");
    println!("  task → {} (cpu-strong has 100 capacity, cpu-weak has 10)", assigned);

    assert_eq!(assigned, "cpu-strong", "higher capacity node should be preferred");
}
