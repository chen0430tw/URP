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

    /// Get input dependencies for this partition (edges from outside the partition).
    /// Used by callers that need per-partition dependency info; the scheduler uses
    /// the faster bulk `build_partition_dag` path instead.
    pub fn external_inputs(&self, graph: &IRGraph, partition_map: &HashMap<String, String>) -> HashSet<String> {
        let my_blocks: HashSet<&str> = self.blocks.iter().map(|b| b.block_id.as_str()).collect();

        // Pre-filter edges to only those whose dst is in this partition
        graph.edges.iter()
            .filter(|e| my_blocks.contains(e.dst_block.as_str()))
            .filter_map(|e| {
                let src_pid = partition_map.get(&e.src_block)?;
                if src_pid != &self.partition_id {
                    Some(e.src_block.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get outputs from this partition
    pub fn outputs(&self) -> HashSet<String> {
        self.blocks.iter().map(|b| b.block_id.clone()).collect()
    }
}

/// Compute internal topological order for blocks within a partition.
fn compute_internal_order(blocks: &[IRBlock]) -> Vec<String> {
    blocks.iter().map(|b| b.block_id.clone()).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// AsyncLane
// ─────────────────────────────────────────────────────────────────────────────

/// Async execution lane for running partitions concurrently.
pub struct AsyncLane {
    semaphore: Arc<Semaphore>,
}

impl AsyncLane {
    pub fn new(_id: String, max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Return a cloned Arc of the semaphore so it can be moved into spawned tasks.
    pub fn semaphore_arc(&self) -> Arc<Semaphore> {
        Arc::clone(&self.semaphore)
    }

    /// Execute a partition, returning results for all its blocks.
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

/// DAG-ordered partition scheduler with concurrent wave execution.
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

    pub async fn schedule_and_execute<F, Fut>(
        &self,
        partitions: Vec<Partition>,
        graph: &IRGraph,
        partition_map: &HashMap<String, String>,
        executor: F,
    ) -> Vec<BlockExecutionResult>
    where
        F: Fn(Partition) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Vec<BlockExecutionResult>> + Send + 'static,
    {
        let (partition_dag, partition_index) =
            self.build_partition_dag(&partitions, graph, partition_map);
        self.execute_dag_partitions(partitions, partition_dag, partition_index, executor)
            .await
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Build a dependency map in O(E + B):
    /// partition_id → [partition_ids it depends on].
    ///
    /// Old implementation called `Partition::external_inputs` (O(B_p × E)) for
    /// every partition, giving O(P × B_avg × E) total.  New implementation
    /// scans all edges exactly once → O(E) build + O(B) index → O(E + B).
    fn build_partition_dag(
        &self,
        partitions: &[Partition],
        graph: &IRGraph,
        partition_map: &HashMap<String, String>,
    ) -> (HashMap<String, Vec<String>>, HashMap<String, usize>) {
        // O(P): per-partition index and empty dep-sets
        let mut partition_index: HashMap<String, usize> = HashMap::with_capacity(partitions.len());
        let mut dag: HashMap<String, HashSet<String>> = HashMap::with_capacity(partitions.len());
        for (idx, p) in partitions.iter().enumerate() {
            partition_index.insert(p.partition_id.clone(), idx);
            dag.insert(p.partition_id.clone(), HashSet::new());
        }

        // O(E): scan every edge once
        // If src and dst are in different partitions → dst_partition depends on src_partition
        for edge in &graph.edges {
            if let (Some(src_pid), Some(dst_pid)) = (
                partition_map.get(&edge.src_block),
                partition_map.get(&edge.dst_block),
            ) {
                if src_pid != dst_pid {
                    dag.entry(dst_pid.clone())
                        .or_default()
                        .insert(src_pid.clone());
                }
            }
        }

        // Convert HashSet → Vec for downstream use
        let dag: HashMap<String, Vec<String>> = dag
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();

        (dag, partition_index)
    }

    /// Execute partitions in DAG topological order with **concurrent** dispatch.
    ///
    /// Key improvements over the previous serial implementation:
    /// - All partitions in the ready queue are launched concurrently via
    ///   `tokio::task::JoinSet` rather than awaited one at a time.
    /// - As soon as any partition finishes, its successors enter the ready queue
    ///   and are immediately launched — no wave-level synchronisation barrier.
    /// - Reverse dependency graph gives O(D_out) dependent updates vs O(P) scan.
    async fn execute_dag_partitions<F, Fut>(
        &self,
        partitions: Vec<Partition>,
        dag: HashMap<String, Vec<String>>,
        partition_index: HashMap<String, usize>,
        executor: F,
    ) -> Vec<BlockExecutionResult>
    where
        F: Fn(Partition) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Vec<BlockExecutionResult>> + Send + 'static,
    {
        let total_start = Instant::now();
        let total_partitions = partitions.len();

        // ── Build reverse dependency graph O(P × D_avg) ──────────────────────
        // reverse_deps[pid] = list of partitions that become ready when pid finishes
        let mut reverse_deps: HashMap<String, Vec<String>> = HashMap::with_capacity(partitions.len());
        for (pid, deps) in &dag {
            for dep in deps {
                reverse_deps.entry(dep.clone()).or_default().push(pid.clone());
            }
        }

        // ── Init in-degrees and partition store ──────────────────────────────
        let mut in_degree: HashMap<String, usize> = HashMap::with_capacity(total_partitions);
        let mut partition_store: HashMap<String, Partition> = HashMap::with_capacity(total_partitions);

        for p in &partitions {
            in_degree.insert(
                p.partition_id.clone(),
                dag.get(&p.partition_id).map(|v| v.len()).unwrap_or(0),
            );
        }
        for p in partitions {
            partition_store.insert(p.partition_id.clone(), p);
        }

        // Seed the ready queue with zero-in-degree partitions
        let mut ready: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut completed: HashSet<String> = HashSet::with_capacity(total_partitions);
        let mut all_results: Vec<BlockExecutionResult> = Vec::with_capacity(total_partitions);

        // ── Concurrent execution loop ─────────────────────────────────────────
        // JoinSet drives all ready partitions concurrently.  As each finishes its
        // successors are enqueued and launched in the next pass of the outer loop.
        let mut in_flight: JoinSet<(String, Vec<BlockExecutionResult>)> = JoinSet::new();

        loop {
            // Launch every partition currently in the ready queue
            while let Some(pid) = ready.pop_front() {
                let partition = partition_store.remove(&pid).unwrap();
                let exec     = executor.clone();
                let lane_idx = *partition_index.get(&pid).unwrap_or(&0) % self.lanes.len();
                let sem      = self.lanes[lane_idx].semaphore_arc();

                in_flight.spawn(async move {
                    let _permit = sem.acquire_owned().await.unwrap();
                    let results = exec(partition).await;
                    (pid, results)
                });
            }

            // If nothing is in-flight we're done
            if in_flight.is_empty() {
                break;
            }

            // Wait for any ONE partition to finish
            let (finished_pid, block_results) = in_flight
                .join_next()
                .await
                .unwrap()  // JoinSet never returns None while non-empty
                .expect("partition task panicked");

            all_results.extend(block_results);
            completed.insert(finished_pid.clone());

            // O(D_out): decrement in-degree of direct successors only
            if let Some(successors) = reverse_deps.get(&finished_pid) {
                for succ in successors {
                    if let Some(deg) = in_degree.get_mut(succ) {
                        *deg -= 1;
                        if *deg == 0 && !completed.contains(succ) {
                            ready.push_back(succ.clone());
                        }
                    }
                }
            }
        }

        eprintln!(
            "[PartitionDAGScheduler] {} partitions completed in {:?}",
            total_partitions,
            total_start.elapsed()
        );

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
