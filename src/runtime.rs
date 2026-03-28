use std::collections::HashMap;
use std::sync::Arc;

use crate::cost::route_cost;
use crate::executor::{eval_opcode, ExecutorRegistry};
use crate::ir::{IRGraph, MergeMode};
use crate::node::Node;
use crate::optimizer::{fuse_linear_blocks, partition_graph};
use crate::packet::{PayloadCodec, PayloadValue, URPPacket};
use crate::partition::bind_partitions;
use crate::policy::SchedulerPolicy;
use crate::reducer::run_reducers;
use crate::remote::RemotePacketLink;
use crate::reservation::ReservationTable;
use crate::ring::LocalRingTunnel;

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
    pub start_time: u32,
    pub end_time: u32,
    pub value: PayloadValue,
    pub merge_mode: MergeMode,
    pub executor_name: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeResult {
    pub fused_graph_id: String,
    pub partitions: HashMap<String, String>,
    pub partition_binding: HashMap<String, String>,
    pub block_binding: HashMap<String, String>,
    pub results: Vec<BlockExecutionResult>,
    pub packet_log: Vec<PacketLog>,
    pub merged: HashMap<String, String>,
    pub remote_sent_packets: usize,
    pub outputs: Vec<PayloadValue>,
}

pub struct URXRuntime<P: SchedulerPolicy> {
    pub nodes: HashMap<String, Node>,
    rings: HashMap<(String, String), LocalRingTunnel>,
    remote: RemotePacketLink,
    reservations: ReservationTable,
    policy: P,
    pub executors: ExecutorRegistry,
}

impl<P: SchedulerPolicy> URXRuntime<P> {
    pub fn new(nodes: Vec<Node>, policy: P) -> Self {
        let nodes = nodes.into_iter().map(|n| (n.node_id.clone(), n)).collect();
        Self {
            nodes,
            rings: HashMap::new(),
            remote: RemotePacketLink::new(),
            reservations: ReservationTable::default(),
            policy,
            executors: ExecutorRegistry::new(),
        }
    }

    fn ensure_ring(&mut self, src: &str, dst: &str) {
        let key = (src.to_string(), dst.to_string());
        self.rings.entry(key).or_insert_with(|| LocalRingTunnel::new(128));
    }

