use std::collections::{HashMap, HashSet};

use crate::ir::{IRBlock, IREdge, IRGraph};

pub fn fuse_linear_blocks(graph: &IRGraph) -> IRGraph {
    // ── Pre-build O(1) lookup structures ────────────────────────────────────
    // Block index: O(B) build, O(1) lookup (replaces O(B) `get_block` per call)
    let block_map: HashMap<&str, &IRBlock> = graph.blocks.iter()
        .map(|b| (b.block_id.as_str(), b))
        .collect();

    // Edge count map: (src, dst) → count, O(E) build, O(1) lookup
    // Used to check whether exactly one edge connects a→b (fusion condition)
    let mut edge_count: HashMap<(&str, &str), usize> = HashMap::new();
    for e in &graph.edges {
        *edge_count.entry((e.src_block.as_str(), e.dst_block.as_str())).or_insert(0) += 1;
    }

    // ── Fusion pass ──────────────────────────────────────────────────────────
    let mut used  = HashSet::new();
    let mut new_blocks = Vec::new();
    let mut map: HashMap<String, String> = HashMap::new();

    let ids: Vec<&str> = graph.blocks.iter().map(|b| b.block_id.as_str()).collect();
    let mut i = 0usize;

    while i < ids.len() {
        let a_id = ids[i];
        if used.contains(a_id) {
            i += 1;
            continue;
        }

        // O(1) block lookup (was O(B))
        let a = block_map[a_id];
        let mut fused = false;

        if i + 1 < ids.len() {
            let b_id = ids[i + 1];
            if !used.contains(b_id) {
                let b = block_map[b_id];

                // O(1) edge-count check (was O(E) filter)
                let direct_count = edge_count
                    .get(&(a_id, b_id))
                    .copied()
                    .unwrap_or(0);

                // Only fuse when b needs no external inputs. If b has inputs,
                // they are named slots that must be filled via the inbox. After
                // fusion the a→b edge is removed, so any such slot would be
                // left unsatisfied at runtime.
                let compatible = a.resource_shape == b.resource_shape
                    && a.required_tag == b.required_tag
                    && a.preferred_zone == b.preferred_zone
                    && direct_count == 1
                    && b.inputs.is_empty();

                if compatible {
                    let fused_id = format!("{}+{}", a.block_id, b.block_id);
                    let fused_block = IRBlock {
                        block_id: fused_id.clone(),
                        opcode: b.opcode.clone(),
                        inputs: if b.inputs.is_empty() { a.inputs.clone() } else { b.inputs.clone() },
                        output: b.output.clone(),
                        required_tag: b.required_tag.clone(),
                        merge_mode: b.merge_mode,
                        resource_shape: b.resource_shape.clone(),
                        preferred_zone: b.preferred_zone.clone(),
                        inertia_key: b.inertia_key.clone().or_else(|| a.inertia_key.clone()),
                        estimated_duration: a.estimated_duration + b.estimated_duration,
                    };
                    new_blocks.push(fused_block);
                    used.insert(a_id);
                    used.insert(b_id);
                    map.insert(a.block_id.clone(), fused_id.clone());
                    map.insert(b.block_id.clone(), fused_id);
                    fused = true;
                }
            }
        }

        if !fused {
            new_blocks.push((*a).clone());
            used.insert(a_id);
            map.insert(a.block_id.clone(), a.block_id.clone());
        }

        i += 1;
    }

    // ── Remap edges, dedup in O(E) using HashSet ─────────────────────────────
    // Old code used `new_edges.iter().any(...)` → O(E²) worst case.
    // HashSet key = (src, dst, output_key, input_key) → O(E) total.
    let mut new_edges = Vec::new();
    let mut seen: HashSet<(String, String, String, String)> = HashSet::new();

    for e in &graph.edges {
        let s = map[&e.src_block].clone();
        let d = map[&e.dst_block].clone();
        if s == d {
            continue; // intra-fused edge: discard
        }
        let key = (s.clone(), d.clone(), e.output_key.clone(), e.input_key.clone());
        if seen.insert(key) {
            new_edges.push(IREdge {
                src_block:  s,
                dst_block:  d,
                output_key: e.output_key.clone(),
                input_key:  e.input_key.clone(),
            });
        }
    }

    IRGraph {
        graph_id: format!("{}_fused", graph.graph_id),
        blocks: new_blocks,
        edges: new_edges,
    }
}

pub fn partition_graph(graph: &IRGraph) -> HashMap<String, String> {
    let mut partitions = HashMap::new();
    let mut current = 0usize;
    let mut prev: Option<&IRBlock> = None;

    for block in &graph.blocks {
        match prev {
            None => {
                partitions.insert(block.block_id.clone(), format!("p{}", current));
            }
            Some(p) => {
                let same = p.required_tag == block.required_tag
                    && p.resource_shape == block.resource_shape
                    && p.preferred_zone == block.preferred_zone;
                if same {
                    partitions.insert(block.block_id.clone(), format!("p{}", current));
                } else {
                    current += 1;
                    partitions.insert(block.block_id.clone(), format!("p{}", current));
                }
            }
        }
        prev = Some(block);
    }

    partitions
}
