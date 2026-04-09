use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use crate::cost::route_cost;
use crate::et_cooling::ETCoolingPolicy;
use crate::executor::{ExecutorRegistry, ThreadPoolExecutor};
use crate::ir::{IRBlock, IRGraph, MergeMode};
use crate::node::{Node, NodeType};
use crate::optimizer::{fuse_linear_blocks, partition_graph};
use crate::packet::{PayloadCodec, PayloadValue, URPPacket};
use crate::partition::bind_partitions;
use crate::policy::SchedulerPolicy;
use crate::reducer::run_reducers;
use crate::remote::RemotePacketLink;
use crate::reservation::{Reservation, ReservationTable};
use crate::ring::LocalRingTunnel;
use crate::scheduler::{Partition, PartitionDAGScheduler};
use tokio::sync::Mutex;

// ─────────────────────────────────────────────────────────────────────────────
// Result types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PacketLog {
    pub src_block: String,
    pub dst_block: String,
    pub src_node: String,
    pub dst_node: String,
    pub route_type: String,
    pub partition_id: String,
    pub route_cost: f32,
}

#[derive(Debug, Clone)]
pub struct BlockExecutionResult {
    pub block_id: String,
    pub partition_id: String,
    pub node_id: String,
    /// Wall-clock milliseconds from graph execution start.
    pub start_time: u32,
    /// Wall-clock milliseconds from graph execution start.
    pub end_time: u32,
    pub value: PayloadValue,
    pub merge_mode: MergeMode,
    pub executor_name: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeResult {
    pub fused_graph_id: String,
    /// block_id → partition_id
    pub partitions: HashMap<String, String>,
    /// partition_id → node_id
    pub partition_binding: HashMap<String, String>,
    /// block_id → node_id
    pub block_binding: HashMap<String, String>,
    pub results: Vec<BlockExecutionResult>,
    pub packet_log: Vec<PacketLog>,
    pub merged: HashMap<String, String>,
    pub remote_sent_packets: usize,
    /// Values produced by leaf blocks (no outgoing edges).
    pub outputs: Vec<PayloadValue>,
}

// ─────────────────────────────────────────────────────────────────────────────
// URXRuntime
// ─────────────────────────────────────────────────────────────────────────────

pub struct URXRuntime<P: SchedulerPolicy> {
    pub nodes: HashMap<String, Node>,
    remote: RemotePacketLink,
    reservations: ReservationTable,
    policy: P,
    pub executors: ExecutorRegistry,
    /// ET-WCN optimizer: when set, replaces `bind_partitions` with SA search.
    et_policy: Option<ETCoolingPolicy>,
}

impl<P: SchedulerPolicy> URXRuntime<P> {
    pub fn new(nodes: Vec<Node>, policy: P) -> Self {
        let nodes = nodes.into_iter().map(|n| (n.node_id.clone(), n)).collect();
        Self {
            nodes,
            remote: RemotePacketLink::new(),
            reservations: ReservationTable::default(),
            policy,
            executors: ExecutorRegistry::new(),
            et_policy: None,
        }
    }

    pub fn set_et_policy(&mut self, p: ETCoolingPolicy) {
        self.et_policy = Some(p);
    }

    /// Pre-load a reservation into the table (e.g. for Demo C backfill testing).
    pub fn add_reservation(&mut self, r: Reservation) {
        self.reservations.add(r);
    }