    fn route_kind(&self, src: &str, dst: &str) -> &'static str {
        let a = self.nodes.get(src).unwrap();
        let b = self.nodes.get(dst).unwrap();
        if a.host_id == b.host_id { "local-ring" } else { "remote-packet" }
    }

    /// Compute topological waves: each wave is a set of blocks with no
    /// dependencies on each other (all predecessors are in earlier waves).
    /// Blocks within the same wave can safely execute in parallel.
    fn topo_waves(&self, graph: &IRGraph) -> Vec<Vec<String>> {
        let mut indeg: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for b in &graph.blocks {
            indeg.insert(b.block_id.clone(), 0);
        }
        for e in &graph.edges {
            *indeg.get_mut(&e.dst_block).unwrap() += 1;
            adj.entry(e.src_block.clone()).or_default().push(e.dst_block.clone());
        }

        let mut waves = Vec::new();
        loop {
            let ready: Vec<String> = indeg.iter()
                .filter(|(_, d)| **d == 0)
                .map(|(k, _)| k.clone())
                .collect();
            if ready.is_empty() { break; }
            for id in &ready {
                indeg.remove(id);
                if let Some(nexts) = adj.get(id) {
                    for nxt in nexts {
                        *indeg.get_mut(nxt).unwrap() -= 1;
                    }
                }
            }
            waves.push(ready);
        }
        waves
    }

    pub async fn execute_graph(&mut self, graph: &IRGraph) -> RuntimeResult {
        let fused = fuse_linear_blocks(graph);
        let partitions = partition_graph(&fused);
        let partition_binding = bind_partitions(&fused, &partitions, &self.nodes, &self.policy);

        for (pid, nid) in &partition_binding {
            self.reservations.add(crate::reservation::Reservation::new(
                pid.clone(),
                nid.clone(),
                0,
                10,
            ));
        }

        let mut block_binding = HashMap::new();
        for b in &fused.blocks {
            let pid = partitions.get(&b.block_id).unwrap();
            let nid = partition_binding.get(pid).unwrap().clone();
            block_binding.insert(b.block_id.clone(), nid);
        }

        let waves = self.topo_waves(&fused);
        let mut inbox: HashMap<String, HashMap<String, PayloadValue>> = HashMap::new();
        let mut results = Vec::new();
        let mut packet_log = Vec::new();

        for b in &fused.blocks {
            inbox.insert(b.block_id.clone(), HashMap::new());
        }

        for wave in waves {
            // ── Determine if any block in this wave uses a parallel executor ──
            let any_parallel = wave.iter().any(|bid| {
                let node_id = block_binding.get(bid).unwrap();
                self.executors.is_parallel(node_id)
            });

            // ── Execute all blocks in this wave ──────────────────────────────
            // Parallel path: spawn one blocking task per block, join all.
            // Sequential path: run blocks one after another (CpuExecutor default).
            let wave_values: Vec<(String, PayloadValue, String)> = if any_parallel {
                // Clone data needed for spawn_blocking (requires 'static)
                let tasks: Vec<_> = wave.iter().map(|block_id| {
                    let block = fused.blocks.iter().find(|b| b.block_id == *block_id).unwrap().clone();
                    let ctx = inbox.get(block_id).unwrap().clone();
                    let node_id = block_binding.get(block_id).unwrap().clone();
                    let executor = self.executors.get(&node_id);
                    let bid = block_id.clone();
                    let exec_name = executor.name().to_string();
                    tokio::task::spawn_blocking(move || {
                        let value = executor.exec(&block, &ctx);
                        (bid, value, exec_name)
                    })
                }).collect();

                let mut values = Vec::new();
                for task in tasks {
                    values.push(task.await.expect("executor task panicked"));
                }
                values
            } else {
                wave.iter().map(|block_id| {
                    let block = fused.blocks.iter().find(|b| b.block_id == *block_id).unwrap();
                    let ctx = inbox.get(block_id).unwrap();
                    let node_id = block_binding.get(block_id).unwrap();
                    let executor = self.executors.get(node_id);
                    let value = executor.exec(block, ctx);
                    (block_id.clone(), value, executor.name().to_string())
                }).collect()
            };

            // ── Post-wave: update inertia, record results, route packets ─────
            for (block_id, value, exec_name) in wave_values {
                let block = fused.blocks.iter().find(|b| b.block_id == block_id).unwrap();
                let node_id = block_binding.get(&block_id).unwrap().clone();

                if let Some(key) = &block.inertia_key {
                    if let Some(node) = self.nodes.get_mut(&node_id) {
                        node.remember_inertia_key(key);
                    }
                }

                results.push(BlockExecutionResult {
                    block_id: block.block_id.clone(),
                    partition_id: partitions.get(&block.block_id).unwrap().clone(),
                    node_id: node_id.clone(),
                    start_time: 0,
                    end_time: 0,
                    value: value.clone(),
                    merge_mode: block.merge_mode,
                    executor_name: exec_name,
                });

                for e in fused.edges.iter().filter(|e| e.src_block == block.block_id) {
                    let dst_node = block_binding.get(&e.dst_block).unwrap().clone();
                    let payload = PayloadCodec::encode(&value);
                    let packet = URPPacket::build(6, block.merge_mode, &e.src_block, &e.dst_block, &payload);
                    let route = self.route_kind(&node_id, &dst_node).to_string();

                    let cost = {
                        let a = self.nodes.get(&node_id).unwrap();
                        let b = self.nodes.get(&dst_node).unwrap();
                        route_cost(a, b)
                    };

                    let recv_value = if route == "local-ring" {
                        self.ensure_ring(&node_id, &dst_node);
                        let key = (node_id.clone(), dst_node.clone());
                        let ring = self.rings.get_mut(&key).unwrap();
                        ring.push(packet).await;
                        let recv = ring.pop().await;
                        PayloadCodec::decode(recv.payload())
                    } else {
                        let recv = self.remote.send_legacy(packet).await;
                        PayloadCodec::decode(recv.payload())
                    };

                    inbox.entry(e.dst_block.clone()).or_default()
                        .insert(e.input_key.clone(), recv_value);

                    packet_log.push(PacketLog {
                        src_block: e.src_block.clone(),
                        dst_block: e.dst_block.clone(),
                        src_node: node_id.clone(),
                        dst_node: dst_node.clone(),
                        route_type: route,
                        partition_id: partitions.get(&e.dst_block).unwrap().clone(),
                        route_cost: cost,
                    });
                }
            }
        }

        let mut grouped: HashMap<MergeMode, Vec<PayloadValue>> = HashMap::new();
        for r in &results {
            grouped.entry(r.merge_mode).or_default().push(r.value.clone());
        }

        RuntimeResult {
            fused_graph_id: fused.graph_id.clone(),
            partitions,
            partition_binding,
            block_binding,
            results,
            packet_log,
            merged: run_reducers(&grouped),
            remote_sent_packets: self.remote.sent_packets,
            outputs: Vec::new(),
        }
    }
}
