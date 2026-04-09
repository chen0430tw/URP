//! Partition DAG Scheduling
//!
//! This module implements DAG-based scheduling for partitions, where:
//! - Partitions have internal ordering (blocks within a partition execute sequentially)
//! - Partitions之间的依赖形成DAG，可以并行执行
//! - Async execution lanes handle concurrent partition execution

use crate::ir::{IRGraph, IRBlock};
use crate::runtime::BlockExecutionResult;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

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

/// Compute internal topological order for blocks within a partition.
///
/// Uses the blocks' natural insertion order as a stable baseline.
/// The actual cross-block dependencies within a partition are resolved at
/// runtime via `intra_partition_topo` in runtime.rs, which has access to the
/// full IRGraph edges. This function is kept for the `Partition` struct field
/// and for callers that don't have graph access.
fn compute_internal_order(blocks: &[IRBlock]) -> Vec<String> {
    // Return block IDs in their given order (already topologically consistent
    // if blocks were added via partition_graph which iterates the fused graph).
    blocks.iter().map(|b| b.block_id.clone()).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// AsyncLane
// ─────────────────────────────────────────────────────────────────────────────

/// Async execution lane for running partitions concurrently.
///
/// Each lane has a semaphore limiting concurrency. The caller provides a
/// closure `F: Fn(Partition) -> Fut` that is fully responsible for block-level
/// execution, inbox management, and inter-block value routing.
pub struct AsyncLane {
    semaphore: Arc<Semaphore>,
}

impl AsyncLane {
    pub fn new(_id: String, max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Execute a partition, returning results for all its blocks.
    ///
    /// The `executor` closure owns the execution logic: it reads inbox state,
    /// runs each block, routes outputs to downstream inboxes, and returns one
    /// `BlockExecutionResult` per block in the partition.
    pub async fn execute_partition<F, Fut>(
        &self,
        partition: Partition,
        executor: F,
    ) -> Vec<BlockExecutionResult>
    where
        F: Fn(Partition) -> Fut,
        Fut: std::future::Future<Output = Vec<BlockExecutionResult>> + Send,
    {
        let _permit = self.semaphore.acquire().await.unwrap();
        executor(partition).await
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PartitionDAGScheduler
// ─────────────────────────────────────────────────────────────────────────────

/// DAG-ordered partition scheduler.
///
/// Builds an inter-partition dependency graph from `Partition::external_inputs`,
/// then executes partitions in topological order via `AsyncLane`s.
///
/// The caller provides the block-level execution logic as a closure:
/// ```text
/// F: Fn(Partition) -> impl Future<Output = Vec<BlockExecutionResult>>
/// ```
/// This closure is called once per partition and is responsible for executing
/// every block within that partition (including inbox reads and output routing).
/// `URXRuntime::execute_graph` uses this scheduler internally.
pub struct PartitionDAGScheduler {
    pub lanes: Vec<AsyncLane>,
}

impl PartitionDAGScheduler {
    pub fn new(num_lanes: usize, max_concurrent_per_lane: usize) -> Self {
        let lanes = (0..num_lanes)
            .map(|i| AsyncLane::new(format!("lane-{}", i), max_concurrent_per_lane))
            .collect();
        Self { lanes }
    }

    /// Schedule and execute partitions following DAG dependencies.
    ///
    /// Partitions are ordered by `Partition::external_inputs` dependencies, then
    /// dispatched to `AsyncLane`s in topological order. Returns a flat
    /// `Vec<BlockExecutionResult>` for all blocks across all partitions.
    pub async fn schedule_and_execute<F, Fut>(
        &self,
        partitions: Vec<Partition>,
        graph: &IRGraph,
        partition_map: &HashMap<String, String>,
        executor: F,
    ) -> Vec<BlockExecutionResult>
    where
        F: Fn(Partition) -> Fut + Clone,
        Fut: std::future::Future<Output = Vec<BlockExecutionResult>> + Send,
    {
        let (partition_dag, partition_index) =
            self.build_partition_dag(&partitions, graph, partition_map);
        self.execute_dag_partitions(partitions, partition_dag, partition_index, executor)
            .await
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Build a dependency map: partition_id → [partition_ids it depends on].
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
            let dep_partitions: HashSet<String> = deps
                .iter()
                .filter_map(|bid| partition_map.get(bid))
                .filter(|pid| *pid != &p.partition_id)
                .cloned()
                .collect();
            dag.insert(p.partition_id.clone(), dep_partitions.into_iter().collect());
        }

        (dag, partition_index)
    }

    /// Execute partitions in DAG topological order, collecting all results.
    ///
    /// **OPTIMIZED**: Uses reverse dependency graph to achieve O(P + E) complexity
    /// instead of O(P²) where P = partitions, E = edges.
    async fn execute_dag_partitions<F, Fut>(
        &self,
        partitions: Vec<Partition>,
        dag: HashMap<String, Vec<String>>,
        partition_index: HashMap<String, usize>,
        executor: F,
    ) -> Vec<BlockExecutionResult>
    where
        F: Fn(Partition) -> Fut + Clone,
        Fut: std::future::Future<Output = Vec<BlockExecutionResult>> + Send,
    {
        let total_start = Instant::now();

        // ── Step 1: Build reverse dependency graph (OPTIMIZATION) ─────────────
        // partition_id -> [partition_ids that depend on it]
        // Complexity: O(P × D), one-time cost
        let reverse_deps_start = Instant::now();
        let mut reverse_deps: HashMap<String, Vec<String>> = HashMap::new();
        for (pid, deps) in &dag {
            for dep_pid in deps {
                reverse_deps.entry(dep_pid.clone())
                    .or_insert_with(Vec::new)
                    .push(pid.clone());
            }
        }
        let reverse_deps_time = reverse_deps_start.elapsed();

        // ── Step 2: Compute Kahn in-degrees ───────────────────────────────────
        let init_start = Instant::now();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut partition_store: HashMap<String, Partition> = HashMap::new();
        let total_partitions = partitions.len();

        for p in &partitions {
            let deps = dag.get(&p.partition_id).map(|v| v.len()).unwrap_or(0);
            in_degree.insert(p.partition_id.clone(), deps);
        }
        for p in partitions {
            partition_store.insert(p.partition_id.clone(), p);
        }

        let mut ready: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut completed: HashSet<String> = HashSet::new();
        let mut all_results: Vec<BlockExecutionResult> = Vec::with_capacity(partition_store.len());
        let init_time = init_start.elapsed();

        // ── Step 3: Execute partitions in topological order ───────────────────
        let execute_start = Instant::now();
        let mut total_update_time = std::time::Duration::ZERO;
        let mut total_execute_time = std::time::Duration::ZERO;

        while let Some(partition_id) = ready.pop_front() {
            let partition = partition_store.remove(&partition_id).unwrap();

            let exec_start = Instant::now();
            let lane_idx = *partition_index.get(&partition_id).unwrap_or(&0) % self.lanes.len();
            let block_results = self.lanes[lane_idx]
                .execute_partition(partition, executor.clone())
                .await;
            total_execute_time += exec_start.elapsed();
            all_results.extend(block_results);
            completed.insert(partition_id.clone());

            // OPTIMIZED: use reverse dependency graph, O(D_incoming) instead of O(P)
            let update_start = Instant::now();
            if let Some(deps) = reverse_deps.get(&partition_id) {
                for dep_pid in deps {
                    if let Some(deg) = in_degree.get_mut(dep_pid) {
                        *deg -= 1;
                        if *deg == 0 && !completed.contains(dep_pid) {
                            ready.push_back(dep_pid.clone());
                        }
                    }
                }
            }
            total_update_time += update_start.elapsed();
        }

        let execute_time = execute_start.elapsed();
        let total_time = total_start.elapsed();

        eprintln!("PartitionDAGScheduler performance stats:");
        eprintln!("  Partitions: {}", total_partitions);
        eprintln!("  Total time: {:?}", total_time);
        eprintln!("    - Reverse deps build: {:?}", reverse_deps_time);
        eprintln!("    - Initialization: {:?}", init_time);
        eprintln!("    - Execution loop: {:?}", execute_time);
        eprintln!("      - Partition execution: {:?}", total_execute_time);
        eprintln!("      - Dependency update: {:?}", total_update_time);
        eprintln!("  Avg time per partition: {:?}", total_time / total_partitions.max(1) as u32);

        all_results
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IRBlock, Opcode};

    #[test]
    fn test_internal_order_computation() {
        let block1 = IRBlock::new("b1", Opcode::UConstI64(10));
        let block2 = IRBlock::new("b2", Opcode::UConstI64(20));
        let mut block3 = IRBlock::new("b3", Opcode::UAdd);
        block3.inputs = vec!["b1".to_string(), "b2".to_string()];

        let blocks = vec![block1.clone(), block2.clone(), block3];
        let order = compute_internal_order(&blocks);

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
        let _lane = AsyncLane::new("test-lane".to_string(), 2);
    }
}
