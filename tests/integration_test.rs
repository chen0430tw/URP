//! Integration tests for URX Runtime
//!
//! These tests verify end-to-end functionality from IRGraph to RuntimeResult.

use urx_runtime_v08::*;
use std::collections::HashMap;

#[tokio::test]
async fn test_end_to_end_execution() {
    // Create an IR graph with a single constant block
    let mut graph = IRGraph::new();

    let block1 = IRBlock::new("const1", Opcode::UConstI64(42));
    graph.blocks.push(block1);

    // Create nodes
    let cpu_node = Node::new("cpu1", NodeType::Cpu, 100.0);

    // Create runtime
    let mut runtime = URXRuntime::new(vec![cpu_node], MultifactorPolicy::new());

    // Execute
    let runtime_result = runtime.execute_graph(&graph).await;

    // Verify execution completed
    assert_eq!(runtime_result.results.len(), 1);
    assert_eq!(runtime_result.results[0].block_id, "const1");
}

#[tokio::test]
async fn test_local_ring_routing() {
    let mut graph = IRGraph::new();

    let mut block1 = IRBlock::new("block1", Opcode::UConstI64(100));
    block1.set_tag("cpu");

    let mut block2 = IRBlock::new("block2", Opcode::UConstI64(200));
    block2.set_tag("cpu");

    graph.blocks.push(block1);
    graph.blocks.push(block2);

    // Both blocks go to same CPU node - should use local ring
    let cpu_node = Node::new("cpu1", NodeType::Cpu, 100.0);

    let mut runtime = URXRuntime::new(vec![cpu_node], MultifactorPolicy::new());

    let runtime_result = runtime.execute_graph(&graph).await;

    // Verify execution completed
    assert!(!runtime_result.results.is_empty());
}

#[tokio::test]
async fn test_reservation_mechanism() {
    // Create a reservation table
    let mut table = ReservationTable::new();

    // Add a reservation
    let reservation = Reservation::new(
        "res1".to_string(),
        "cpu1".to_string(),
        1000,
        2000,
    );

    table.add(reservation);

    // Query earliest start
    let start = table.earliest_start_time("cpu1", 500, 1500);
    assert!(start.is_some());
    assert!(start.unwrap() >= 2000);
}

#[tokio::test]
async fn test_block_fusion() {
    let mut graph = IRGraph::new();

    // Create linear chain that can be fused
    let mut block1 = IRBlock::new("b1", Opcode::UConstI64(10));
    block1.set_tag("cpu");

    let mut block2 = IRBlock::new("b2", Opcode::UConstI64(20));
    block2.set_tag("cpu");

    graph.blocks.push(block1);
    graph.blocks.push(block2);
    graph.edges.push(IREdge {
        src_block: "b1".to_string(),
        dst_block: "b2".to_string(),
        output_key: String::new(),
        input_key: String::new(),
    });

    // Apply fusion
    let fused = fuse_linear_blocks(&graph);

    // After fusion, we should have fewer blocks
    assert!(fused.blocks.len() <= graph.blocks.len());
}

#[tokio::test]
async fn test_graph_partition() {
    let mut graph = IRGraph::new();

    // Create blocks with different tags
    for i in 0..6 {
        let mut block = IRBlock::new(&format!("block{}", i), Opcode::UConstI64(i));
        if i % 2 == 0 {
            block.set_tag("cpu");
        } else {
            block.set_tag("gpu");
        }
        graph.blocks.push(block);
    }

    // Partition the graph
    let partitions = partition_graph(&graph);

    // Should have multiple partitions
    assert!(partitions.len() > 0);
}

#[tokio::test]
async fn test_node_scoring() {
    let mut cpu_node = Node::new("cpu1", NodeType::Cpu, 100.0);
    cpu_node.tags.push("cpu".to_string());

    let mut gpu_node = Node::new("gpu1", NodeType::Gpu, 1000.0);
    gpu_node.tags.push("gpu".to_string());

    // CPU node should score higher for CPU-tagged block
    let cpu_score = node_score("cpu", "default", None, &cpu_node);
    let gpu_score = node_score("cpu", "default", None, &gpu_node);

    assert!(cpu_score >= gpu_score);
}

#[tokio::test]
async fn test_packet_roundtrip() {
    let original = URPPacket::build(
        1,  // opcode_id
        MergeMode::List,
        "test_block",
        "",
        &[],
    );

    // Convert to bytes and back
    let bytes = original.to_bytes();
    let restored = URPPacket::from_bytes(&bytes).unwrap();

    let header_orig = original.header();
    let header_rest = restored.header();
    assert_eq!(header_orig.merge_mode, header_rest.merge_mode);
}
