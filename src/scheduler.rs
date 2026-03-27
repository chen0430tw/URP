//! Partition DAG Scheduling
//!
//! This module implements DAG-based scheduling for partitions, where:
//! - Partitions have internal ordering (blocks within a partition execute sequentially)
//! - Partitions之间的依赖形成DAG，可以并行执行
//! - Async execution lanes handle concurrent partition execution

use crate::ir::{IRBlock, IRGraph, MergeMode};
use crate::policy::SchedulerPolicy;
use crate::runtime::{BlockExecutionResult, RuntimeResult};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;

/// A partition with its blocks and execution metadata
#[derive(Debug, Clone)]
pub struct Partition {
    pub partition_id: String,
    pub blocks: Vec<IRBlock>,
    pub node_id: String,
    pub internal_order: Vec<String>, // Topological order within partition
}

impl Partition {
    pub fn new(partition_id: String, blocks: Vec<IRBlock>, node_id: String) -> Self {
        let internal_order = compute_internal_order(&blocks);
        Self {
            partition_id,
            blocks,
            node_id,
            internal_order,
        }
    }

    /// Get input dependencies for this partition (edges from outside the partition)
    pub fn external_inputs(&self, graph: &IRGraph, partition_map: &HashMap<String, String>) -> HashSet<String> {
        let mut inputs = HashSet::new();
        let my_partitions: HashSet<String> = self.blocks.iter()
            .map(|b| partition_map.get(&b.block_id).cloned().unwrap_or_default())
            .collect();

        for block in &self.blocks {
            for edge in &graph.edges {
                if edge.dst_block == block.block_id {
                    let src_partition = partition_map.get(&edge.src_block);
                    if let Some(sp) = src_partition {
                        if !my_partitions.contains(sp) && sp != &self.partition_id {
                            inputs.insert(edge.src_block.clone());
                        }
                    }
                }
            }
        }
        inputs
    }

    /// Get outputs from this partition
    pub fn outputs(&self) -> HashSet<String> {
        self.blocks.iter().map(|b| b.block_id.clone()).collect()
    }
}

/// Compute internal topological order for blocks within a partition
fn compute_internal_order(blocks: &[IRBlock]) -> Vec<String> {
    let block_ids: HashSet<String> = blocks.iter().map(|b| b.block_id.clone()).collect();
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    for block in blocks {
        in_degree.entry(block.block_id.clone()).or_insert(0);
    }

    // Build adjacency list (only consider internal edges)
    for block in blocks {
        for src_block in &block.inputs {
            if block_ids.contains(src_block) {
                adj.entry(src_block.clone()).or_default().push(block.block_id.clone());
                *in_degree.entry(block.block_id.clone()).or_insert(0) += 1;
            }
        }
    }

    // Topological sort using Kahn's algorithm
    let mut queue: VecDeque<String> = in_degree.iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut result = Vec::new();
    while let Some(id) = queue.pop_front() {
        result.push(id.clone());
        if let Some(neighbors) = adj.get(&id) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    if *deg > 0 {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(neighbor.clone());
                        }
                    }
                }
            }
        }
    }

    result
}

/// Async execution lane for running partitions concurrently
pub struct AsyncLane {
    #[allow(dead_code)]
    id: String,
    semaphore: Arc<Semaphore>,
    #[allow(dead_code)]
    active_tasks: Arc<Mutex<JoinSet<BlockExecutionResult>>>,
}

impl AsyncLane {
    pub fn new(id: String, max_concurrent: usize) -> Self {
        Self {
            id,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            active_tasks: Arc::new(Mutex::new(JoinSet::new())),
        }
    }

    pub async fn execute_partition<F, Fut>(
        &self,
        partition: &Partition,
        executor: F,
    ) -> BlockExecutionResult
    where
        F: Fn(IRBlock, String) -> Fut + Clone,
        Fut: std::future::Future<Output = BlockExecutionResult> + Send,
    {
        let _permit = self.semaphore.acquire().await.unwrap();

        // Execute blocks sequentially within partition
        let mut results = Vec::new();
        for block_id in &partition.internal_order {
            if let Some(block) = partition.blocks.iter().find(|b| b.block_id == *block_id) {
                let result = executor(block.clone(), partition.node_id.clone()).await;
                results.push(result);
            }
        }

        // Return the last result as the partition result
        results.into_iter().last().unwrap_or_else(|| BlockExecutionResult {
            block_id: partition.partition_id.clone(),
            partition_id: partition.partition_id.clone(),
            node_id: partition.node_id.clone(),
            start_time: 0,
            end_time: 0,
            value: crate::reducer::PayloadValue::I64(0),
            merge_mode: MergeMode::List,
        })
    }
}

/// Partition DAG scheduler
pub struct PartitionDAGScheduler {
    lanes: Vec<AsyncLane>,
}

impl PartitionDAGScheduler {
    pub fn new(num_lanes: usize, max_concurrent_per_lane: usize) -> Self {
        let lanes = (0..num_lanes)
            .map(|i| AsyncLane::new(format!("lane-{}", i), max_concurrent_per_lane))
            .collect();
        Self { lanes }
    }

