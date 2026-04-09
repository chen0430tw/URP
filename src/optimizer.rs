use std::collections::{HashMap, HashSet};

use crate::ir::{IRBlock, IREdge, IRGraph};

pub fn fuse_linear_blocks(graph: &IRGraph) -> IRGraph {
    let mut used = HashSet::new();
    let mut new_blocks = Vec::new();
    let mut map: HashMap<String, String> = HashMap::new();

    let ids: Vec<String> = graph.blocks.iter().map(|b| b.block_id.clone()).collect();
    let mut i = 0usize;

    while i < ids.len() {
        let a_id = &ids[i];
        if used.contains(a_id) {
            i += 1;
            continue;
        }

        let a = graph.get_block(a_id).unwrap();
        let mut fused = false;

        if i + 1 < ids.len() {
            let b_id = &ids[i + 1];
            if !used.contains(b_id) {
                let b = graph.get_block(b_id).unwrap();
                let direct_edges: Vec<_> = graph.edges.iter()
                    .filter(|e| e.src_block == *a_id && e.dst_block == *b_id)
                    .collect();

                // Only fuse when b needs no external inputs. If b has inputs,
                // they are named slots that must be filled via the inbox. After
                // fusion the a→b edge is removed, so any such slot would be
                // left unsatisfied at runtime.
                let compatible = a.resource_shape == b.resource_shape
                    && a.required_tag == b.required_tag
                    && a.preferred_zone == b.preferred_zone
                    && direct_edges.len() == 1
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
                    used.insert(a_id.clone());
                    used.insert(b_id.clone());
                    map.insert(a_id.clone(), fused_id.clone());
                    map.insert(b_id.clone(), fused_id);
                    fused = true;
                }
            }
        }

        if !fused {
            new_blocks.push(a.clone());
            used.insert(a_id.clone());
            map.insert(a_id.clone(), a_id.clone());
        }

        i += 1;
    }

    let mut new_edges = Vec::new();
    for e in &graph.edges {
        let s = map.get(&e.src_block).unwrap().clone();
        let d = map.get(&e.dst_block).unwrap().clone();
        if s == d {
            continue;
        }
        let candidate = IREdge {
            src_block: s,
            dst_block: d,
            output_key: e.output_key.clone(),
            input_key: e.input_key.clone(),
        };
        let exists = new_edges.iter().any(|x: &IREdge|
            x.src_block == candidate.src_block &&
            x.dst_block == candidate.dst_block &&
            x.output_key == candidate.output_key &&
            x.input_key == candidate.input_key
        );
        if !exists {
            new_edges.push(candidate);
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