    /// Workstation mode: auto-register executors based on node type.
    ///
    /// - `NodeType::Cpu`  → `ThreadPoolExecutor` with all logical cores
    /// - `NodeType::Gpu`  → `WgpuExecutor` (GPU feature) or ThreadPool fallback
    ///
    /// Call before `execute_graph`. Nodes without a type match keep the
    /// default `CpuExecutor`.
    pub fn enable_workstation_mode(&mut self) {
        let parallelism = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let tpool: Arc<dyn crate::executor::HardwareExecutor> =
            Arc::new(ThreadPoolExecutor::new(parallelism));

        let cpu_node_ids: Vec<String> = self.nodes.values()
            .filter(|n| n.node_type == NodeType::Cpu)
            .map(|n| n.node_id.clone())
            .collect();
        for nid in cpu_node_ids {
            self.executors.register(nid, Arc::clone(&tpool));
        }

        #[cfg(feature = "gpu")]
        {
            let gpu_node_ids: Vec<String> = self.nodes.values()
                .filter(|n| n.node_type == NodeType::Gpu)
                .map(|n| n.node_id.clone())
                .collect();
            if !gpu_node_ids.is_empty() {
                let gpu_exec = pollster::block_on(crate::gpu_executor::WgpuExecutor::new());
                match gpu_exec {
                    Ok(exec) => {
                        let shared: Arc<dyn crate::executor::HardwareExecutor> = Arc::new(exec);
                        for nid in gpu_node_ids {
                            self.executors.register(nid, Arc::clone(&shared));
                        }
                    }
                    Err(e) => {
                        eprintln!("[workstation] GPU init failed, falling back to thread-pool: {e}");
                        for nid in gpu_node_ids {
                            self.executors.register(nid, Arc::clone(&tpool));
                        }
                    }
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // execute_graph
    // ─────────────────────────────────────────────────────────────────────

    pub async fn execute_graph(&mut self, graph: &IRGraph) -> RuntimeResult {
        let graph_start = Instant::now();

        // ── 1. Fuse + partition ──────────────────────────────────────────
        let fused = fuse_linear_blocks(graph);
        let partition_map = partition_graph(&fused);

        let partition_binding = if let Some(ref et) = self.et_policy {
            et.optimise_binding(&fused, &partition_map, &self.nodes)
        } else {
            bind_partitions(&fused, &partition_map, &self.nodes, &self.policy)
        };

        // ── 2. Build Partition objects (from scheduler.rs) ───────────────
        let mut blocks_by_partition: HashMap<String, Vec<IRBlock>> = HashMap::new();
        for b in &fused.blocks {
            let pid = partition_map.get(&b.block_id).cloned().unwrap_or_default();
            blocks_by_partition.entry(pid).or_default().push(b.clone());
        }

        let partitions: HashMap<String, Partition> = partition_binding
            .iter()
            .map(|(pid, nid)| {
                let blocks = blocks_by_partition.remove(pid).unwrap_or_default();
                (pid.clone(), Partition::new(pid.clone(), blocks, nid.clone()))
            })
            .collect();

        // ── 3. Block → node binding ──────────────────────────────────────
        let mut block_binding: HashMap<String, String> = HashMap::new();
        for b in &fused.blocks {
            let pid = partition_map.get(&b.block_id).unwrap();
            let nid = partition_binding.get(pid).unwrap().clone();
            block_binding.insert(b.block_id.clone(), nid);
        }

        // ── 4. Reservation ──────────────────────────────────────────────
        let mut partition_slots: HashMap<String, (u32, u32)> = HashMap::new();
        let now_slot = 0u32;

        for (pid, nid) in &partition_binding {
            let partition = &partitions[pid];
            let est: u32 = partition.blocks.iter().map(|b| b.estimated_duration).sum();
            let est = est.max(1);

            let slot_start = self.reservations
                .earliest_start_time(nid, est, now_slot)
                .unwrap_or(now_slot);
            let slot_end = slot_start + est;

            partition_slots.insert(pid.clone(), (slot_start, slot_end));
            self.reservations.add(Reservation::new(
                pid.clone(),
                nid.clone(),
                slot_start,
                slot_end,
            ));
        }

        // ── 5. Pre-build index structures for O(1) lookups ───────────────
        // OPTIMIZATION: block_id -> block lookup (avoids O(n) linear scan per block)
        let block_index: HashMap<String, IRBlock> = fused.blocks.iter()
            .map(|b| (b.block_id.clone(), b.clone()))
            .collect();

        // OPTIMIZATION: src_block -> outgoing edges (avoids filtering all edges per block)
        let mut outgoing_edges: HashMap<String, Vec<crate::ir::IREdge>> = HashMap::new();
        for e in &fused.edges {
            outgoing_edges.entry(e.src_block.clone())
                .or_default()
                .push(e.clone());
        }

        // ── 6. Shared execution state (Arc<Mutex<>>) ────────────────────
        let mut init_inbox: HashMap<String, HashMap<String, PayloadValue>> = HashMap::new();
        for b in &fused.blocks {
            init_inbox.insert(b.block_id.clone(), HashMap::new());
        }

        let shared_inbox  = Arc::new(Mutex::new(init_inbox));
        let shared_log    = Arc::new(Mutex::new(Vec::<PacketLog>::new()));
        let _shared_rings = Arc::new(Mutex::new(HashMap::<(String, String), LocalRingTunnel>::new()));
        let shared_remote = Arc::new(Mutex::new(RemotePacketLink::new()));
        let shared_inertia: Arc<Mutex<Vec<(String, String)>>> =
            Arc::new(Mutex::new(Vec::new()));

        // Snapshot non-mutable runtime state for closure capture.
        let exec_reg    = self.executors.clone();
        let nodes_snap  = self.nodes.clone();
        let fused_arc   = Arc::new(fused);
        let bb_arc      = Arc::new(block_binding.clone());
        let pm_arc      = Arc::new(partition_map.clone());
        let bi_arc      = Arc::new(block_index);
        let oe_arc      = Arc::new(outgoing_edges);

        // ── 7. Build executor closure and run scheduler ──────────────────

        let partitions_vec: Vec<Partition> = partitions.into_values().collect();

        let (inbox_c, log_c, remote_c, inertia_c) = (
            Arc::clone(&shared_inbox),
            Arc::clone(&shared_log),
            Arc::clone(&shared_remote),
            Arc::clone(&shared_inertia),
        );
        let (fused_ec, bb_ec, pm_ec, bi_ec, oe_ec) = (
            Arc::clone(&fused_arc),
            Arc::clone(&bb_arc),
            Arc::clone(&pm_arc),
            Arc::clone(&bi_arc),
            Arc::clone(&oe_arc),
        );

        let exec_closure = move |partition: Partition| {
            let inbox_c   = Arc::clone(&inbox_c);
            let log_c     = Arc::clone(&log_c);
            let remote_c  = Arc::clone(&remote_c);
            let inertia_c = Arc::clone(&inertia_c);
            let exec_reg  = exec_reg.clone();
            let nodes_s   = nodes_snap.clone();
            let fused     = Arc::clone(&fused_ec);
            let bb        = Arc::clone(&bb_ec);
            let pm        = Arc::clone(&pm_ec);
            let bi        = Arc::clone(&bi_ec);   // block index
            let oe        = Arc::clone(&oe_ec);   // outgoing edges index
            let gs        = graph_start;

            async move {
                let block_order = intra_partition_topo(&partition.blocks, &fused);
                let mut part_results = Vec::with_capacity(block_order.len());
                let node_id  = partition.node_id.clone();
                let executor = exec_reg.get(&node_id);
                let executor_name = executor.name().to_string();
                let is_parallel = exec_reg.is_parallel(&node_id);

                for block_id in &block_order {
                    // OPTIMIZATION: O(1) block lookup instead of O(n) linear scan
                    let block = bi.get(block_id).unwrap().clone();

                    // Batch inbox read: lock once, get context, release
                    let ctx = {
                        let inbox = inbox_c.lock().await;
                        inbox.get(block_id).cloned().unwrap_or_default()
                    };

                    let block_start_ms = gs.elapsed().as_micros() as u32;

                    let value = if is_parallel {
                        let exec_c  = Arc::clone(&executor);
                        let block_c = block.clone();
                        let ctx_c   = ctx.clone();
                        tokio::task::spawn_blocking(move || exec_c.exec(&block_c, &ctx_c))
                            .await
                            .expect("executor task panicked")
                    } else {
                        executor.exec(&block, &ctx)
                    };

                    let block_end_ms = gs.elapsed().as_micros() as u32;

                    if let Some(key) = &block.inertia_key {
                        inertia_c.lock().await.push((node_id.clone(), key.clone()));
                    }

                    part_results.push(BlockExecutionResult {
                        block_id:      block_id.clone(),
                        partition_id:  partition.partition_id.clone(),
                        node_id:       node_id.clone(),
                        start_time:    block_start_ms,
                        end_time:      block_end_ms,
                        value:         value.clone(),
                        merge_mode:    block.merge_mode,
                        executor_name: executor_name.clone(),
                    });

                    // OPTIMIZATION: use pre-built outgoing edge index, O(1) instead of O(E)
                    if let Some(edges) = oe.get(block_id) {
                        // OPTIMIZATION: collect all updates, then batch-write with one lock each
                        let mut inbox_updates: Vec<(String, String, PayloadValue)> =
                            Vec::with_capacity(edges.len());
                        let mut log_updates: Vec<PacketLog> =
                            Vec::with_capacity(edges.len());

                        for e in edges {
                            let dst_node = bb.get(&e.dst_block).unwrap().clone();
                            // Borrow src/dst only long enough to extract owned values
                            // so no references cross the .await below.
                            let (is_local, cost, dst_addr) = {
                                let src_n = nodes_s.get(&node_id).unwrap();
                                let dst_n = nodes_s.get(&dst_node).unwrap();
                                (
                                    src_n.host_id == dst_n.host_id,
                                    route_cost(src_n, dst_n),
                                    dst_n.address.clone(),
                                )
                            };

                            // OPTIMIZATION: same-host routing skips encode/ring/decode entirely
                            let recv_value = if is_local {
                                value.clone()
                            } else {
                                let payload = PayloadCodec::encode(&value);
                                let packet  = URPPacket::build(
                                    6, block.merge_mode, &e.src_block, &e.dst_block, &payload,
                                );
                                // Real TCP when dst_node has a configured address; stub otherwise
                                let recv = if let Some(ref addr) = dst_addr {
                                    remote_c.lock().await
                                        .send(addr, packet).await
                                        .unwrap_or_else(|err| panic!(
                                            "TCP send to {addr} failed: {err}"
                                        ))
                                } else {
                                    remote_c.lock().await.send_legacy(packet).await
                                };
                                PayloadCodec::decode(recv.payload())
                            };

                            inbox_updates.push((
                                e.dst_block.clone(),
                                e.input_key.clone(),
                                recv_value,
                            ));
                            log_updates.push(PacketLog {
                                src_block:    e.src_block.clone(),
                                dst_block:    e.dst_block.clone(),
                                src_node:     node_id.clone(),
                                dst_node:     dst_node.clone(),
                                route_type:   if is_local { "local-direct" } else { "remote-packet" }
                                              .to_string(),
                                partition_id: pm.get(&e.dst_block).unwrap().clone(),
                                route_cost:   cost,
                            });
                        }

                        // Batch inbox write — one lock for all edges of this block
                        {
                            let mut inbox = inbox_c.lock().await;
                            for (block, key, val) in inbox_updates {
                                inbox.entry(block).or_default().insert(key, val);
                            }
                        }
                        // Batch log write — one lock for all edges of this block
                        log_c.lock().await.extend(log_updates);
                    }
                }

                part_results
            }
        };

        // OPTIMIZATION: use all logical cores as lane count for parallel partition execution
        let num_lanes = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let scheduler = PartitionDAGScheduler::new(num_lanes, num_lanes);
        let results = scheduler
            .schedule_and_execute(partitions_vec, &fused_arc, &partition_map, exec_closure)
            .await;

        // ── 8. Apply inertia updates and accumulate remote counter ───────
        for (node_id, key) in shared_inertia.lock().await.drain(..) {
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.remember_inertia_key(&key);
            }
        }
        self.remote.sent_packets += shared_remote.lock().await.sent_packets;

        // ── 9. Collect leaf outputs and merge ────────────────────────────
        let packet_log = Arc::try_unwrap(shared_log)
            .unwrap()
            .into_inner();

        let has_outgoing: HashSet<&str> = fused_arc
            .edges
            .iter()
            .map(|e| e.src_block.as_str())
            .collect();

        let outputs: Vec<PayloadValue> = results
            .iter()
            .filter(|r| !has_outgoing.contains(r.block_id.as_str()))
            .map(|r| r.value.clone())
            .collect();

        let mut grouped: HashMap<MergeMode, Vec<PayloadValue>> = HashMap::new();
        for r in &results {
            grouped.entry(r.merge_mode).or_default().push(r.value.clone());
        }

        RuntimeResult {
            fused_graph_id:       fused_arc.graph_id.clone(),
            partitions:           partition_map,
            partition_binding,
            block_binding,
            results,
            packet_log,
            merged:               run_reducers(&grouped),
            remote_sent_packets:  self.remote.sent_packets,
            outputs,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Topological sort of blocks within a single partition, using only the graph
/// edges whose both endpoints belong to this partition.
fn intra_partition_topo(blocks: &[IRBlock], graph: &IRGraph) -> Vec<String> {
    let ids: HashSet<&str> = blocks.iter().map(|b| b.block_id.as_str()).collect();
    let mut in_deg: HashMap<String, usize> =
        blocks.iter().map(|b| (b.block_id.clone(), 0)).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    for e in &graph.edges {
        if ids.contains(e.src_block.as_str()) && ids.contains(e.dst_block.as_str()) {
            adj.entry(e.src_block.clone()).or_default().push(e.dst_block.clone());
            *in_deg.get_mut(&e.dst_block).unwrap() += 1;
        }
    }

    let mut queue: VecDeque<String> = in_deg
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| k.clone())
        .collect();
    let mut order = Vec::new();

    while let Some(bid) = queue.pop_front() {
        order.push(bid.clone());
        for next in adj.get(&bid).cloned().unwrap_or_default() {
            let d = in_deg.get_mut(&next).unwrap();
            *d -= 1;
            if *d == 0 {
                queue.push_back(next);
            }
        }
    }

    order
}