    /// Schedule and execute partitions following DAG dependencies
    pub async fn schedule_and_execute<P, F, Fut>(
        &self,
        partitions: Vec<Partition>,
        graph: &IRGraph,
        partition_map: &HashMap<String, String>,
        executor: F,
    ) -> RuntimeResult
    where
        P: SchedulerPolicy,
        F: Fn(IRBlock, String) -> Fut + Clone,
        Fut: std::future::Future<Output = BlockExecutionResult> + Send,
    {
        // Build partition dependency graph
        let (partition_dag, partition_index) = self.build_partition_dag(&partitions, graph, partition_map);

        // Execute partitions using topological order
        let results: RuntimeResult = self.execute_dag_partitions::<F, Fut>(partitions, partition_dag, partition_index, executor).await;

        results
    }

    /// Build DAG of partition dependencies
    fn build_partition_dag(
        &self,
        partitions: &[Partition],
        graph: &IRGraph,
        partition_map: &HashMap<String, String>,
    ) -> (HashMap<String, Vec<String>>, HashMap<String, usize>) {
        let mut partition_index = HashMap::new();
        for (idx, p) in partitions.iter().enumerate() {
            partition_index.insert(p.partition_id.clone(), idx);
        }

        let mut dag: HashMap<String, Vec<String>> = HashMap::new();
        for p in partitions {
            let deps = p.external_inputs(graph, partition_map);
            let dep_partitions: HashSet<String> = deps.iter()
                .filter_map(|bid| partition_map.get(bid))
                .filter(|pid| *pid != &p.partition_id)
                .cloned()
                .collect();

            dag.insert(p.partition_id.clone(), dep_partitions.into_iter().collect());
        }

        (dag, partition_index)
    }

    /// Execute partitions in DAG order with parallel execution
    async fn execute_dag_partitions<F, Fut>(
        &self,
        partitions: Vec<Partition>,
        dag: HashMap<String, Vec<String>>,
        partition_index: HashMap<String, usize>,
        executor: F,
    ) -> RuntimeResult
    where
        F: Fn(IRBlock, String) -> Fut + Clone,
        Fut: std::future::Future<Output = BlockExecutionResult> + Send,
    {
        // Track completed partitions and their in-degrees
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut partition_map: HashMap<String, Partition> = HashMap::new();

        for p in &partitions {
            let deps = dag.get(&p.partition_id).map(|v| v.len()).unwrap_or(0);
            in_degree.insert(p.partition_id.clone(), deps);
            partition_map.insert(p.partition_id.clone(), p.clone());
        }

        // Find partitions with no dependencies
        let mut ready: VecDeque<String> = in_degree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut completed: HashSet<String> = HashSet::new();
        let mut all_results: Vec<BlockExecutionResult> = Vec::new();

        // Process partitions in topological order
        while let Some(partition_id) = ready.pop_front() {
            let partition = partition_map.remove(&partition_id).unwrap();

            // Execute this partition
            let lane_idx = *partition_index.get(&partition_id).unwrap_or(&0) % self.lanes.len();
            let result = self.lanes[lane_idx].execute_partition(&partition, executor.clone()).await;
            all_results.push(result);
            completed.insert(partition_id.clone());

            // Update in-degrees of dependent partitions
            for (pid, deps) in &dag {
                if deps.contains(&partition_id) {
                    if let Some(deg) = in_degree.get_mut(pid) {
                        *deg -= 1;
                        if *deg == 0 && !completed.contains(pid) {
                            ready.push_back(pid.clone());
                        }
                    }
                }
            }
        }

        // Collect partition bindings
        let partition_binding: HashMap<String, String> = partitions.iter()
            .map(|p| (p.partition_id.clone(), p.node_id.clone()))
            .collect();

        RuntimeResult {
            fused_graph_id: "dag-schedule".to_string(),
            partitions: partition_binding.clone(),
            partition_binding: partition_binding.clone(),
            block_binding: HashMap::new(),
            results: all_results,
            outputs: Vec::new(),
            packet_log: Vec::new(),
            merged: HashMap::new(),
            remote_sent_packets: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IRBlock, IRGraph, MergeMode, Opcode};

    #[test]
    fn test_internal_order_computation() {
        let block1 = IRBlock::new("b1", Opcode::UConstI64(10));
        let block2 = IRBlock::new("b2", Opcode::UConstI64(20));
        let mut block3 = IRBlock::new("b3", Opcode::UAdd);
        block3.inputs = vec!["b1".to_string(), "b2".to_string()];

        let blocks = vec![block1.clone(), block2.clone(), block3];
        let order = compute_internal_order(&blocks);

        // b1 and b2 should come before b3
        let pos1 = order.iter().position(|id| id == "b1");
        let pos2 = order.iter().position(|id| id == "b2");
        let pos3 = order.iter().position(|id| id == "b3");

        assert!(pos1.is_some());
        assert!(pos2.is_some());
        assert!(pos3.is_some());
        if let (Some(p1), Some(p2), Some(p3)) = (pos1, pos2, pos3) {
            assert!(p1 < p3);
            assert!(p2 < p3);
        }
    }

    #[test]
    fn test_partition_creation() {
        let blocks = vec![
            IRBlock::new("b1", Opcode::UConstI64(10)),
            IRBlock::new("b2", Opcode::UConstI64(20)),
        ];
        let partition = Partition::new("p1".to_string(), blocks, "node1".to_string());

        assert_eq!(partition.partition_id, "p1");
        assert_eq!(partition.node_id, "node1");
        assert_eq!(partition.blocks.len(), 2);
        assert!(!partition.internal_order.is_empty());
    }

    #[tokio::test]
    async fn test_async_lane_creation() {
        let lane = AsyncLane::new("test-lane".to_string(), 2);
        assert_eq!(lane.id, "test-lane");
    }
}
