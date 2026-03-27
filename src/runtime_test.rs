#[cfg(test)]
mod tests {
    use crate::ir::{IRBlock, IRGraph, MergeMode, Opcode};
    use crate::node::{Node, NodeType};
    use crate::runtime::URPRuntime;
    use crate::policy::{SchedulerPolicy, MultifactorPolicy};

    #[tokio::test]
    async fn test_simple_graph_execution() {
        let mut graph = IRGraph::new();

        // Add a simple constant block
        let block1 = IRBlock::new("block1", Opcode::UConstI64(42));
        graph.blocks.push(block1);

        // Create a CPU node
        let node = Node::new("node1", NodeType::Cpu, 100);

        // Create runtime
        let runtime = URXRuntime::new(MultifactorPolicy::new());

        // Execute
        let result = runtime.execute(graph, vec![node]).await;

        assert!(result.is_ok());
        let runtime_result = result.unwrap();
        assert!(!runtime_result.outputs.is_empty());
    }

    #[test]
    fn test_topological_order() {
        let mut graph = IRGraph::new();

        // Create a dependency chain: block1 -> block2 -> block3
        let block1 = IRBlock::new("block1", Opcode::UConstI64(10));
        let block2 = IRBlock::new("block2", Opcode::UConstI64(20));
        let block3 = IRBlock::new("block3", Opcode::UConstI64(30));

        graph.blocks.push(block1);
        graph.blocks.push(block2);
        graph.blocks.push(block3);

        // Add edges: block1 -> block2, block2 -> block3
        graph.edges.push(IREdge {
            src_block: "block1".to_string(),
            dst_block: "block2".to_string(),
            output_key: String::new(),
            input_key: String::new(),
        });
        graph.edges.push(IREdge {
            src_block: "block2".to_string(),
            dst_block: "block3".to_string(),
            output_key: String::new(),
            input_key: String::new(),
        });

        // Get topological order
        let order = crate::runtime::topo_order(&graph);

        assert!(order.is_ok());
        let ordered_blocks = order.unwrap();

        // block1 should come before block2, block2 before block3
        let pos1 = ordered_blocks.iter().position(|b| b.block_id == "block1").unwrap();
        let pos2 = ordered_blocks.iter().position(|b| b.block_id == "block2").unwrap();
        let pos3 = ordered_blocks.iter().position(|b| b.block_id == "block3").unwrap();

        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
    }

    #[test]
    fn test_topological_order_with_cycle() {
        let mut graph = IRGraph::new();

        let block1 = IRBlock::new("block1", Opcode::UConstI64(10));
        let block2 = IRBlock::new("block2", Opcode::UConstI64(20));

        graph.blocks.push(block1);
        graph.blocks.push(block2);

        // Create a cycle: block1 -> block2, block2 -> block1
        graph.edges.push(IREdge {
            src_block: "block1".to_string(),
            dst_block: "block2".to_string(),
            output_key: String::new(),
            input_key: String::new(),
        });
        graph.edges.push(IREdge {
            src_block: "block2".to_string(),
            dst_block: "block1".to_string(),
            output_key: String::new(),
            input_key: String::new(),
        });

        // Should detect the cycle
        let order = crate::runtime::topo_order(&graph);
        assert!(order.is_err());
    }

    #[tokio::test]
    async fn test_multi_node_scheduling() {
        let mut graph = IRGraph::new();

        // Add multiple blocks
        for i in 0..5 {
            let block = IRBlock::new(&format!("block{}", i), Opcode::UConstI64(i * 10));
            graph.blocks.push(block);
        }

        // Create multiple nodes
        let cpu_node = Node::new("cpu1", NodeType::Cpu, 100);
        let gpu_node = Node::new("gpu1", NodeType::Gpu, 1000);

        let runtime = URXRuntime::new(MultifactorPolicy::new());

        let result = runtime.execute(
            graph,
            vec![cpu_node, gpu_node],
        ).await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_graph() {
        let graph = IRGraph::new();
        let order = crate::runtime::topo_order(&graph);

        assert!(order.is_ok());
        let ordered_blocks = order.unwrap();
        assert!(ordered_blocks.is_empty());
    }

    #[tokio::test]
    async fn test_single_block_graph() {
        let mut graph = IRGraph::new();
        let block = IRBlock::new("single", Opcode::UConstI64(999));
        graph.blocks.push(block);

        let node = Node::new("node1", NodeType::Cpu, 100);
        let runtime = URXRuntime::new(MultifactorPolicy::new());

        let result = runtime.execute(graph, vec![node]).await;

        assert!(result.is_ok());
        let runtime_result = result.unwrap();
        assert_eq!(runtime_result.outputs.len(), 1);
    }
}
