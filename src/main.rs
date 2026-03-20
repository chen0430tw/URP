mod node;
mod ir;
mod optimizer;
mod cost;
mod partition;
mod policy;
mod reservation;
mod packet;
mod ring;
mod remote;
mod executor;
mod reducer;
mod runtime;

use ir::{IRBlock, IREdge, IRGraph, MergeMode, Opcode};
use node::{Node, NodeType};
use policy::MultifactorPolicy;
use runtime::URXRuntime;

#[tokio::main]
async fn main() {
    let graph = IRGraph {
        graph_id: "demo_v08".into(),
        blocks: vec![
            IRBlock {
                block_id: "b1".into(),
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
                block_id: "b2".into(),
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
                block_id: "b3".into(),
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
                block_id: "b4".into(),
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
                block_id: "b5".into(),
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
            IREdge { src_block: "b1".into(), dst_block: "b3".into(), output_key: "x".into(), input_key: "a".into() },
            IREdge { src_block: "b2".into(), dst_block: "b3".into(), output_key: "y".into(), input_key: "b".into() },
            IREdge { src_block: "b4".into(), dst_block: "b5".into(), output_key: "prefix".into(), input_key: "left".into() },
            IREdge { src_block: "b3".into(), dst_block: "b5".into(), output_key: "sum".into(), input_key: "right".into() },
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
    let result = rt.execute_graph(&graph).await;

    println!("fused_graph_id = {}", result.fused_graph_id);
    println!("partitions = {:#?}", result.partitions);
    println!("partition_binding = {:#?}", result.partition_binding);
    println!("block_binding = {:#?}", result.block_binding);
    println!("packet_log = {:#?}", result.packet_log);
    println!("merged = {:#?}", result.merged);
    println!("remote_sent_packets = {}", result.remote_sent_packets);
}
